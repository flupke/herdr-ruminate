use std::path::Path;
use std::sync::Arc;

use linear_issues::LinearIssue;
pub use review_explore::ImplementationState;
use review_explore::{
    CodeLocation, Conclusion, ConversationTurn, Design, Exploration, Interpretation,
    InterviewUpdate, KeptAnswer, MarkCounts, NotRelevantMark, Question, QuizAnswers, QuizResponse,
    RoundOverview, StartBlock, Step, StepState,
};
use review_explore_citations::Citation;
use review_explore_tally::MarkTally;
use review_repository::repository::{ShortRevision, SnapshotIdentity};
use review_turn_path::TurnPath;
use review_types::ReviewUnit;
use serde::Serialize;
use tokio::sync::watch;
use ts_rs::TS;

use crate::blind::BlindQuestion;
use crate::{CommandSender, PageConversation, PageQuizResponse, PastRounds, Token, Waiting};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoundStage {
    NoRound {
        start: String,
    },
    Starting {
        start: String,
        started_at_ms: Option<u64>,
    },
    StartFailed {
        failure: String,
        start: String,
    },
    AgentWorking {
        request: String,
        sent_at_ms: Option<u64>,
        finishing_since_ms: Option<u64>,
        not_prepared: Option<String>,
        answer: Option<Box<SentAnswer>>,
    },
    Question {
        number: usize,
        question: Box<Question>,
        citations: Arc<[Citation]>,
        marks: QuestionMarks,
        response: TurnResponse,
        answer_cancelled: bool,
    },
    Interrupted {
        request: Option<String>,
        attempt: Option<String>,
        interruption: Interruption,
        answer: Option<Box<SentAnswer>>,
    },
    Conclusion {
        request: String,
        conclusion: Box<Conclusion>,
        implementation: Option<PageImplementation>,
        quiz: PageQuiz,
        response: TurnResponse,
    },
    StorageFailed {
        failure: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SentAnswer {
    pub question: Option<AnsweredQuestion>,
    pub kept: KeptAnswer,
    pub marked: MarkCounts,
    pub marks_problem: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnsweredQuestion {
    pub question: Box<Question>,
    pub citations: Arc<[Citation]>,
    pub picked_blind: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Interruption {
    Failed(String),
    NotStarted,
    Uncertain,
    Stopped,
}

impl RoundStage {
    pub(crate) fn asks(&self, id: &str, version: u32) -> Option<&Question> {
        self.asked_question()
            .filter(|question| question.is_version(id, version))
    }

    pub(crate) fn asked_question(&self) -> Option<&Question> {
        match self {
            Self::Question { question, .. } => Some(question),
            _ => None,
        }
    }

    pub(crate) fn blind(&self) -> Option<BlindQuestion<'_>> {
        match self {
            Self::Question {
                question,
                answer_cancelled: false,
                ..
            } => BlindQuestion::of(question),
            _ => None,
        }
    }

    pub fn response(&self) -> Option<&TurnResponse> {
        match self {
            Self::Question { response, .. } | Self::Conclusion { response, .. } => {
                Some(response).filter(|response| !response.is_empty())
            }
            _ => None,
        }
    }

    pub(crate) fn sent(&self) -> Option<&SentAnswer> {
        match self {
            Self::AgentWorking { answer, .. } | Self::Interrupted { answer, .. } => {
                answer.as_deref()
            }
            _ => None,
        }
    }

    pub(crate) fn offered_start(&self) -> Option<&str> {
        match self {
            Self::NoRound { start } | Self::StartFailed { start, .. } => Some(start),
            _ => None,
        }
    }

    pub(crate) fn stops(&self, waiting: &Waiting) -> bool {
        match (self, waiting) {
            (Self::Starting { start, .. }, Waiting::Start(stopped)) => start == stopped,
            (Self::AgentWorking { request, .. }, Waiting::Turn(stopped)) => request == stopped,
            _ => false,
        }
    }

    pub(crate) fn retries(&self, request: &str, attempt: &str) -> bool {
        matches!(
            self,
            Self::Interrupted { request: Some(retried), attempt: Some(latest), .. }
                if retried == request && latest == attempt
        )
    }

    pub(crate) fn concludes(&self, conclusion: &str) -> bool {
        matches!(self, Self::Conclusion { request, .. } if request == conclusion)
    }

    pub(crate) fn sends_implementation(&self, delivery: &str) -> bool {
        matches!(
            self,
            Self::Conclusion { implementation: Some(implementation), .. }
                if implementation.delivery == delivery
                    && implementation.state == ImplementationState::Sending
        )
    }

    pub(crate) fn resends_implementation(
        &self,
        conclusion: &str,
        delivery: &str,
        attempt: &str,
    ) -> bool {
        matches!(
            self,
            Self::Conclusion { request, implementation: Some(implementation), .. }
                if request == conclusion
                    && implementation.delivery == delivery
                    && implementation.attempt == attempt
                    && matches!(
                        implementation.state,
                        ImplementationState::Paused | ImplementationState::NotStarted
                    )
        )
    }

    pub(crate) fn takes_quiz(&self, response: &PageQuizResponse) -> bool {
        let Self::Conclusion {
            request,
            conclusion,
            quiz,
            ..
        } = self
        else {
            return false;
        };
        *request == response.conclusion
            && quiz.takes_answers
            && quiz
                .answers
                .clone()
                .record(&conclusion.quiz, response.response)
                .is_ok()
    }

    pub(crate) fn implementation(&self) -> Option<&PageImplementation> {
        match self {
            Self::Conclusion { implementation, .. } => implementation.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn quiz_has(&self, response: &PageQuizResponse) -> bool {
        let Self::Conclusion { request, quiz, .. } = self else {
            return false;
        };
        *request == response.conclusion
            && match response.response {
                QuizResponse::Pick { item, answer } => quiz
                    .answers
                    .pick(item)
                    .is_some_and(|pick| pick.answer == answer),
                QuizResponse::Skip => quiz.answers.skipped,
            }
    }

    pub(crate) fn offers_implement(&self, conclusion: &str, replaces: Option<&str>) -> bool {
        let Self::Conclusion {
            request,
            implementation,
            ..
        } = self
        else {
            return false;
        };
        request == conclusion
            && match implementation {
                None => replaces.is_none(),
                Some(implementation) => {
                    implementation.state.allows_another()
                        && replaces == Some(implementation.delivery.as_str())
                }
            }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageImplementation {
    pub delivery: String,
    pub attempt: String,
    pub text: String,
    pub state: ImplementationState,
    pub sent_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PageQuiz {
    pub proofs: Vec<Arc<[Citation]>>,
    pub answers: QuizAnswers,
    pub takes_answers: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct TurnResponse {
    pub interpretations: Vec<Interpretation>,
    pub reply: Option<String>,
    pub path: Option<TurnPath>,
    pub revision: Option<QuestionRevised>,
    pub marks_problem: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QuestionRevised {
    pub reason: String,
    pub replaced: u32,
}

impl TurnResponse {
    pub fn of(exploration: &Exploration, turn: &ConversationTurn) -> Self {
        Self {
            interpretations: turn.answer.as_deref().map_or_else(Vec::new, |answer| {
                exploration
                    .interpretations
                    .iter()
                    .filter(|interpretation| interpretation.answer == answer)
                    .cloned()
                    .collect()
            }),
            reply: turn
                .update
                .reply
                .as_ref()
                .map(|reply| reply.text.clone())
                .filter(|text| !text.trim().is_empty()),
            path: None,
            revision: turn.revision.as_ref().and_then(|revision| {
                let question = turn.update.next.as_ref()?;
                let replaced = exploration
                    .questions
                    .iter()
                    .take_while(|posted| *posted != question)
                    .last()?;
                Some(QuestionRevised {
                    reason: revision.reason.clone(),
                    replaced: replaced.version,
                })
            }),
            marks_problem: None,
        }
    }

    pub(crate) fn path_line(&self) -> Option<std::borrow::Cow<'static, str>> {
        self.path.as_ref().and_then(TurnPath::line)
    }

    fn is_empty(&self) -> bool {
        self.interpretations.is_empty() && self.reply.is_none() && self.path_line().is_none()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
pub struct ReviewName {
    pub repository: String,
    pub revision: ShortRevision,
    pub title: String,
    pub issues: Vec<LinearIssue>,
}

impl ReviewName {
    pub fn of(root: &Path, identity: &SnapshotIdentity) -> Self {
        Self {
            revision: identity.short_revision(),
            title: identity.title().to_owned(),
            ..Self::unnamed(root)
        }
    }

    pub fn unnamed(root: &Path) -> Self {
        Self {
            repository: root.file_name().map_or_else(
                || root.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QuestionMarks {
    pub reviewed: Vec<CodeLocation>,
    pub not_relevant: Vec<NotRelevantMark>,
    pub reopened: Vec<CodeLocation>,
}

impl QuestionMarks {
    pub fn requested(turn: &InterviewUpdate) -> Self {
        Self {
            reviewed: turn.reviewed.clone(),
            not_relevant: turn.not_relevant.clone(),
            reopened: turn.reopened.clone(),
        }
    }

    pub fn counts(&self) -> MarkCounts {
        MarkCounts::count(
            &self.reviewed,
            NotRelevantMark::locations(&self.not_relevant),
            &self.reopened,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PublishedRound<'a> {
    pub id: &'a str,
    pub review_unit: &'a ReviewUnit,
    pub design: Option<&'a Design>,
    pub changed_files: usize,
    pub author: review_explore::ChangeAuthor,
    pub cancellable: Option<&'a LatestAnswer>,
    pub earlier: bool,
    pub overview: &'a RoundOverview,
    pub earlier_citations: &'a [Arc<[Citation]>],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
pub struct LatestAnswer {
    pub id: String,
    pub choice: Option<String>,
    pub comment: String,
    #[serde(skip)]
    #[ts(skip)]
    pub answered: Answered,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Answered {
    pub question: Option<(String, u32)>,
    pub option: Option<String>,
    pub in_reply_to: String,
    pub drawing_board: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct RoundSnapshot {
    pub(crate) revision: u64,
    pub(crate) round: Option<String>,
    pub(crate) review_unit: Option<ReviewUnit>,
    pub(crate) design: Option<Arc<Design>>,
    pub(crate) changed_files: usize,
    pub(crate) author: review_explore::ChangeAuthor,
    pub(crate) cancellable: Option<LatestAnswer>,
    pub(crate) earlier: bool,
    pub(crate) past: bool,
    pub(crate) overview: Option<Arc<RoundOverview>>,
    pub(crate) earlier_citations: Arc<[Arc<[Citation]>]>,
    pub(crate) stage: RoundStage,
    pub(crate) review: Option<ReviewName>,
    pub(crate) start_block: Option<StartBlock>,
    pub(crate) tally: Option<Arc<MarkTally>>,
}

impl RoundSnapshot {
    pub(crate) fn question_number(&self) -> Option<usize> {
        let RoundStage::Question { number, .. } = self.stage else {
            return None;
        };
        let step = self.overview.as_deref().and_then(|overview| {
            overview
                .rail
                .iter()
                .find_map(|step| match (&step.step, step.state) {
                    (Step::Question { number }, StepState::Current { .. }) => Some(*number),
                    _ => None,
                })
        });
        Some(step.unwrap_or(number))
    }

    pub(crate) fn answered_number(&self) -> Option<usize> {
        self.cancellable.as_ref()?.answered.question.as_ref()?;
        self.latest_answer_number()
    }

    pub(crate) fn sent_number(&self) -> Option<usize> {
        self.stage.sent()?.question.as_ref()?;
        self.latest_answer_number()
    }

    fn latest_answer_number(&self) -> Option<usize> {
        let rail = &self.overview.as_deref()?.rail;
        let mut done = None;
        let mut current = None;
        for step in rail {
            match (&step.step, step.state) {
                (Step::Question { number }, StepState::Done) => done = Some(*number),
                (Step::Question { number }, StepState::Current { .. }) => current = Some(*number),
                _ => {}
            }
        }
        match self.stage {
            RoundStage::Question { .. } | RoundStage::Conclusion { .. } => done,
            _ => current.or(done),
        }
    }

    pub(crate) fn conversation_round(&self) -> Option<(&str, &ReviewUnit)> {
        if matches!(
            self.stage,
            RoundStage::StorageFailed { .. }
                | RoundStage::NoRound { .. }
                | RoundStage::StartFailed { .. }
        ) {
            return None;
        }
        self.round.as_deref().zip(self.review_unit.as_ref())
    }

    pub(crate) fn number_of_question(&self, id: &str, version: u32) -> Option<usize> {
        self.numbered_version(id, Some(version))
            .or_else(|| self.numbered_version(id, None))
    }

    fn numbered_version(&self, id: &str, version: Option<u32>) -> Option<usize> {
        let is = |question_id: &str, question_version: u32| {
            question_id == id && version.is_none_or(|version| version == question_version)
        };
        let overview = self.overview.as_deref()?;
        if let Some(earlier) = overview
            .earlier
            .iter()
            .rev()
            .find(|earlier| is(&earlier.question.id, earlier.question.version))
        {
            return Some(earlier.number);
        }
        if self
            .stage
            .asked_question()
            .is_some_and(|asked| is(&asked.id, asked.version))
        {
            return self.question_number();
        }
        let answered = self.cancellable.as_ref()?.answered.question.as_ref()?;
        is(&answered.0, answered.1)
            .then(|| self.answered_number())
            .flatten()
    }

    pub(crate) fn repeats(&self, answered: &Answered, comment: &str) -> bool {
        self.cancellable.as_ref().is_some_and(|latest| {
            let same = match &answered.question {
                Some(_) => {
                    latest.answered.question == answered.question
                        && latest.answered.option == answered.option
                        && latest.answered.drawing_board == answered.drawing_board
                }
                None => {
                    latest.answered.question.is_none()
                        && latest.answered.option.is_none()
                        && latest.answered.in_reply_to == answered.in_reply_to
                }
            };
            same && latest.comment == comment
        })
    }

    fn shows(&self, round: Option<PublishedRound<'_>>, stage: &RoundStage) -> bool {
        self.stage == *stage
            && self.round.as_deref() == round.map(|round| round.id)
            && self.review_unit.as_ref() == round.map(|round| round.review_unit)
            && self.design.as_deref() == round.and_then(|round| round.design)
            && self.changed_files == round.map_or(0, |round| round.changed_files)
            && self.cancellable.as_ref() == round.and_then(|round| round.cancellable)
            && self.earlier == round.is_some_and(|round| round.earlier)
            && self.overview.as_deref() == round.map(|round| round.overview)
            && *self.earlier_citations == *round.map_or(&[][..], |round| round.earlier_citations)
    }

    fn new(
        revision: u64,
        round: Option<PublishedRound<'_>>,
        stage: RoundStage,
        review: Option<ReviewName>,
        start_block: Option<StartBlock>,
    ) -> Self {
        Self {
            revision,
            round: round.map(|round| round.id.to_owned()),
            review_unit: round.map(|round| round.review_unit.clone()),
            design: round.and_then(|round| round.design.cloned().map(Arc::new)),
            changed_files: round.map_or(0, |round| round.changed_files),
            author: round.map(|round| round.author).unwrap_or_default(),
            cancellable: round.and_then(|round| round.cancellable.cloned()),
            earlier: round.is_some_and(|round| round.earlier),
            past: false,
            overview: round.map(|round| Arc::new(round.overview.clone())),
            earlier_citations: round
                .map_or_else(|| Arc::from([]), |round| round.earlier_citations.into()),
            stage,
            review,
            start_block,
            tally: None,
        }
    }

    pub(crate) fn past(round: PublishedRound<'_>, stage: RoundStage, review: ReviewName) -> Self {
        Self {
            past: true,
            ..Self::new(0, Some(round), stage, Some(review), None)
        }
    }

    pub(crate) fn waits_for_reviewer(&self) -> bool {
        match &self.stage {
            RoundStage::Question { .. } => true,
            RoundStage::Conclusion { implementation, .. } => implementation.is_none(),
            RoundStage::Interrupted { request, .. } => request.is_some(),
            _ => false,
        }
    }
}

pub struct RoundPublisher(watch::Sender<RoundSnapshot>);

impl RoundPublisher {
    pub fn new(round: Option<PublishedRound<'_>>, stage: RoundStage) -> Self {
        Self(watch::Sender::new(RoundSnapshot::new(
            1, round, stage, None, None,
        )))
    }

    pub fn publish(&self, round: Option<PublishedRound<'_>>, stage: RoundStage) {
        self.publish_with(round, stage, None);
    }

    pub fn publish_counted(
        &self,
        round: Option<PublishedRound<'_>>,
        stage: RoundStage,
        tally: MarkTally,
    ) {
        self.publish_with(round, stage, Some(tally));
    }

    fn publish_with(
        &self,
        round: Option<PublishedRound<'_>>,
        stage: RoundStage,
        tally: Option<MarkTally>,
    ) {
        self.0.send_if_modified(|snapshot| {
            let tally = tally
                .filter(|tally| snapshot.tally.as_deref() != Some(tally))
                .map(Arc::new);
            let shown = snapshot.shows(round, &stage);
            if shown && tally.is_none() {
                return false;
            }
            if shown {
                snapshot.revision += 1;
            } else {
                let kept = snapshot.tally.take();
                *snapshot = RoundSnapshot {
                    tally: kept,
                    ..RoundSnapshot::new(
                        snapshot.revision + 1,
                        round,
                        stage,
                        snapshot.review.take(),
                        snapshot.start_block,
                    )
                };
            }
            if tally.is_some() {
                snapshot.tally = tally;
            }
            true
        });
    }

    pub fn name(&self, review: ReviewName) {
        self.0.send_if_modified(|snapshot| {
            if snapshot.review.as_ref() == Some(&review) {
                return false;
            }
            snapshot.revision += 1;
            snapshot.review = Some(review);
            true
        });
    }

    pub fn block_starts(&self, block: Option<StartBlock>) {
        self.0.send_if_modified(|snapshot| {
            if snapshot.start_block == block {
                return false;
            }
            snapshot.revision += 1;
            snapshot.start_block = block;
            true
        });
    }

    pub fn tally(&self, tally: MarkTally) {
        self.0.send_if_modified(|snapshot| {
            if snapshot.tally.as_deref() == Some(&tally) {
                return false;
            }
            snapshot.revision += 1;
            snapshot.tally = Some(Arc::new(tally));
            true
        });
    }

    pub fn subscribe(&self) -> RoundFeed {
        RoundFeed(self.0.subscribe())
    }
}

impl Default for RoundPublisher {
    fn default() -> Self {
        let start = uuid::Uuid::new_v4().simple().to_string();
        Self::new(None, RoundStage::NoRound { start })
    }
}

#[derive(Clone, Debug)]
pub struct RoundFeed(watch::Receiver<RoundSnapshot>);

impl RoundFeed {
    pub fn stage(&self) -> RoundStage {
        self.0.borrow().stage.clone()
    }

    pub fn round(&self) -> Option<String> {
        self.0.borrow().round.clone()
    }

    pub fn design(&self) -> Option<Arc<Design>> {
        self.0.borrow().design.clone()
    }

    pub fn cancellable(&self) -> Option<LatestAnswer> {
        self.0.borrow().cancellable.clone()
    }

    pub fn earlier(&self) -> bool {
        self.0.borrow().earlier
    }

    pub fn overview(&self) -> Option<Arc<RoundOverview>> {
        self.0.borrow().overview.clone()
    }

    pub fn earlier_citations(&self) -> Arc<[Arc<[Citation]>]> {
        self.0.borrow().earlier_citations.clone()
    }

    pub fn review(&self) -> Option<ReviewName> {
        self.0.borrow().review.clone()
    }

    pub fn start_block(&self) -> Option<StartBlock> {
        self.0.borrow().start_block
    }

    pub fn tally(&self) -> Option<Arc<MarkTally>> {
        self.0.borrow().tally.clone()
    }

    pub async fn changed(&mut self) -> bool {
        self.0.changed().await.is_ok()
    }

    pub(crate) fn latest(&self) -> RoundSnapshot {
        self.0.borrow().clone()
    }
}

#[derive(Clone)]
pub struct PageRound {
    pub(crate) stages: RoundFeed,
    pub(crate) commands: CommandSender,
    pub(crate) conversation: Option<PageConversation>,
    pub(crate) past: Option<PastRounds>,
}

impl PageRound {
    pub fn new(stages: RoundFeed, commands: CommandSender) -> Self {
        Self {
            stages,
            commands,
            conversation: None,
            past: None,
        }
    }

    #[must_use]
    pub fn with_past_rounds(mut self, past: PastRounds) -> Self {
        self.past = Some(past);
        self
    }

    #[must_use]
    pub fn without_past_rounds(mut self) -> Self {
        self.past = None;
        self
    }

    #[must_use]
    pub fn with_conversation(mut self, conversation: PageConversation) -> Self {
        self.conversation = Some(conversation);
        self
    }

    pub fn stages(&self) -> &RoundFeed {
        &self.stages
    }
}

pub trait Rounds: Send + Sync + 'static {
    fn find(&self, token: &str) -> Option<PageRound>;

    fn after_reset(&self) -> Option<Token> {
        None
    }
}
