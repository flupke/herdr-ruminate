mod bridge;
mod endpoint;
mod handler;
mod instructions;
mod server;

use review_threads::{MessageId, Post, ReviewThread, ThreadId};
use tokio::sync::oneshot;

pub use bridge::serve_stdio;
pub use endpoint::Endpoint;
pub use server::Server;

#[derive(Debug)]
pub enum Operation {
    ListThreads,
    GetThread(ThreadId),
    GetNewMessages,
    Reply(Box<Post>),
    SubmitQuestion(Box<review_explore::InterviewUpdate>),
    SubmitConclusion(Box<review_explore::ConclusionSubmission>),
    ReviseQuestion(Box<review_explore::QuestionRevision>),
}

#[derive(Debug)]
pub struct Request {
    pub access: String,
    pub operation: Operation,
    // Codex names its session in `_meta.threadId`; Claude Code names none.
    pub agent_session: Option<String>,
    response: oneshot::Sender<Result<Response, String>>,
}

impl Operation {
    pub fn belongs_to_explore(&self) -> bool {
        matches!(
            self,
            Self::SubmitQuestion(_) | Self::SubmitConclusion(_) | Self::ReviseQuestion(_)
        )
    }
}

pub type PendingResponse = oneshot::Receiver<Result<Response, String>>;

impl Request {
    pub fn new(access: String, operation: Operation) -> (Self, PendingResponse) {
        let (response, received) = oneshot::channel();
        (
            Self {
                access,
                operation,
                agent_session: None,
                response,
            },
            received,
        )
    }

    pub fn respond(self, result: Result<Response, String>) {
        let _ = self.response.send(result);
    }
}

#[derive(Debug)]
pub enum Response {
    Threads(Vec<ReviewThread>),
    NewMessages(Vec<ReviewThread>),
    Posted(MessageId),
    Explore {
        applied: bool,
        shown_as: Option<review_types::QuestionNumber>,
    },
}

#[cfg(test)]
mod tests;
