use std::sync::mpsc;
use std::time::Duration;

use review_run_ahead::DiscardReason;
use review_thread_service::RoundMessage;
use review_threads::MessageId;

use super::{Event, RunAheadInput, RunAheadState};
use crate::{ExploreSession, Input};

pub const TALK_QUIET: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(super) struct Talk {
    messages: Vec<MessageId>,
    quiet: Option<QuietWait>,
    unreplied_logged: bool,
}

struct QuietWait {
    number: u64,
    _stop: mpsc::Sender<()>,
}

impl Talk {
    pub(super) fn waiting_for(messages: Vec<MessageId>) -> Option<Self> {
        (!messages.is_empty()).then(|| Self {
            messages,
            ..Self::default()
        })
    }

    pub(super) fn interrupt(&mut self) {
        self.quiet = None;
    }
}

impl RunAheadState {
    fn quiet_wait(&self) -> Option<&QuietWait> {
        self.armed.as_ref()?.talk.as_ref()?.quiet.as_ref()
    }
}

impl ExploreSession {
    pub(super) fn run_ahead_idle(&mut self) {
        if self
            .run_ahead
            .armed
            .as_ref()
            .is_some_and(|armed| armed.talk.is_some())
        {
            self.talk_idle();
        } else {
            self.run_ahead_take();
        }
    }

    pub(crate) fn run_ahead_round_message(&mut self, message: &RoundMessage) {
        let Some(armed) = self.run_ahead.armed.as_mut().filter(|armed| {
            armed.asked.round.unit == message.review_unit
                && armed.asked.round.instance == message.round
        }) else {
            return;
        };
        let talk = armed.talk.get_or_insert_default();
        talk.messages.push(message.message.clone());
        talk.quiet = None;
        talk.unreplied_logged = false;
        self.run_ahead.log(
            "the reviewer wrote in the round conversation: forks are taken again once it is quiet",
        );
        self.discard_taken(DiscardReason::ChatMessage);
    }

    fn talk_idle(&mut self) {
        let Some(armed) = self.run_ahead.armed.as_ref() else {
            return;
        };
        let Some(talk) = armed.talk.as_ref().filter(|talk| talk.quiet.is_none()) else {
            return;
        };
        let round = &armed.asked.round;
        let replied = self
            .rounds
            .unanswered_round_messages(&round.unit, &round.instance)
            .map(|unanswered| {
                !unanswered
                    .iter()
                    .any(|(message, _)| talk.messages.contains(message))
            });
        match replied {
            Ok(true) => {}
            Ok(false) => {
                if let Some(talk) = self
                    .run_ahead
                    .armed
                    .as_mut()
                    .and_then(|armed| armed.talk.as_mut())
                    .filter(|talk| !talk.unreplied_logged)
                {
                    talk.unreplied_logged = true;
                    self.run_ahead.log(
                        "the agent is idle but has not replied in the round conversation: no \
                         forks until it does",
                    );
                }
                return;
            }
            Err(error) => {
                self.run_ahead.log(&format!(
                    "the agent's replies in the round conversation are unknown: {error}"
                ));
                return;
            }
        }
        self.run_ahead.quiet_waits += 1;
        let quiet = self.start_quiet_wait(self.run_ahead.quiet_waits);
        let wait = self.run_ahead.talk_quiet;
        self.run_ahead.log(&format!(
            "the agent replied in the round conversation: forks are taken once it is quiet for {}s",
            wait.as_secs()
        ));
        if let Some(talk) = self
            .run_ahead
            .armed
            .as_mut()
            .and_then(|armed| armed.talk.as_mut())
        {
            talk.quiet = Some(quiet);
        }
    }

    fn start_quiet_wait(&self, number: u64) -> QuietWait {
        let (stop, stopped) = mpsc::channel::<()>();
        let (inbox, wait) = (self.inbox.clone(), self.run_ahead.talk_quiet);
        std::thread::spawn(move || {
            if let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(wait) {
                inbox.deliver(Input::RunAhead(RunAheadInput(Event::Quiet { number })));
            }
        });
        QuietWait {
            number,
            _stop: stop,
        }
    }

    pub(super) fn run_ahead_quiet(&mut self, number: u64) {
        if self
            .run_ahead
            .quiet_wait()
            .is_none_or(|quiet| quiet.number != number)
        {
            return;
        }
        if let Some(armed) = self.run_ahead.armed.as_mut() {
            armed.talk = None;
        }
        self.run_ahead
            .log("the round conversation is quiet: taking the forks again");
        self.run_ahead_take();
    }

    #[cfg(test)]
    pub(crate) fn quiet_wait_runs(&self) -> bool {
        self.run_ahead.quiet_wait().is_some()
    }

    #[cfg(test)]
    pub(crate) fn end_quiet_wait(&mut self) {
        let number = self
            .run_ahead
            .quiet_wait()
            .expect("a quiet wait runs")
            .number;
        self.handle(Input::RunAhead(RunAheadInput(Event::Quiet { number })));
    }

    #[cfg(test)]
    pub(crate) fn quiet_for(&mut self, wait: Duration) {
        self.run_ahead.talk_quiet = wait;
    }
}
