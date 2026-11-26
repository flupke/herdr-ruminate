use change_references::ChangeReferences;
use std::sync::{Arc, Mutex, PoisonError};

use review_explore::{
    AgentRecord, Alternative, CancelFrom, Decision, Design, DiagramError, EarlierQuestion,
    KeptAnswer, MarkCounts, Question, QuizAnswers, QuizProgress, QuizResponse, RoundOverview,
    StartBlock, TurnMarks,
};
use review_explore_citations::Citation;
use review_explore_page::{
    Answered, AnsweredQuestion, CommandRefusal, CommandSender, ImplementationState, Interruption,
    LatestAnswer, PageCommand, PageConversation, PageImplementation, PageRound, PastChange,
    PastRound, PastRoundEntry, PastRounds, PastRoundsPublisher, PublishedRound, QuestionMarks,
    Recovery, ReviewName, RoundPublisher, RoundStage, Rounds, SentAnswer, ThreadSender, Token,
    Waiting,
};
use review_explore_tally::MarkTally;
use review_repository::repository::ShortRevision;
use review_threads::{ReviewThreads, ThreadCommand};
use review_turn_path::{PlainReason, TurnPath};
use review_types::ReviewUnit;
use serde::Serialize;

use crate::conversation::{SentMessage, SessionThreads};
use crate::overview::Posted;
use crate::round_data::RoundData;
use crate::tally::SessionMarks;

#[derive(Clone)]
pub(crate) struct Sessions {
    data: &'static dyn RoundData,
    clock: Clock,
    open: Arc<Mutex<Vec<Session>>>,
}

struct Session {
    token: Token,
    clock: Clock,
    turn_answer: Option<Box<SentAnswer>>,
    sent_at_ms: Option<u64>,
    id: String,
    data: &'static dyn RoundData,
    round: RoundPublisher,
    rounds: usize,
    running: bool,
    asked: usize,
    latest_question: Option<RoundStage>,
    concluded: bool,
    answers: Vec<PostedAnswer>,
    diagram_errors: Vec<DiagramError>,
    starts: Vec<SentStart>,
    implementations: Vec<String>,
    quiz: Option<QuizAnswers>,
    turns: usize,
    attempts: usize,
    offers: usize,
    answered: Vec<LatestAnswer>,
    decisions: Vec<Decision>,
    answered_questions: Vec<(EarlierQuestion, Arc<[Citation]>)>,
    taken_up: usize,
    answer_marks: Vec<QuestionMarks>,
    marked_by_hand: u64,
    actions: Vec<String>,
    threads: SessionThreads,
    earlier: bool,
    page: PageLink,
    past: Vec<PastRound>,
    past_list: PastRoundsPublisher,
}

struct RoundParts {
    id: String,
    review: ReviewUnit,
    design: Design,
    overview: RoundOverview,
    earlier_citations: Vec<Arc<[Citation]>>,
}

