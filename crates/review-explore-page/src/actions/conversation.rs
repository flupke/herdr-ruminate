use review_threads::{AskedUnder, MessageId, Post, ThreadCommand};
use review_types::ReviewUnit;

use super::{Actions, crlf_to_lf};
use crate::Rounds;
use crate::conversation::{PageConversation, ThreadsSnapshot};
use crate::notice::{Action, Notice, Problem};
use crate::rpc::{ConversationCall, MessageParams, Outcome, ReadParams, RetryMessagesParams};

struct Shown<'a> {
    unit: ReviewUnit,
    link: &'a PageConversation,
    threads: ThreadsSnapshot,
    writable: bool,
}

impl<R: Rounds> Actions<'_, R> {
    pub(super) async fn in_conversation(&self, call: ConversationCall) -> Result<Outcome, Notice> {
        match call {
            ConversationCall::SendMessage(params) => self.send_message(params).await,
            ConversationCall::ReadMessages(params) => self.read_messages(params).await,
            ConversationCall::RetryMessages(params) => self.retry_messages(params).await,
        }
    }

    fn shown_conversation(&self, round: &str) -> Option<Shown<'_>> {
        let link = self.round.conversation.as_ref()?;
        let snapshot = self.round.stages.latest();
        let (shown, unit) = snapshot.conversation_round()?;
        if shown != round {
            return None;
        }
        Some(Shown {
            unit: unit.clone(),
            link,
            threads: link.threads.latest(),
            writable: !snapshot.earlier,
        })
    }

    async fn send_message(&self, params: MessageParams) -> Result<Outcome, Notice> {
        let failed = |reason: &str| Notice::new(Action::Message, Problem::Failed(reason.into()));
        let id = MessageId::parse(&params.id).map_err(|error| failed(&error))?;
        let text = crlf_to_lf(params.text);
        if text.trim().is_empty() {
            return Err(failed("A message cannot be empty"));
        }
        if self.posted(&id) {
            return Ok(Outcome::applied(false));
        }
        let shown = self
            .shown_conversation(&params.round)
            .filter(|shown| shown.writable)
            .ok_or_else(|| Notice::new(Action::Message, Problem::Stale))?;
        let quote = params.quote.filter(|quote| !quote.trim().is_empty());
        let asked_under = params.asked_under.map(|asked| self.numbered(asked));
        let post = Post::to_round(&params.round, text, asked_under, quote).with_id(id);
        let command = ThreadCommand::Post {
            review_unit: shown.unit,
            post,
        };
        send(shown.link, command, Action::Message).await
    }

    fn numbered(&self, asked: AskedUnder) -> AskedUnder {
        match asked {
            AskedUnder::Question {
                question, version, ..
            } => AskedUnder::Question {
                number: self
                    .round
                    .stages
                    .latest()
                    .number_of_question(&question, version)
                    .map(Into::into),
                question,
                version,
            },
            other => other,
        }
    }

    fn posted(&self, id: &MessageId) -> bool {
        let Some(link) = self.round.conversation.as_ref() else {
            return false;
        };
        let snapshot = self.round.stages.latest();
        snapshot
            .review_unit
            .as_ref()
            .is_some_and(|unit| link.threads.latest().holds(unit, id))
    }

    async fn read_messages(&self, params: ReadParams) -> Result<Outcome, Notice> {
        let Some(shown) = self.shown_conversation(&params.round) else {
            return Ok(Outcome::default());
        };
        let Some(thread) = shown
            .threads
            .threads(&shown.unit)
            .and_then(|threads| threads.round_conversation(&params.round))
        else {
            return Ok(Outcome::default());
        };
        let command = ThreadCommand::MarkRead {
            review_unit: shown.unit.clone(),
            thread_id: thread.id.clone(),
            through: params.through,
        };
        send(shown.link, command, Action::Message).await
    }

    async fn retry_messages(&self, params: RetryMessagesParams) -> Result<Outcome, Notice> {
        let stale = || Notice::new(Action::RetryMessages, Problem::Stale);
        let shown = self
            .shown_conversation(&params.round)
            .filter(|shown| shown.writable)
            .ok_or_else(stale)?;
        let thread = shown
            .threads
            .threads(&shown.unit)
            .and_then(|threads| {
                let thread = threads.round_conversation(&params.round)?;
                threads.retry(&thread.id).ok().map(|()| thread.id.clone())
            })
            .ok_or_else(stale)?;
        let command = ThreadCommand::Retry {
            review_unit: shown.unit.clone(),
            thread_id: thread,
        };
        send(shown.link, command, Action::RetryMessages).await
    }
}

async fn send(
    link: &PageConversation,
    command: ThreadCommand,
    action: Action,
) -> Result<Outcome, Notice> {
    match link.sender.send(command).await {
        Ok(applied) => Ok(Outcome::applied(applied)),
        Err(problem) => Err(Notice::new(action, problem)),
    }
}
