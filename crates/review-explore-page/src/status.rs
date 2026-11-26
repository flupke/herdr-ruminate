use review_explore::StartBlock;
use serde::Serialize;
use ts_rs::TS;

use crate::notice::{Action, Notice, Problem, RecoveryAction};
use crate::round::{
    ImplementationState, Interruption, PageImplementation, RoundSnapshot, RoundStage,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum StatusKind {
    Progress,
    Info,
    Warn,
    Danger,
    Ok,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum StatusRole {
    Status,
    Alert,
    Note,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum ButtonTier {
    Primary,
    Secondary,
}

#[derive(Debug, Serialize, TS)]
pub(crate) struct StatusCard {
    kind: StatusKind,
    id: &'static str,
    role: StatusRole,
    title: String,
    time: Option<CardTime>,
    reason: Option<String>,
    since: Option<CardTime>,
    code: bool,
    imperative: Option<&'static str>,
    next: Option<&'static str>,
    actions: Vec<StatusAction>,
}

#[derive(Debug, Serialize, TS)]
struct StatusAction {
    method: &'static str,
    fields: Vec<Field>,
    label: &'static str,
    tier: ButtonTier,
    hint: Option<&'static str>,
}

#[derive(Debug, Serialize, TS)]
struct CardTime {
    words: &'static str,
    ms: u64,
}

#[derive(Debug, Serialize, TS)]
struct Field {
    name: &'static str,
    value: String,
}

impl StatusCard {
    fn new(kind: StatusKind, id: &'static str, title: impl Into<String>) -> Self {
        Self {
            kind,
            id,
            role: StatusRole::Status,
            title: title.into(),
            time: None,
            reason: None,
            since: None,
            code: false,
            imperative: None,
            next: None,
            actions: Vec::new(),
        }
    }

    fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    fn time(mut self, words: &'static str, ms: Option<u64>) -> Self {
        self.time = ms.map(|ms| CardTime { words, ms });
        self
    }

    fn since(mut self, words: &'static str, ms: Option<u64>) -> Self {
        self.since = ms.map(|ms| CardTime { words, ms });
        self
    }

    fn code(mut self) -> Self {
        self.code = true;
        self
    }

    fn next(mut self, next: &'static str) -> Self {
        self.next = Some(next);
        self
    }

    fn imperative(mut self, imperative: &'static str) -> Self {
        self.imperative = Some(imperative);
        self
    }

    fn role(mut self, role: StatusRole) -> Self {
        self.role = role;
        self
    }

    fn action(mut self, action: Option<StatusAction>) -> Self {
        self.actions.extend(action);
        self
    }

    fn next_if(self, shown: bool, next: &'static str) -> Self {
        if shown { self.next(next) } else { self }
    }

    fn retry(
        self,
        turn: Option<(&str, &str)>,
        next: Option<&'static str>,
        tier: ButtonTier,
        hint: Option<&'static str>,
    ) -> Self {
        let Some((request, attempt)) = turn else {
            return self;
        };
        let card = match next {
            Some(next) => self.next(next),
            None => self,
        };
        card.action(Some(StatusAction::retry(request, attempt, tier, hint)))
    }

    pub(crate) fn start_block(cards: &[Self]) -> Option<&'static str> {
        cards
            .iter()
            .map(|card| card.id)
            .find(|id| *id == START_BLOCK)
    }

    pub(crate) fn say_start(cards: &[Self]) -> bool {
        cards
            .iter()
            .any(|card| [START_FAILURE, START_BLOCK].contains(&card.id))
    }

    pub(crate) fn above_stage(round: &RoundSnapshot, offers_actions: bool) -> Vec<Self> {
        let mut cards = Vec::new();
        if !offers_actions && !round.past {
            cards.push(Self::earlier());
        }
        cards.extend(Self::of_stage(round, offers_actions));
        cards
    }

    pub(crate) fn implementation(
        implementation: &PageImplementation,
        request: &str,
        offers_actions: bool,
        offers_implement: bool,
        posts_review: bool,
    ) -> Self {
        use StatusKind::{Danger, Info, Ok, Progress, Warn};
        const ID: &str = "implementation";
        let (what, does, again) = if posts_review {
            (
                "review request",
                "It posts the review; this round is done.",
                "Select an agent in the review pane, then Post review again.",
            )
        } else {
            (
                "implementation request",
                "It implements the list; this round is done.",
                "Select an agent in the review pane, then Implement again.",
            )
        };
        let delivery = || Field::new("delivery", &implementation.delivery);
        let resend = |label| StatusAction {
            method: "resend-implementation",
            fields: vec![
                Field::new("conclusion", request),
                delivery(),
                Field::new("attempt", &implementation.attempt),
            ],
            label,
            tier: ButtonTier::Primary,
            hint: None,
        };
        match &implementation.state {
            ImplementationState::Sending => {
                Self::new(Progress, ID, format!("Sending the {what} to the agent…")).action(
                    offers_actions.then(|| StatusAction {
                        method: "cancel-implementation",
                        fields: vec![delivery()],
                        label: if posts_review {
                            "Cancel the review request"
                        } else {
                            "Cancel the implementation request"
                        },
                        tier: ButtonTier::Secondary,
                        hint: Some("Cancel stops the request if it has not reached the agent yet."),
                    }),
                )
            }
            ImplementationState::Sent => {
                Self::new(Ok, ID, format!("The agent received the {what}"))
                    .time("Sent at", implementation.sent_at_ms)
                    .reason(does)
            }
            ImplementationState::NotStarted => {
                Self::new(Warn, ID, format!("The agent did not start on the {what}"))
                    .reason("The list may still wait in the agent's prompt box.")
                    .role(StatusRole::Alert)
                    .next_if(
                        offers_actions,
                        "Look at the agent's pane, then Retry: it sends the same request again.",
                    )
                    .action(offers_actions.then(|| resend("Retry")))
            }
            ImplementationState::Paused => Self::new(Info, ID, "The request was never sent")
                .reason("The pane closed before it went out. The list you authorized is saved.")
                .action(offers_implement.then(|| resend("Send the saved request"))),
            ImplementationState::Unknown => {
                Self::new(Warn, ID, "The agent may or may not have the request")
                    .reason("The review pane was reopened while it sent the request.")
                    .imperative("Check the agent's pane first.")
                    .next_if(offers_implement, "Send again only if it never arrived.")
                    .action(offers_implement.then(|| StatusAction {
                        method: "implement",
                        fields: vec![
                            Field::new("conclusion", request),
                            Field::new("replaces", &implementation.delivery),
                            Field::new("text", &implementation.text),
                        ],
                        label: "Send a new request anyway",
                        tier: ButtonTier::Secondary,
                        hint: None,
                    }))
            }
            ImplementationState::NotSent(reason) => {
                Self::new(Danger, ID, format!("The {what} could not be sent"))
                    .reason(reason.as_str())
                    .code()
                    .next_if(offers_implement, again)
                    .role(StatusRole::Alert)
            }
            ImplementationState::Cancelled => Self::new(
                Info,
                ID,
                format!("The {what} was cancelled before it was sent"),
            ),
        }
    }

    pub(crate) fn undelivered_messages(error: &str, round: &str) -> Self {
        Self::new(
            StatusKind::Danger,
            "conversation-delivery",
            "Your message did not reach the agent",
        )
        .reason(error)
        .code()
        .next(SELECT_AGENT_THEN_RETRY)
        .role(StatusRole::Alert)
        .action(Some(StatusAction {
            method: "retry-messages",
            fields: vec![Field::new("round", round)],
            label: "Retry",
            tier: ButtonTier::Primary,
            hint: Some("Wakes the agent again for your waiting messages."),
        }))
    }

    pub(crate) fn of_notice(notice: &Notice) -> Self {
        let words = NoticeWords::of(notice.action);
        let card = match &notice.problem {
            Problem::Stale => Self::new(
                StatusKind::Info,
                "notice",
                notice.action.question_number().map_or_else(
                    || words.moved.to_owned(),
                    |number| format!("Question {number} was already answered"),
                ),
            )
            .reason(words.stale)
            .next("This page now shows the round as it is."),
            Problem::Failed(reason) => {
                Self::new(StatusKind::Danger, "notice", words.title).reason(reason.as_str())
            }
            Problem::NoReply => {
                Self::new(StatusKind::Warn, "notice", "The review tool did not reply")
                    .next(words.unknown)
            }
        };
        card.role(StatusRole::Alert)
    }

    pub(crate) fn read_only() -> Self {
        Self::new(StatusKind::Info, "notice", "This past round is read only")
            .reason("Nothing on a past round can change.")
            .next("Go back to the current round to act on it.")
            .role(StatusRole::Alert)
    }

    fn earlier() -> Self {
        Self::new(StatusKind::Info, "earlier", "This is an earlier round")
            .reason("Another review pane saved a newer round of this review.")
            .next(
                "Reopen the review pane to see the newer round, or reset this one from the ⋯ menu \
                 to start over.",
            )
            .role(StatusRole::Note)
    }

    fn of_stage(round: &RoundSnapshot, offers_actions: bool) -> Vec<Self> {
        match &round.stage {
            RoundStage::NoRound { .. } => Self::start(round, None),
            RoundStage::StartFailed { failure, .. } => Self::start(round, Some(failure)),
            RoundStage::Starting {
                start,
                started_at_ms,
            } => vec![Self::starting(start, *started_at_ms)],
            RoundStage::AgentWorking { .. } | RoundStage::Interrupted { .. } => {
                if round.stage.sent().is_some() && (offers_actions || round.past) {
                    Vec::new()
                } else {
                    Self::of_turn(round, offers_actions).into_iter().collect()
                }
            }
            RoundStage::StorageFailed { failure } => vec![Self::storage(failure)],
            RoundStage::Question { .. } | RoundStage::Conclusion { .. } => Vec::new(),
        }
    }

    pub(crate) fn of_turn(round: &RoundSnapshot, offers_actions: bool) -> Option<Self> {
        let sent = Sent::of(round);
        match &round.stage {
            RoundStage::AgentWorking {
                request,
                finishing_since_ms: Some(since_ms),
                ..
            } => Some(Self::finishing(request, *since_ms, offers_actions)),
            RoundStage::AgentWorking {
                request,
                sent_at_ms,
                not_prepared,
                ..
            } => Some(Self::working(
                round,
                Working {
                    request,
                    sent_at_ms: *sent_at_ms,
                    not_prepared: not_prepared.as_deref(),
                },
                sent,
                offers_actions,
            )),
            RoundStage::Interrupted {
                request,
                attempt,
                interruption,
                ..
            } if offers_actions => {
                let turn = request.as_deref().zip(attempt.as_deref());
                Some(Self::interrupted(
                    request.is_some(),
                    turn,
                    interruption,
                    sent,
                ))
            }
            RoundStage::Interrupted {
                request,
                interruption,
                ..
            } => round
                .past
                .then(|| Self::stopped(request.is_some(), interruption, sent)),
            _ => None,
        }
    }

    fn stopped(any: bool, interruption: &Interruption, sent: Option<Sent>) -> Self {
        use StatusKind::{Danger, Info, Warn};
        const ID: &str = "stopped";
        let (subject, what) = match sent {
            Some(sent) => (sent.subject(), sent.noun()),
            None => ("The turn's prompt", "the turn's prompt"),
        };
        let card = match interruption {
            _ if !any => Self::new(Info, ID, "The round had no turn to answer"),
            Interruption::Failed(failure) => {
                Self::new(Danger, ID, format!("{subject} did not reach the agent"))
                    .reason(failure.as_str())
            }
            Interruption::NotStarted => Self::new(
                Warn,
                ID,
                format!("The agent had not started on {what} when the round was saved"),
            ),
            Interruption::Uncertain => Self::new(
                Warn,
                ID,
                format!("The round was sending {what} when it was saved"),
            )
            .reason("Whether the agent received it is unknown."),
            Interruption::Stopped => Self::new(
                Info,
                ID,
                "The agent had not answered when the round was saved",
            ),
        };
        card.next(
            "A round still running in another review pane moves on there; this page follows it.",
        )
        .role(StatusRole::Note)
    }

    fn start(round: &RoundSnapshot, failure: Option<&str>) -> Vec<Self> {
        let failed = failure
            .filter(|failure| {
                round
                    .start_block
                    .is_none_or(|block| block.reason() != *failure)
            })
            .map(|failure| {
                Self::new(
                    StatusKind::Danger,
                    START_FAILURE,
                    "The round could not start",
                )
                .reason(failure)
                .code()
                .next("Fix what it says, then Start again.")
                .role(StatusRole::Alert)
            });
        let blocked = round.start_block.map(|block| match block {
            StartBlock::NothingToReview => Self::new(
                StatusKind::Ok,
                START_BLOCK,
                "Every changed line is reviewed",
            )
            .next(
                "Nothing is left to explore. Unmark lines or files in the review pane to \
                     start a round.",
            ),
        });
        failed.into_iter().chain(blocked).collect()
    }

    fn starting(start: &str, started_at_ms: Option<u64>) -> Self {
        Self::new(StatusKind::Progress, "waiting", "Preparing the round")
            .reason(
                "The review tool captures the change; Jev marks the insignificant lines first \
                 when it is on.",
            )
            .since("Sent", started_at_ms)
            .action(Some(StatusAction::stop(Stopped::Start(start))))
    }

    fn working(
        round: &RoundSnapshot,
        working: Working<'_>,
        sent: Option<Sent>,
        offers_actions: bool,
    ) -> Self {
        let Working {
            request,
            sent_at_ms,
            not_prepared,
        } = working;
        let title = match sent {
            Some(sent) => format!("The agent is working on {}", sent.named()),
            None if round.design.is_none() => {
                "The agent is working on the design and its first question".to_owned()
            }
            None => "The agent is working on its next turn".to_owned(),
        };
        let next = if sent.is_some_and(Sent::answers_question) {
            "You can leave this tab; its title changes when the next question is ready."
        } else {
            "You can leave this tab; its title changes when the agent's turn is ready."
        };
        let card = Self::new(StatusKind::Progress, "waiting", title);
        match not_prepared {
            Some(line) => card.reason(format!("{line}.")),
            None => card,
        }
        .since("Sent", sent_at_ms)
        .next(next)
        .action(offers_actions.then(|| StatusAction::stop(Stopped::Turn(request, sent))))
    }

    fn finishing(request: &str, since_ms: u64, offers_actions: bool) -> Self {
        Self::new(
            StatusKind::Progress,
            "waiting",
            "The agent is finishing a turn it started while you were thinking",
        )
        .since("Started", Some(since_ms))
        .next("You can leave this tab; its title changes when the next question is ready.")
        .action(offers_actions.then(|| StatusAction::stop(Stopped::Finishing(request))))
    }

    fn storage(failure: &str) -> Self {
        Self::new(
            StatusKind::Danger,
            "storage",
            "Explore cannot save rounds, so this page can do nothing",
        )
        .reason(failure)
        .code()
        .next("Fix the problem, then close the review pane and open it again.")
        .role(StatusRole::Alert)
    }

    fn interrupted(
        any: bool,
        turn: Option<(&str, &str)>,
        interruption: &Interruption,
        sent: Option<Sent>,
    ) -> Self {
        use StatusKind::{Danger, Info, Warn};
        const ID: &str = "interruption";
        let (subject, what) = match sent {
            Some(sent) => (sent.subject(), sent.noun()),
            None => ("The turn's prompt", "the turn's prompt"),
        };
        match interruption {
            Interruption::Failed(failure) => {
                Self::new(Danger, ID, format!("{subject} did not reach the agent"))
                    .reason(failure.as_str())
                    .role(StatusRole::Alert)
                    .retry(
                        turn,
                        Some(SELECT_AGENT_THEN_RETRY),
                        ButtonTier::Primary,
                        sent.filter(|sent| sent.answers_question())
                            .map(|_| "Your answer and its marks are kept."),
                    )
            }
            Interruption::NotStarted => {
                Self::new(Warn, ID, format!("The agent did not start on {what}"))
                    .reason("Its prompt may still wait in the agent's prompt box.")
                    .role(StatusRole::Alert)
                    .retry(
                        turn,
                        Some("Look at the agent's pane, then Retry: it sends the same turn again."),
                        ButtonTier::Primary,
                        None,
                    )
            }
            Interruption::Uncertain => {
                Self::new(Warn, ID, format!("The agent may or may not have {what}"))
                    .reason("The review pane was reopened while it sent the prompt.")
                    .imperative("Check the agent's pane before you retry.")
                    .retry(
                        turn,
                        None,
                        ButtonTier::Secondary,
                        Some("Sends the same turn again, which could duplicate it."),
                    )
            }
            Interruption::Stopped if !any => {
                Self::new(Info, ID, "The round has no turn to send again")
                    .next("Reset the round from the ⋯ menu, then start a new one.")
            }
            Interruption::Stopped => Self::new(Info, ID, "The turn is paused")
                .reason(
                    "You stopped waiting, or the review pane was reopened before the prompt went \
                     out.",
                )
                .retry(
                    turn,
                    Some(match sent {
                        Some(Sent::Answer { .. }) => "Retry sends it again, with your answer.",
                        Some(Sent::Reply) => "Retry sends it again, with your reply.",
                        None => "Retry sends it again.",
                    }),
                    ButtonTier::Primary,
                    None,
                ),
        }
    }
}

impl StatusAction {
    fn stop(stopped: Stopped<'_>) -> Self {
        let (hint, field) = match stopped {
            Stopped::Turn(request, Some(Sent::Answer { .. })) => (
                "Your answer stays; Retry sends it again.",
                Field::new("request", request),
            ),
            Stopped::Turn(request, Some(Sent::Reply)) => (
                "Your reply stays; Retry sends it again.",
                Field::new("request", request),
            ),
            Stopped::Turn(request, None) => (
                "After Stop waiting, Retry sends the turn again.",
                Field::new("request", request),
            ),
            Stopped::Finishing(request) => (
                "Your answer goes to the agent, which starts the turn over.",
                Field::new("request", request),
            ),
            Stopped::Start(start) => (
                "Stop waiting drops the round before it starts.",
                Field::new("start", start),
            ),
        };
        Self {
            method: "stop",
            fields: vec![field],
            label: "Stop waiting",
            tier: ButtonTier::Secondary,
            hint: Some(hint),
        }
    }

    fn retry(request: &str, attempt: &str, tier: ButtonTier, hint: Option<&'static str>) -> Self {
        Self {
            method: "retry",
            fields: vec![
                Field::new("request", request),
                Field::new("attempt", attempt),
            ],
            label: "Retry",
            tier,
            hint,
        }
    }
}

#[derive(Clone, Copy)]
enum Sent {
    Answer { number: Option<usize> },
    Reply,
}

impl Sent {
    fn of(round: &RoundSnapshot) -> Option<Self> {
        let answer = round.stage.sent()?;
        Some(match answer.question {
            Some(_) => Self::Answer {
                number: round.sent_number(),
            },
            None => Self::Reply,
        })
    }

    fn answers_question(self) -> bool {
        matches!(self, Self::Answer { .. })
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Answer { .. } => "your answer",
            Self::Reply => "your reply",
        }
    }

    fn subject(self) -> &'static str {
        match self {
            Self::Answer { .. } => "Your answer",
            Self::Reply => "Your reply",
        }
    }

    fn named(self) -> String {
        match self {
            Self::Answer {
                number: Some(number),
            } => format!("your answer to question {number}"),
            _ => self.noun().to_owned(),
        }
    }
}