impl RoundParts {
    fn round<'a>(&'a self, session: &'a Session) -> PublishedRound<'a> {
        PublishedRound {
            id: &self.id,
            review_unit: &self.review,
            design: (session.asked > 0).then_some(&self.design),
            changed_files: session.data.change().0.len(),
            author: if session.starts.last().is_some_and(|start| start.others) {
                review_explore::ChangeAuthor::Other
            } else {
                review_explore::ChangeAuthor::Reviewer
            },
            cancellable: session.answered.last(),
            earlier: session.earlier,
            overview: &self.overview,
            earlier_citations: &self.earlier_citations,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Clock {
    Real,
    Fixed,
}

impl Clock {
    fn now_ms(self) -> u64 {
        match self {
            Self::Real => review_explore::now_ms(),
            Self::Fixed => FIXED_NOW_MS,
        }
    }

    pub(crate) fn stamped(self, threads: &ReviewThreads) -> ReviewThreads {
        let mut threads = threads.clone();
        if let Self::Fixed = self {
            threads.stamp_postings(|sequence| FIXED_NOW_MS + sequence * 1_000);
        }
        threads
    }
}

const MINUTE_MS: u64 = 60 * 1_000;

const FIXED_NOW_MS: u64 = 1_791_000_000_000;

#[derive(Default)]
enum PageLink {
    #[default]
    Following,
    Held {
        unseen: Option<Box<RoundStage>>,
    },
    Away,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PostedAnswer {
    question: String,
    version: u32,
    choice: Option<String>,
    comment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_pick: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    drawing_board: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SentStart {
    challenger: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    others: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewer: Option<String>,
}

impl Session {
    fn new(
        id: String,
        token: Token,
        clock: Clock,
        data: &'static dyn RoundData,
        asked: usize,
    ) -> Self {
        let round = RoundPublisher::default();
        round.name(data.review());
        Self {
            id,
            token,
            clock,
            turn_answer: None,
            sent_at_ms: Some(clock.now_ms()),
            data,
            round,
            rounds: 1,
            running: true,
            asked,
            latest_question: (asked > 0).then(|| data.question_stage(asked, None)),
            concluded: false,
            answers: Vec::new(),
            diagram_errors: Vec::new(),
            starts: Vec::new(),
            implementations: Vec::new(),
            quiz: None,
            turns: 0,
            attempts: 0,
            offers: 0,
            answered: Vec::new(),
            decisions: Vec::new(),
            answered_questions: Vec::new(),
            taken_up: 0,
            answer_marks: Vec::new(),
            marked_by_hand: 0,
            actions: Vec::new(),
            threads: SessionThreads::new(clock),
            earlier: false,
            page: PageLink::Following,
            past: Vec::new(),
            past_list: PastRoundsPublisher::default(),
        }
    }

    fn round_id(&self) -> String {
        format!("{}-{}", self.id, self.rounds)
    }

    fn publish(&mut self, stage: RoundStage) {
        if let PageLink::Held { unseen } = &mut self.page {
            *unseen = Some(Box::new(stage));
            return;
        }
        let parts = self.parts(&stage);
        let tally = self.tally(&stage);
        let round = self.running.then(|| parts.round(self));
        self.round.publish_counted(round, stage, tally);
    }

    fn parts(&self, stage: &RoundStage) -> RoundParts {
        let (mut earlier, earlier_citations): (Vec<_>, Vec<_>) = self.answered_questions
            [..self.taken_up]
            .iter()
            .cloned()
            .unzip();
        for cancel in earlier
            .iter_mut()
            .filter_map(|record| record.cancel.as_mut())
        {
            cancel.later = self
                .answered
                .iter()
                .position(|answer| answer.id == cancel.answer)
                .map_or(0, |index| self.answered.len() - index - 1);
        }
        let overview = RoundOverview {
            decisions: self.decisions.clone(),
            earlier,
            glossary: if self.asked > 0 {
                self.data.glossary()
            } else {
                std::collections::BTreeMap::new()
            },
            ..Posted {
                asked: self.asked,
                concluded: self.concluded,
                quiz: self.quiz.as_ref().map(|answers| QuizProgress {
                    items: self.data.quiz_items().len(),
                    answers: Some(answers),
                }),
            }
            .overview(stage)
        };
        RoundParts {
            id: self.round_id(),
            review: SessionThreads::review(),
            design: self.data.design(),
            overview,
            earlier_citations,
        }
    }

    fn save_past(&mut self) {
        let past = self.past_at(
            self.data.review(),
            ChangeReferences::default(),
            self.clock.now_ms(),
        );
        self.past.insert(0, past);
        self.list_past();
    }

    fn past_at(
        &self,
        review: ReviewName,
        references: ChangeReferences,
        saved_at_ms: u64,
    ) -> PastRound {
        let mut stage = self.stage();
        if let RoundStage::Conclusion { quiz, .. } = &mut stage {
            quiz.takes_answers = false;
        }
        let parts = self.parts(&stage);
        PastRound::new(
            parts.round(self),
            stage,
            review,
            references,
            Some(self.threads.stamped()),
            saved_at_ms,
        )
    }

    fn list_past(&self) {
        let mut changes: Vec<PastChange> = Vec::new();
        for past in &self.past {
            let review = past.review();
            let entry = PastRoundEntry {
                id: past.id().to_owned(),
                saved_at_ms: past.saved_at_ms(),
            };
            match changes
                .iter_mut()
                .find(|change| change.title == review.title)
            {
                Some(change) => change.rounds.push(entry),
                None => changes.push(PastChange {
                    title: review.title.clone(),
                    references: past.references().clone(),
                    revision: review.revision.clone(),
                    rounds: vec![entry],
                }),
            }
        }
        self.past_list.publish(changes);
    }

    fn tally(&self, stage: &RoundStage) -> MarkTally {
        let waiting = match stage {
            RoundStage::Question {
                question, marks, ..
            } if self.running && !self.earlier => Some((&**question, marks)),
            _ => None,
        };
        SessionMarks {
            change: self.data.change(),
            jev: self.data.jev_marks(),
            running: self.running,
            answered: &self.answer_marks,
            by_hand: self.marked_by_hand,
            waiting,
        }
        .tally()
    }

    fn stage(&self) -> RoundStage {
        match &self.page {
            PageLink::Held {
                unseen: Some(stage),
            } => (**stage).clone(),
            _ => self.round.subscribe().stage(),
        }
    }

    fn release(&mut self) {
        if let PageLink::Held { unseen } = std::mem::take(&mut self.page)
            && let Some(stage) = unseen
        {
            self.publish(*stage);
        }
    }

    fn restart(&mut self) {
        let stage = self.stage();
        let feed = self.round.subscribe();
        let round = RoundPublisher::default();
        if let Some(review) = feed.review() {
            round.name(review);
        }
        round.block_starts(feed.start_block());
        self.round = round;
        self.page = PageLink::Away;
        self.publish(stage);
    }

    fn take_answer(&mut self, answer: AnswerTaken<'_>) -> SentAnswer {
        let (alternatives, question) = match &self.latest_question {
            Some(RoundStage::Question { question, .. }) => (
                question.alternatives.as_slice(),
                Some((question.id.clone(), question.version)),
            ),
            _ => (&[][..], None),
        };
        let drawing_board = matches!(
            answer,
            AnswerTaken::FromPage {
                drawing_board: true,
                ..
            }
        );
        let (picked, comment, first_pick) = answer.kept(alternatives, self.data.pane_comment());
        let mut kept = KeptAnswer::new(picked, &comment, first_pick);
        if drawing_board {
            kept = kept.into_drawing_board();
        }
        let mut turn_answer = SentAnswer {
            question: None,
            kept: kept.clone(),
            marked: MarkCounts::default(),
            marks_problem: None,
        };
        let no_marks = QuestionMarks::default();
        if let Some(RoundStage::Question {
            question,
            citations,
            marks,
            ..
        }) = &self.latest_question
        {
            let marks = if drawing_board { &no_marks } else { marks };
            let number = self.decisions.len() + 1;
            self.decisions.push(Decision {
                number,
                question: question.text.clone(),
                answer: kept.clone(),
            });
            let record = EarlierQuestion {
                number,
                question: (**question).clone(),
                answer: Some(kept),
                recorded: AgentRecord::default(),
                marks: vec![TurnMarks {
                    reviewed: marks.reviewed.clone(),
                    not_relevant: marks.not_relevant.clone(),
                    ..TurnMarks::default()
                }],
                cancel: Some(CancelFrom {
                    answer: format!("answer-{}", self.answered.len() + 1),
                    later: 0,
                }),
            };
            self.answered_questions.push((record, citations.clone()));
            turn_answer.question = Some(AnsweredQuestion {
                question: question.clone(),
                citations: citations.clone(),
                picked_blind: first_pick.is_some(),
            });
            turn_answer.marked = marks.counts();
        }
        self.answer_marks.push(match &self.latest_question {
            Some(RoundStage::Question { marks, .. }) if !drawing_board => marks.clone(),
            _ => QuestionMarks::default(),
        });
        self.answered.push(LatestAnswer {
            id: format!("answer-{}", self.answered.len() + 1),
            choice: turn_answer.kept.choice.clone(),
            comment,
            answered: Answered {
                question,
                option: picked.map(|alternative| alternative.id.clone()),
                in_reply_to: self.turn_id(),
                drawing_board,
            },
        });
        turn_answer
    }

    fn take_up_answers(&mut self, stage: &RoundStage) {
        let response = stage.response();
        for (record, _) in &mut self.answered_questions[self.taken_up..] {
            record.recorded = AgentRecord {
                interpretation: response
                    .and_then(|response| response.interpretations.last().cloned()),
                reply: response.and_then(|response| response.reply.clone()),
            };
        }
        self.taken_up = self.answered_questions.len();
    }

    fn forget_round(&mut self) {
        self.asked = 0;
        self.answered.clear();
        self.decisions.clear();
        self.answered_questions.clear();
        self.taken_up = 0;
        self.answer_marks.clear();
        self.latest_question = None;
        self.turn_answer = None;
        self.concluded = false;
        self.earlier = false;
    }

    fn turn_after(&mut self, step: Step) -> RoundStage {
        let answer = match step {
            Step::Answer {
                after_first_pick: false,
            } => Some(self.take_answer(AnswerTaken::InPane)),
            Step::Answer {
                after_first_pick: true,
            } => Some(self.take_answer(AnswerTaken::AfterFirstPick)),
            _ => None,
        };
        self.new_turn(answer)
    }

    fn new_turn(&mut self, answer: Option<SentAnswer>) -> RoundStage {
        self.turns += 1;
        self.turn_answer = answer.map(Box::new);
        self.attempt_again()
    }

    fn attempt_again(&mut self) -> RoundStage {
        self.attempts += 1;
        self.sent_at_ms = Some(self.clock.now_ms());
        self.working()
    }

    fn attempt(&self) -> String {
        format!("attempt-{}", self.attempts)
    }

    fn offered_start(&self) -> String {
        format!("{}-start-{}", self.id, self.offers)
    }

    fn no_round(&mut self) -> RoundStage {
        self.offers += 1;
        RoundStage::NoRound {
            start: self.offered_start(),
        }
    }

    fn working(&self) -> RoundStage {
        RoundStage::AgentWorking {
            request: self.turn_id(),
            sent_at_ms: self.sent_at_ms,
            finishing_since_ms: None,
            not_prepared: None,
            answer: self.turn_answer.clone(),
        }
    }

    fn interrupted(&self, interruption: Interruption) -> RoundStage {
        RoundStage::Interrupted {
            request: Some(self.turn_id()),
            attempt: Some(self.attempt()),
            interruption,
            answer: self.turn_answer.clone(),
        }
    }

    fn turn_id(&self) -> String {
        format!("turn-{}", self.turns)
    }

    fn stage_after(&mut self, step: Step, question: Option<Question>) -> Option<RoundStage> {
        Some(match step {
            Step::Question { .. } | Step::Revise => self.posted(step, question)?,
            Step::Cancel => self.question_after_cancel_answer()?,
            Step::Answer { .. } | Step::Kickoff => self.turn_after(step),
            Step::Prompt(outcome) => self.prompt_unworked(outcome),
            Step::Interrupt => self.interrupted(Interruption::Stopped),
            Step::Conclude { quiz } => {
                self.concluded = true;
                self.quiz = quiz.then(QuizAnswers::default);
                let stage = self.conclusion(None);
                self.take_up_answers(&stage);
                stage
            }
            Step::Implement | Step::Deliver => self.implementation_stage(step)?,
            Step::Round(event) => self.after_round_event(event),
        })
    }

    fn posted(&mut self, step: Step, question: Option<Question>) -> Option<RoundStage> {
        let Step::Question { path } = step else {
            return self.revised(question?);
        };
        self.asked += 1;
        self.concluded = false;
        let mut stage = self.data.question_stage(self.asked, question);
        if let RoundStage::Question { response, .. } = &mut stage {
            response.path = path.path();
        }
        self.latest_question = Some(stage.clone());
        self.take_up_answers(&stage);
        Some(stage)
    }

    fn revised(&mut self, question: Question) -> Option<RoundStage> {
        let mut stage = self.latest_question.clone()?;
        let RoundStage::Question {
            question: asked,
            response,
            ..
        } = &mut stage
        else {
            return None;
        };
        *response = review_explore_page::TurnResponse {
            revision: Some(review_explore_page::QuestionRevised {
                reason: "You said the reviewer may keep **one draft per question**: the question \
                         now asks where that draft lives."
                    .into(),
                replaced: asked.version,
            }),
            path: None,
            ..response.clone()
        };
        **asked = question;
        self.latest_question = Some(stage.clone());
        Some(stage)
    }

    fn after_round_event(&mut self, event: RoundEvent) -> RoundStage {
        match event {
            RoundEvent::Reset => self.no_round(),
            RoundEvent::FailStart => {
                self.offers += 1;
                RoundStage::StartFailed {
                    failure: self.round.subscribe().start_block().map_or_else(
                        || "Repository comparison is not ready; retry Start".into(),
                        |block| block.reason().into(),
                    ),
                    start: self.offered_start(),
                }
            }
            RoundEvent::Earlier => {
                self.earlier = true;
                self.stage()
            }
            RoundEvent::FailStorage => RoundStage::StorageFailed {
                failure: STORAGE_FAILURE.into(),
            },
        }
    }

    fn question_after_cancel_answers_from(&mut self, answer: &str) -> Option<RoundStage> {
        let index = self.answered.iter().position(|kept| kept.id == answer)?;
        if index + 1 == self.answered.len() {
            return self.question_after_cancel_answer();
        }
        let (record, _) = self
            .answered_questions
            .iter()
            .find(|(record, _)| {
                record
                    .cancel
                    .as_ref()
                    .is_some_and(|cancel| cancel.answer == answer)
            })?
            .clone();
        let number = record.number;
        self.answered.truncate(index);
        self.answer_marks.truncate(index);
        self.decisions.truncate(number - 1);
        self.answered_questions.truncate(number - 1);
        self.taken_up = self.taken_up.min(self.answered_questions.len());
        self.concluded = false;
        self.turn_answer = None;
        self.asked = number;
        let mut stage = self.data.question_stage(number, Some(record.question));
        if let RoundStage::Question {
            answer_cancelled, ..
        } = &mut stage
        {
            *answer_cancelled = true;
        }
        self.latest_question = Some(stage.clone());
        Some(stage)
    }

    fn question_after_cancel_answer(&mut self) -> Option<RoundStage> {
        let mut stage = self.latest_question.clone()?;
        self.answered.pop();
        self.concluded = false;
        self.decisions.pop();
        self.answered_questions.pop();
        self.taken_up = self.taken_up.min(self.answered_questions.len());
        self.answer_marks.pop();
        if let RoundStage::Question {
            answer_cancelled, ..
        } = &mut stage
        {
            *answer_cancelled = true;
        }
        Some(stage)
    }

    fn implementation_stage(&self, step: Step) -> Option<RoundStage> {
        match step {
            Step::Implement => Some(self.conclusion(Some(PageImplementation {
                delivery: "pane".into(),
                attempt: self.attempt(),
                text: self.data.to_be_implemented(),
                state: ImplementationState::Sent,
                sent_at_ms: Some(SENT_AT_MS),
            }))),
            _ => self.finish_sending(ImplementationState::Sent),
        }
    }

    fn implementation(&self) -> Option<PageImplementation> {
        match self.stage() {
            RoundStage::Conclusion { implementation, .. } => implementation,
            _ => None,
        }
    }

    fn prompt_unworked(&self, outcome: PromptOutcome) -> RoundStage {
        self.finish_sending(outcome.implementation())
            .unwrap_or_else(|| self.interrupted(outcome.interruption()))
    }

    fn finish_sending(&self, state: ImplementationState) -> Option<RoundStage> {
        let sending = self
            .implementation()
            .filter(|implementation| implementation.state == ImplementationState::Sending)?;
        let sent_at_ms = (state == ImplementationState::Sent).then_some(SENT_AT_MS);
        Some(self.conclusion(Some(PageImplementation {
            state,
            sent_at_ms,
            ..sending
        })))
    }

    fn conclusion(&self, implementation: Option<PageImplementation>) -> RoundStage {
        self.data
            .conclusion_stage(implementation, self.quiz.clone())
    }
}

#[derive(Clone, Copy)]
enum AnswerTaken<'a> {
    InPane,
    AfterFirstPick,
    FromPage {
        choice: Option<&'a str>,
        comment: &'a str,
        first_pick: Option<&'a str>,
        drawing_board: bool,
    },
}

impl<'a> AnswerTaken<'a> {
    fn kept(
        self,
        alternatives: &'a [Alternative],
        pane_comment: &str,
    ) -> (Option<&'a Alternative>, String, Option<&'a str>) {
        match self {
            Self::InPane => (alternatives.first(), pane_comment.to_owned(), None),
            Self::AfterFirstPick => {
                let kept = alternatives
                    .iter()
                    .find(|alternative| alternative.recommendation.is_some())
                    .or(alternatives.first());
                let first = alternatives.iter().find(|alternative| {
                    Some(alternative.id.as_str()) != kept.map(|kept| kept.id.as_str())
                });
                (kept, String::new(), first.map(|first| first.id.as_str()))
            }
            Self::FromPage {
                choice,
                comment,
                first_pick,
                ..
            } => (
                alternatives
                    .iter()
                    .find(|alternative| Some(alternative.id.as_str()) == choice),
                comment.to_owned(),
                first_pick,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PromptOutcome {
    NotSent,
    NotStarted,
    ReopenedBeforeSending,
    ReopenedWhileSending,
}

impl PromptOutcome {
    fn implementation(self) -> ImplementationState {
        match self {
            Self::NotSent => ImplementationState::NotSent(NOT_DELIVERED.into()),
            Self::NotStarted => ImplementationState::NotStarted,
            Self::ReopenedBeforeSending => ImplementationState::Paused,
            Self::ReopenedWhileSending => ImplementationState::Unknown,
        }
    }

    fn interruption(self) -> Interruption {
        match self {
            Self::NotSent => Interruption::Failed(NOT_DELIVERED.into()),
            Self::NotStarted => Interruption::NotStarted,
            Self::ReopenedBeforeSending => Interruption::Stopped,
            Self::ReopenedWhileSending => Interruption::Uncertain,
        }
    }
}

const SENT_AT_MS: u64 = 1_790_000_000_000;

const NOT_DELIVERED: &str = "The selected agent is no longer available";

const HAND_MARKED_LINES: u64 = 3;

const STORAGE_FAILURE: &str = "No space left on device (os error 28)";

const WIDE_CHAT_REPLY: &str = "Each caller of `flush`, with what it sends and when the reviewer \
sees the notification:

| Caller | Reason | What it sends | When it runs | What the notification says |
| --- | --- | --- | --- | --- |
| `push` | `FlushReason::Full` | the whole queue, twenty replies | at the twentieth reply | twenty replies from the threads that have them |
| `tick` | `FlushReason::Idle` | whatever waits, one reply or more | two seconds after the last reply | the replies, with how long they waited |
| `close_threads` | `FlushReason::Closing` | whatever waits, even one reply | when the pane closes | the replies, before the pane goes away |

The tick is the only caller that reads the clock:

```rust
if self.policy.is_idle(now.duration_since(last)) && !self.waiting.is_empty() { self.flush(FlushReason::Idle); }
```

A reply is saved to its thread before any of them runs, so a crash loses the notification only.";

const DIAGRAM_CHAT_REPLY: &str = "What happens when a reply arrives while the pane is closing:

```mermaid
sequenceDiagram
  autonumber
  participant Thread as Review thread
  participant Queue as Reply queue
  participant Tick as Idle tick
  participant Pane
  participant Herdr
  Thread->>Queue: push reply
  Queue->>Queue: save to the thread
  Pane->>Queue: close_threads
  Queue->>Herdr: flush (Closing)
  Herdr-->>Pane: notification
  Note over Tick: stopped with the pane
```

The tick never runs after the pane closes: `close_threads` sends whatever waits.";

impl Session {
    fn recover(&mut self, recovery: &Recovery) -> Result<(), CommandRefusal> {
        let (name, stage) = match recovery {
            Recovery::Stop(Waiting::Start(_)) => ("stop", Some(self.no_round())),
            Recovery::Stop(Waiting::Turn(_)) => {
                ("stop", Some(self.interrupted(Interruption::Stopped)))
            }
            Recovery::Retry { .. } => ("retry", Some(self.attempt_again())),
            Recovery::CancelAnswer { answer } => (
                "cancel-answer",
                self.question_after_cancel_answers_from(answer),
            ),
            Recovery::Reset { .. } => ("reset", Some(self.no_round())),
            Recovery::CancelImplementation { .. } => (
                "cancel-implementation",
                self.finish_sending(ImplementationState::Cancelled),
            ),
            Recovery::ResendImplementation { .. } => {
                ("resend-implementation", Some(self.resend_implementation()))
            }
        };
        let stage = stage.ok_or(CommandRefusal::Stale)?;
        if matches!(stage, RoundStage::NoRound { .. }) {
            if self.running {
                self.save_past();
            }
            self.running = false;
            self.forget_round();
        }
        self.actions.push(name.into());
        self.publish(stage);
        Ok(())
    }

    fn resend_implementation(&mut self) -> RoundStage {
        self.attempts += 1;
        let attempt = self.attempt();
        let implementation = self
            .implementation()
            .map(|implementation| PageImplementation {
                state: ImplementationState::Sending,
                attempt,
                ..implementation
            });
        self.conclusion(implementation)
    }
}

impl Session {
    fn take(&mut self, command: PageCommand) -> Result<(), CommandRefusal> {
        match command {
            PageCommand::Answer(answer) => {
                let sent = self.take_answer(AnswerTaken::FromPage {
                    choice: answer.input.option.as_deref(),
                    comment: &answer.input.text,
                    first_pick: answer.input.first_pick.as_deref(),
                    drawing_board: answer.input.drawing_board,
                });
                self.answers.push(PostedAnswer {
                    question: answer.question,
                    version: answer.version,
                    choice: answer.input.option,
                    comment: answer.input.text,
                    first_pick: answer.input.first_pick,
                    drawing_board: answer.input.drawing_board,
                });
                let stage = self.new_turn(Some(sent));
                self.publish(stage);
            }
            PageCommand::Pick { .. } => {}
            PageCommand::DiagramFailed(error) => {
                if !self.diagram_errors.contains(&error) {
                    self.diagram_errors.push(error);
                }
            }
            PageCommand::Start { choices, start } => {
                self.starts.push(SentStart {
                    challenger: choices.challenger,
                    others: choices.author == review_explore::ChangeAuthor::Other,
                    reviewer: choices.reviewer,
                });
                self.publish(RoundStage::Starting {
                    start,
                    started_at_ms: Some(self.clock.now_ms()),
                });
            }
            PageCommand::Implement(implement) => {
                self.implementations.push(implement.text.clone());
                self.attempts += 1;
                let sending = PageImplementation {
                    delivery: format!("page-{}", self.implementations.len()),
                    attempt: self.attempt(),
                    text: implement.text,
                    state: ImplementationState::Sending,
                    sent_at_ms: None,
                };
                let stage = self.conclusion(Some(sending));
                self.publish(stage);
            }
            PageCommand::Quiz(quiz) => self.take_quiz(quiz.response)?,
            PageCommand::Recover(recovery) => self.recover(&recovery)?,
        }
        Ok(())
    }

    fn take_quiz(&mut self, response: QuizResponse) -> Result<(), CommandRefusal> {
        let answers = self.quiz.as_mut().ok_or(CommandRefusal::Stale)?;
        answers
            .record(&self.data.quiz_items(), response)
            .map_err(|_| CommandRefusal::Stale)?;
        let stage = self.conclusion(self.implementation());
        self.publish(stage);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PageChange {
    Hold,
    Restart,
    Back,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Step {
    Question { path: RunAheadPath },
    Revise,
    Answer { after_first_pick: bool },
    Prompt(PromptOutcome),
    Cancel,
    Interrupt,
    Conclude { quiz: bool },
    Implement,
    Deliver,
    Kickoff,
    Round(RoundEvent),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum RunAheadPath {
    Unwatched,
    Prepared,
    NoTurn,
}

impl RunAheadPath {
    fn path(self) -> Option<TurnPath> {
        match self {
            Self::Unwatched => None,
            Self::Prepared => Some(TurnPath::Prepared {
                session: "fork".into(),
            }),
            Self::NoTurn => Some(TurnPath::Plain {
                reason: PlainReason::NoTurn,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum RoundEvent {
    Reset,
    FailStart,
    Earlier,
    FailStorage,
}

impl Step {
    const NAMES: [(&str, Self); 21] = [
        (
            "question",
            Self::Question {
                path: RunAheadPath::Unwatched,
            },
        ),
        (
            "question-prepared",
            Self::Question {
                path: RunAheadPath::Prepared,
            },
        ),
        (
            "question-not-prepared",
            Self::Question {
                path: RunAheadPath::NoTurn,
            },
        ),
        (
            "answer",
            Self::Answer {
                after_first_pick: false,
            },
        ),
        (
            "answer-after-first-pick",
            Self::Answer {
                after_first_pick: true,
            },
        ),
        ("fail", Self::Prompt(PromptOutcome::NotSent)),
        ("not-started", Self::Prompt(PromptOutcome::NotStarted)),
        (
            "reopen-unsent",
            Self::Prompt(PromptOutcome::ReopenedBeforeSending),
        ),
        (
            "reopen-sending",
            Self::Prompt(PromptOutcome::ReopenedWhileSending),
        ),
        ("revise", Self::Revise),
        ("cancel", Self::Cancel),
        ("interrupt", Self::Interrupt),
        ("conclude", Self::Conclude { quiz: false }),
        ("conclude-with-quiz", Self::Conclude { quiz: true }),
        ("implement", Self::Implement),
        ("deliver", Self::Deliver),
        ("reset", Self::Round(RoundEvent::Reset)),
        ("kickoff", Self::Kickoff),
        ("fail-start", Self::Round(RoundEvent::FailStart)),
        ("earlier", Self::Round(RoundEvent::Earlier)),
        ("fail-storage", Self::Round(RoundEvent::FailStorage)),
    ];

    pub(crate) fn parse(name: &str) -> Option<Self> {
        Self::NAMES
            .into_iter()
            .find_map(|(known, step)| (known == name).then_some(step))
    }
}

impl Sessions {
    pub(crate) fn new(data: &'static dyn RoundData, clock: Clock) -> Self {
        Self {
            data,
            clock,
            open: Arc::default(),
        }
    }

    pub(crate) fn open(&self, token: Token, asked: usize, data: Option<&'static dyn RoundData>) {
        let data = data.unwrap_or(self.data);
        let id = format!("session-{}", self.lock().len() + 1);
        let mut session = Session::new(id, token, self.clock, data, asked);
        session.past = past_rounds(&session.id, self.clock, data);
        session.list_past();
        let stage = session
            .latest_question
            .clone()
            .unwrap_or(RoundStage::AgentWorking {
                request: "turn-0".into(),
                sent_at_ms: session.sent_at_ms,
                finishing_since_ms: None,
                not_prepared: None,
                answer: None,
            });
        session.publish(stage);
        self.lock().push(session);
    }

    pub(crate) fn step(&self, token: &str, step: Step, question: Option<Question>) -> bool {
        let mut sessions = self.lock();
        let Some(session) = find(&mut sessions, token) else {
            return false;
        };
        let Some(stage) = session.stage_after(step, question) else {
            return false;
        };
        let running = !matches!(
            stage,
            RoundStage::NoRound { .. }
                | RoundStage::Starting { .. }
                | RoundStage::StartFailed { .. }
        );
        if !running {
            if session.running {
                session.save_past();
            }
            session.forget_round();
        } else if !session.running {
            session.rounds += 1;
        }
        session.running = running;
        session.publish(stage);
        true
    }

    pub(crate) fn block_starts(&self, token: &str, block: Option<StartBlock>) -> bool {
        let mut sessions = self.lock();
        let Some(session) = find(&mut sessions, token) else {
            return false;
        };
        session.round.block_starts(block);
        true
    }

    pub(crate) fn mark_by_hand(&self, token: &str) -> bool {
        let mut sessions = self.lock();
        let Some(session) = find(&mut sessions, token) else {
            return false;
        };
        session.marked_by_hand += HAND_MARKED_LINES;
        if !matches!(session.page, PageLink::Held { .. }) {
            session.round.tally(session.tally(&session.stage()));
        }
        true
    }

    pub(crate) fn change_page(&self, token: &str, change: PageChange) -> bool {
        let mut sessions = self.lock();
        let Some(session) = find(&mut sessions, token) else {
            return false;
        };
        match change {
            PageChange::Hold => session.page = PageLink::Held { unseen: None },
            PageChange::Restart => session.restart(),
            PageChange::Back => session.page = PageLink::Following,
        }
        true
    }

    pub(crate) fn away(&self, token: &str) -> bool {
        find(&mut self.lock(), token).is_some_and(|session| matches!(session.page, PageLink::Away))
    }

    pub(crate) fn starts(&self, token: &str) -> Option<Vec<SentStart>> {
        find(&mut self.lock(), token).map(|session| session.starts.clone())
    }

    pub(crate) fn answers(&self, token: &str) -> Option<Vec<PostedAnswer>> {
        find(&mut self.lock(), token).map(|session| session.answers.clone())
    }

    pub(crate) fn implementations(&self, token: &str) -> Option<Vec<String>> {
        find(&mut self.lock(), token).map(|session| session.implementations.clone())
    }

    fn command(&self, token: &str, command: PageCommand) -> Result<(), CommandRefusal> {
        let mut sessions = self.lock();
        let session = find(&mut sessions, token).ok_or(CommandRefusal::Stale)?;
        let held = matches!(session.page, PageLink::Held { .. });
        if held && !matches!(command, PageCommand::DiagramFailed(_)) {
            session.release();
            return Err(CommandRefusal::Stale);
        }
        session.take(command)
    }

    pub(crate) fn actions(&self, token: &str) -> Option<Vec<String>> {
        find(&mut self.lock(), token).map(|session| session.actions.clone())
    }

    pub(crate) fn quiz(&self, token: &str) -> Option<QuizAnswers> {
        find(&mut self.lock(), token).map(|session| session.quiz.clone().unwrap_or_default())
    }

    pub(crate) fn diagram_errors(&self, token: &str) -> Option<Vec<DiagramError>> {
        find(&mut self.lock(), token).map(|session| session.diagram_errors.clone())
    }

    fn thread_command(&self, token: &str, command: ThreadCommand) -> Result<(), CommandRefusal> {
        let mut sessions = self.lock();
        let session = find(&mut sessions, token).ok_or(CommandRefusal::Stale)?;
        if matches!(command, ThreadCommand::Retry { .. }) {
            session.actions.push("retry-messages".into());
        }
        session.threads.take(command)
    }

    pub(crate) fn agent_replies(&self, token: &str) -> bool {
        self.reply_with(token, |session| session.data.chat_reply())
    }

    pub(crate) fn agent_replies_wide(&self, token: &str) -> bool {
        self.reply_with(token, |_| WIDE_CHAT_REPLY)
    }

    pub(crate) fn agent_replies_with_diagram(&self, token: &str) -> bool {
        self.reply_with(token, |_| DIAGRAM_CHAT_REPLY)
    }

    fn reply_with(&self, token: &str, reply: impl FnOnce(&Session) -> &'static str) -> bool {
        let mut sessions = self.lock();
        let Some(session) = find(&mut sessions, token) else {
            return false;
        };
        let round = session.round_id();
        let text = reply(session);
        session.threads.agent_replies(&round, text)
    }

    pub(crate) fn messages_not_delivered(&self, token: &str) -> bool {
        find(&mut self.lock(), token)
            .is_some_and(|session| session.threads.not_delivered(NOT_DELIVERED))
    }

    pub(crate) fn messages(&self, token: &str) -> Option<Vec<SentMessage>> {
        find(&mut self.lock(), token).map(|session| session.threads.sent())
    }

    fn past_round(&self, token: &str, id: &str) -> Option<PastRound> {
        find(&mut self.lock(), token)?
            .past
            .iter()
            .find(|past| past.id() == id)
            .cloned()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Session>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Rounds for Sessions {
    fn find(&self, token: &str) -> Option<PageRound> {
        let opener = self.clone();
        let owner = token.to_owned();
        let open = move |id: &str| opener.past_round(&owner, id);
        let (stages, threads, past) = {
            let mut sessions = self.lock();
            let session = find(&mut sessions, token)?;
            (
                session.round.subscribe(),
                session.threads.subscribe(),
                PastRounds::new(&session.past_list, open),
            )
        };
        let sessions = self.clone();
        let owner = token.to_owned();
        let commands = CommandSender::new(move |command, reply| {
            reply.send(sessions.command(&owner, command));
        });
        let sessions = self.clone();
        let owner = token.to_owned();
        let sender = ThreadSender::new(move |command, reply| {
            reply.send(sessions.thread_command(&owner, command));
        });
        Some(
            PageRound::new(stages, commands)
                .with_conversation(PageConversation::new(threads, sender))
                .with_past_rounds(past),
        )
    }
}

fn past_rounds(session: &str, clock: Clock, data: &'static dyn RoundData) -> Vec<PastRound> {
    data.saved_rounds()
        .iter()
        .enumerate()
        .map(|(index, saved)| {
            let mut past = Session::new(
                format!("{session}-past-{}", index + 1),
                Token::random(),
                clock,
                data,
                0,
            );
            for &step in saved.steps {
                if let Some(stage) = past.stage_after(step, None) {
                    past.publish(stage);
                }
            }
            if let Some(quiz) = &mut past.quiz {
                for item in 0..data.quiz_items().len() {
                    let _ = quiz.record(&data.quiz_items(), QuizResponse::Pick { item, answer: 0 });
                }
                let stage = past.conclusion(None);
                past.publish(stage);
            }
            let review = ReviewName {
                title: saved.title.to_owned(),
                revision: ShortRevision {
                    prefix: saved.revision.0.to_owned(),
                    rest: saved.revision.1.to_owned(),
                },
                issues: Vec::new(),
                ..data.review()
            };
            past.past_at(
                review,
                saved.references(),
                clock.now_ms() - saved.minutes_ago * MINUTE_MS,
            )
        })
        .collect()
}

fn find<'a>(sessions: &'a mut [Session], token: &str) -> Option<&'a mut Session> {
    sessions
        .iter_mut()
        .find(|session| session.token.matches(token))
}
