use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use codex_daemon::{Incoming, Replier, Rpc};
use review_run_ahead::{ForkEnd, RunAheadLog, TokenUsage};
use serde_json::Value;

use crate::approvals;

pub(crate) struct LiveFork {
    pub(crate) rpc: Rpc,
    pub(crate) turn: Arc<TurnWatch>,
}

#[derive(Default)]
pub(crate) struct TurnWatch {
    state: Mutex<TurnState>,
    changed: Condvar,
}

#[derive(Default)]
struct TurnState {
    session: Option<String>,
    id: Option<String>,
    ended: Option<Box<dyn FnOnce(ForkEnd) + Send>>,
    over: bool,
    usage: TokenUsage,
}

impl TurnWatch {
    fn lock(&self) -> MutexGuard<'_, TurnState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn forked(&self, session: &str) {
        self.lock().session = Some(session.to_owned());
    }

    pub(crate) fn starting(&self, ended: Box<dyn FnOnce(ForkEnd) + Send>) {
        self.lock().ended = Some(ended);
    }

    pub(crate) fn started(&self, id: &str) {
        self.lock().id = Some(id.to_owned());
    }

    pub(crate) fn id(&self) -> Option<String> {
        self.lock().id.clone()
    }

    pub(crate) fn wait_over(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let mut state = self.lock();
        while !state.over {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }

    pub(crate) fn handler(
        self: &Arc<Self>,
        log: RunAheadLog,
    ) -> impl FnMut(Incoming, &Replier) + Send + 'static {
        let watch = Arc::clone(self);
        move |incoming, replier| match incoming {
            Incoming::Request { id, method, params } => {
                let answer = approvals::answer(&method, &params);
                if let Some(what) = answer.declined {
                    let session = watch.lock().session.clone().unwrap_or_default();
                    log.write(&format!("the fork {session} was refused {what}"));
                }
                replier.reply(&id, answer.result);
            }
            Incoming::Notification { method, params } => {
                let session = params.get("threadId").and_then(Value::as_str);
                if session.is_some() && watch.lock().session.as_deref() == session {
                    watch.hear(&method, &params);
                }
            }
            Incoming::Lost(error) => watch.end(error, false),
        }
    }

    fn hear(&self, method: &str, params: &Value) {
        match method {
            "thread/tokenUsage/updated" => {
                if let Some(last) = params.pointer("/tokenUsage/last") {
                    self.lock().usage += usage(last);
                }
            }
            "turn/completed" => {
                let turn = params.get("turn").cloned().unwrap_or_default();
                let id = turn.get("id").and_then(Value::as_str);
                if self
                    .lock()
                    .id
                    .as_deref()
                    .is_some_and(|ours| Some(ours) != id)
                {
                    return;
                }
                let (exit, finished) = match turn.get("status").and_then(Value::as_str) {
                    Some("completed") => ("its turn completed".to_owned(), true),
                    Some("interrupted") => ("its turn was interrupted".to_owned(), false),
                    _ => (
                        format!(
                            "its turn failed: {}",
                            turn.pointer("/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("Codex gave no reason")
                        ),
                        false,
                    ),
                };
                self.end(exit, finished);
            }
            _ => {}
        }
    }

    fn end(&self, exit: String, finished: bool) {
        let mut state = self.lock();
        state.over = true;
        let ended = state.ended.take();
        let usage = state.usage;
        drop(state);
        self.changed.notify_all();
        if let Some(ended) = ended {
            ended(ForkEnd {
                exit,
                usage,
                finished,
            });
        }
    }
}

// Codex's input tokens include those read from the prompt cache (`cachedInputTokens`) and
// those written to it.
fn usage(last: &Value) -> TokenUsage {
    let count = |name: &str| last.get(name).and_then(Value::as_u64).unwrap_or(0);
    let (cache_read, cache_creation) = (count("cachedInputTokens"), count("cacheWriteInputTokens"));
    TokenUsage {
        input: count("inputTokens").saturating_sub(cache_read + cache_creation),
        cache_creation,
        cache_read,
        output: count("outputTokens"),
    }
}

#[cfg(test)]
#[path = "live.tests.rs"]
mod tests;
