use review_threads::AskedUnder;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::status::StatusCard;
use crate::view::PageView;

#[derive(Debug, Serialize, TS)]
#[serde(tag = "method", content = "params", rename_all = "kebab-case")]
pub(crate) enum Notification {
    State(Box<StateParams>),
    Ping,
}

#[derive(Debug, Serialize, TS)]
pub(crate) struct StateParams {
    pub(crate) epoch: String,
    pub(crate) seq: Seq,
    pub(crate) view: PageView,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, TS)]
pub(crate) struct Seq {
    pub(crate) revision: u64,
    pub(crate) picks: u64,
    pub(crate) threads: u64,
    pub(crate) rounds: u64,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct Request {
    pub(crate) id: u64,
    #[serde(flatten)]
    pub(crate) call: Call,
}

#[derive(Debug, Deserialize, TS)]
#[serde(untagged)]
pub(crate) enum Call {
    Round(RoundCall),
    Conclusion(ConclusionCall),
    Conversation(ConversationCall),
}

#[derive(Debug, Deserialize, TS)]
#[serde(tag = "method", content = "params", rename_all = "kebab-case")]
pub(crate) enum RoundCall {
    Answer(AnswerParams),
    Pick(PickParams),
    Start(StartParams),
    Stop(StopParams),
    Retry(RetryParams),
    CancelAnswer(CancelAnswerParams),
    Reset(ResetParams),
    DiagramFailed(DiagramParams),
    Ping,
}

#[derive(Debug, Deserialize, TS)]
#[serde(tag = "method", content = "params", rename_all = "kebab-case")]
pub(crate) enum ConclusionCall {
    Implement(ImplementParams),
    Quiz(QuizParams),
    QuizSkip(QuizSkipParams),
    CancelImplementation(CancelImplementationParams),
    ResendImplementation(ResendImplementationParams),
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct AnswerParams {
    pub(crate) round: Option<String>,
    pub(crate) question: String,
    pub(crate) version: u32,
    pub(crate) choice: Option<String>,
    pub(crate) comment: String,
    #[serde(default)]
    pub(crate) drawing_board: bool,
    #[serde(default)]
    pub(crate) number: Option<usize>,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct PickParams {
    pub(crate) round: Option<String>,
    pub(crate) question: String,
    pub(crate) version: u32,
    pub(crate) choice: String,
    #[serde(default)]
    pub(crate) number: Option<usize>,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct StartParams {
    pub(crate) challenger: bool,
    #[serde(default)]
    pub(crate) others: bool,
    #[serde(default)]
    pub(crate) reviewer: String,
    pub(crate) start: String,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct StopParams {
    #[serde(default)]
    pub(crate) start: Option<String>,
    #[serde(default)]
    pub(crate) request: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct RetryParams {
    pub(crate) request: String,
    pub(crate) attempt: String,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct CancelAnswerParams {
    pub(crate) answer: String,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct ResetParams {
    pub(crate) round: String,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct ImplementParams {
    pub(crate) conclusion: String,
    pub(crate) replaces: Option<String>,
    pub(crate) text: String,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct QuizParams {
    pub(crate) conclusion: String,
    pub(crate) item: usize,
    pub(crate) answer: usize,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct QuizSkipParams {
    pub(crate) conclusion: String,
}

#[derive(Debug, Deserialize, TS)]
#[serde(tag = "method", content = "params", rename_all = "kebab-case")]
pub(crate) enum ConversationCall {
    SendMessage(MessageParams),
    ReadMessages(ReadParams),
    RetryMessages(RetryMessagesParams),
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct MessageParams {
    pub(crate) round: String,
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) asked_under: Option<AskedUnder>,
    pub(crate) quote: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct ReadParams {
    pub(crate) round: String,
    pub(crate) through: u64,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct RetryMessagesParams {
    pub(crate) round: String,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct CancelImplementationParams {
    pub(crate) delivery: String,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct ResendImplementationParams {
    pub(crate) conclusion: String,
    pub(crate) delivery: String,
    pub(crate) attempt: String,
}

#[derive(Debug, Deserialize, TS)]
pub(crate) struct DiagramParams {
    pub(crate) question: String,
    pub(crate) version: u32,
    pub(crate) source: String,
    pub(crate) message: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(untagged)]
pub(crate) enum Reply {
    Result { id: u64, result: Outcome },
    Error { id: u64, error: RpcError },
}

#[derive(Debug, Default, Serialize, TS)]
pub(crate) struct Outcome {
    pub(crate) applied: bool,
    pub(crate) reopen: Option<String>,
}

impl Outcome {
    pub(crate) fn applied(applied: bool) -> Self {
        Self {
            applied,
            reopen: None,
        }
    }
}

#[derive(Debug, Serialize, TS)]
pub(crate) struct RpcError {
    pub(crate) code: i32,
    pub(crate) message: String,
    pub(crate) data: Option<StatusCard>,
}

impl RpcError {
    pub(crate) const INVALID_REQUEST: i32 = -32600;
    pub(crate) const STALE: i32 = 409;
    pub(crate) const FAILED: i32 = 500;
    pub(crate) const NO_REPLY: i32 = 504;
}