#[derive(Clone, Copy)]
struct Working<'a> {
    request: &'a str,
    sent_at_ms: Option<u64>,
    not_prepared: Option<&'a str>,
}

#[derive(Clone, Copy)]
enum Stopped<'a> {
    Start(&'a str),
    Turn(&'a str, Option<Sent>),
    Finishing(&'a str),
}

impl Field {
    fn new(name: &'static str, value: &str) -> Self {
        Self {
            name,
            value: value.to_owned(),
        }
    }
}

struct NoticeWords {
    title: &'static str,
    moved: &'static str,
    stale: &'static str,
    unknown: &'static str,
}

impl NoticeWords {
    const fn new(
        title: &'static str,
        moved: &'static str,
        stale: &'static str,
        unknown: &'static str,
    ) -> Self {
        Self {
            title,
            moved,
            stale,
            unknown,
        }
    }

    fn of(action: Action) -> Self {
        match action {
            Action::Answer { .. } => Self::new(
                "Your answer was not sent",
                QUESTION_ANSWERED,
                "In the review pane or in another tab, or the round moved on, so your answer was \
                 not sent.",
                "Load this page again to see whether it took your answer.",
            ),
            Action::Start => Self::new(
                "The round was not started",
                "A round was started meanwhile",
                "In the pane or in another tab, so this start did nothing.",
                "Load this page again to see whether it started the round.",
            ),
            Action::Pick { .. } => Self::new(
                "Your pick was not kept",
                QUESTION_ANSWERED,
                "In the review pane or in another tab, or the round moved on, so your pick was not \
                 kept.",
                "Load this page again to see whether it kept your pick.",
            ),
            Action::Implement => Self::new(
                "The implementation request was not sent",
                "This conclusion no longer waits for a request",
                "A request was sent from the pane or from another tab, or the round moved on, so \
                 this one was not sent.",
                "Load this page again to see whether it sent the implementation request.",
            ),
            Action::Quiz => Self::new(
                "Your quiz answer was not kept",
                "This quiz question no longer waits for an answer",
                "It was answered or skipped in another tab, or the round moved on, so your quiz \
                 answer was not kept.",
                "Load this page again to see whether it kept your quiz answer.",
            ),
            Action::Message => Self::new(
                "Your message was not sent",
                "This round is over",
                "It was reset or replaced, in the pane or in another tab, so your message was not \
                 sent.",
                "Load this page again to see whether the conversation took your message.",
            ),
            Action::RetryMessages => Self::new(
                "Retry did nothing",
                "No message waits for the agent",
                "The agent replied, or the round moved on, so Retry did nothing.",
                "Load this page again to see whether the agent was woken again.",
            ),
            Action::Recover(recovery) => Self::recovery(recovery),
        }
    }

    fn recovery(action: RecoveryAction) -> Self {
        match action {
            RecoveryAction::Stop => Self::new(
                "Stop waiting did nothing",
                "The page no longer waits for what it showed",
                "The round moved on, in the pane or in another tab, so Stop waiting did nothing.",
                "Load this page again to see whether it stopped waiting.",
            ),
            RecoveryAction::Retry => Self::new(
                "Retry did nothing",
                "The turn is no longer interrupted",
                "It was sent again in the pane or in another tab, or the round moved on, so Retry \
                 did nothing.",
                "Load this page again to see whether it sent the turn again.",
            ),
            RecoveryAction::CancelAnswer => Self::new(
                "Your answer was not cancelled",
                "It can no longer be cancelled",
                "The round moved on, in the pane or in another tab, so your answer was not \
                 cancelled.",
                "Load this page again to see whether it cancelled your answer.",
            ),
            RecoveryAction::Reset => Self::new(
                "The round was not reset",
                "The round was already reset or replaced",
                "In the pane or in another tab, so this reset did nothing.",
                "Load this page again to see whether it reset the round.",
            ),
            RecoveryAction::CancelImplementation => Self::new(
                "The implementation request was not cancelled",
                "The implementation request is no longer being sent",
                "It was sent or cancelled meanwhile, so it was not cancelled.",
                "Load this page again to see whether it cancelled the implementation request.",
            ),
        }
    }
}

const SELECT_AGENT_THEN_RETRY: &str = "Select an agent in the review pane, then Retry.";

const START_FAILURE: &str = "start-failure";
const START_BLOCK: &str = "start-block";

const QUESTION_ANSWERED: &str = "This question was already answered";
