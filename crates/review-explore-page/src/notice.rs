use crate::command::CommandRefusal;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Notice {
    pub(crate) action: Action,
    pub(crate) problem: Problem,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Answer { number: Option<usize> },
    Start,
    Pick { number: Option<usize> },
    Implement,
    Quiz,
    Message,
    RetryMessages,
    Recover(RecoveryAction),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryAction {
    Stop,
    Retry,
    CancelAnswer,
    Reset,
    CancelImplementation,
}

impl RecoveryAction {
    fn name(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Retry => "retry",
            Self::CancelAnswer => "cancel-answer",
            Self::Reset => "reset",
            Self::CancelImplementation => "cancel-implementation",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Problem {
    Stale,
    Failed(String),
    NoReply,
}

impl From<CommandRefusal> for Problem {
    fn from(refusal: CommandRefusal) -> Self {
        match refusal {
            CommandRefusal::Stale | CommandRefusal::AlreadyApplied => Self::Stale,
            CommandRefusal::Failed(reason) => Self::Failed(reason),
        }
    }
}

impl Action {
    pub(crate) fn question_number(self) -> Option<usize> {
        match self {
            Self::Answer { number } | Self::Pick { number } => number,
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Answer { .. } => "answer",
            Self::Start => "start",
            Self::Pick { .. } => "pick",
            Self::Implement => "implement",
            Self::Quiz => "quiz",
            Self::Message => "send-message",
            Self::RetryMessages => "retry-messages",
            Self::Recover(recovery) => recovery.name(),
        }
    }
}

impl Notice {
    pub(crate) fn code(&self) -> i32 {
        match self.problem {
            Problem::Stale => crate::rpc::RpcError::STALE,
            Problem::Failed(_) => crate::rpc::RpcError::FAILED,
            Problem::NoReply => crate::rpc::RpcError::NO_REPLY,
        }
    }

    pub(crate) fn message(&self) -> String {
        match &self.problem {
            Problem::Stale => format!("{} is stale", self.action.name()),
            Problem::Failed(reason) => reason.clone(),
            Problem::NoReply => "The review tool did not reply".to_owned(),
        }
    }

    pub(crate) fn new(action: Action, problem: Problem) -> Self {
        Self { action, problem }
    }
}
