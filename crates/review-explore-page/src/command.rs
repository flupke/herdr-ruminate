use std::sync::Arc;
use std::time::Duration;

use review_explore::{AnswerInput, DiagramError, QuizResponse};
use tokio::sync::oneshot;

use crate::notice::Problem;

const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum PageCommand {
    Answer(PageAnswer),
    DiagramFailed(DiagramError),
    Pick {
        question: String,
        version: u32,
    },
    Start {
        choices: review_explore::RoundChoices,
        start: String,
    },
    Implement(PageImplement),
    Quiz(PageQuizResponse),
    Recover(Recovery),
}

#[derive(Debug)]
pub enum Recovery {
    Stop(Waiting),
    Retry {
        request: String,
        attempt: String,
    },
    CancelAnswer {
        answer: String,
    },
    Reset {
        round: String,
    },
    CancelImplementation {
        delivery: String,
    },
    ResendImplementation {
        conclusion: String,
        delivery: String,
        attempt: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Waiting {
    Start(String),
    Turn(String),
}

#[derive(Debug)]
pub struct PageQuizResponse {
    pub conclusion: String,
    pub response: QuizResponse,
}

#[derive(Debug)]
pub struct PageAnswer {
    pub question: String,
    pub version: u32,
    pub input: AnswerInput,
}

#[derive(Debug)]
pub struct PageImplement {
    pub conclusion: String,
    pub replaces: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandRefusal {
    Stale,
    Failed(String),
    AlreadyApplied,
}

impl CommandRefusal {
    pub fn stale_unless_repeat(repeat: bool) -> Self {
        if repeat {
            Self::AlreadyApplied
        } else {
            Self::Stale
        }
    }
}

#[derive(Debug)]
pub struct CommandReply(oneshot::Sender<Result<(), CommandRefusal>>);

impl CommandReply {
    pub fn channel() -> (Self, oneshot::Receiver<Result<(), CommandRefusal>>) {
        let (reply, replied) = oneshot::channel();
        (Self(reply), replied)
    }

    pub fn send(self, result: Result<(), CommandRefusal>) {
        let _ = self.0.send(result);
    }
}

#[derive(Clone)]
pub struct CommandSender(Arc<dyn Fn(PageCommand, CommandReply) + Send + Sync>);

impl CommandSender {
    pub fn new(deliver: impl Fn(PageCommand, CommandReply) + Send + Sync + 'static) -> Self {
        Self(Arc::new(deliver))
    }

    pub(crate) async fn send(&self, command: PageCommand) -> Result<bool, Problem> {
        let (reply, replied) = CommandReply::channel();
        (self.0)(command, reply);
        await_reply(replied).await
    }
}

pub(crate) async fn await_reply(
    replied: oneshot::Receiver<Result<(), CommandRefusal>>,
) -> Result<bool, Problem> {
    match tokio::time::timeout(REPLY_TIMEOUT, replied).await {
        Ok(Ok(Ok(()))) => Ok(true),
        Ok(Ok(Err(CommandRefusal::AlreadyApplied))) => Ok(false),
        Ok(Ok(Err(refusal))) => Err(Problem::from(refusal)),
        Ok(Err(_)) | Err(_) => Err(Problem::NoReply),
    }
}
