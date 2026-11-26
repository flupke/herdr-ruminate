use crate::summary::{Share, Sorted};
use review_explore::ExploreRound;
use review_run_ahead::{
    AnswerRecord, DiscardReason, ForkRecord, PlainReason, RoundForks, TokenUsage, TurnPath,
};
use std::{collections::BTreeMap, time::Duration};

#[derive(Default)]
pub(crate) struct RunAheadSample {
    ran: bool,
    answers: Share,
    not_prepared: Vec<&'static str>,
    prepared_waits_ms: Vec<u64>,
    plain_waits_ms: Vec<u64>,
    forks: usize,
    ready: Share,
    used_tokens: TokenUsage,
    discarded_tokens: TokenUsage,
}

impl RunAheadSample {
    pub(crate) fn of(round: &ExploreRound, record: &RoundForks) -> Self {
        let mut sample = Self {
            ran: !record.forks.is_empty() || !record.answers.is_empty(),
            forks: record.forks.len(),
            ..Self::default()
        };
        for answer in &record.answers {
            sample.add_answer(round, record, answer);
        }
        for fork in &record.forks {
            sample.add_fork(record, fork);
        }
        sample
    }

    fn add_answer(&mut self, round: &ExploreRound, record: &RoundForks, answer: &AnswerRecord) {
        let used = record.used_a_prepared_turn(answer);
        self.answers.add(used);
        if !used {
            self.not_prepared.push(reason(&answer.path));
        }
        let waits = if used {
            &mut self.prepared_waits_ms
        } else {
            &mut self.plain_waits_ms
        };
        waits.extend(wait_ms(round, answer));
    }

    fn add_fork(&mut self, record: &RoundForks, fork: &ForkRecord) {
        if let Some(ready) = ready_at_the_answer(record, fork) {
            self.ready.add(ready);
        }
        let usage = fork.usage.unwrap_or_default();
        if fork.is_continued() {
            self.used_tokens += usage;
        } else {
            self.discarded_tokens += usage;
        }
    }
}

fn wait_ms(round: &ExploreRound, answer: &AnswerRecord) -> Option<u64> {
    let started = round.turns.get(&answer.request)?.started_at_ms?;
    let elapsed = round.exploration.agent_elapsed_ms.get(&answer.request)?;
    started.saturating_add(*elapsed).checked_sub(answer.at_ms)
}

fn ready_at_the_answer(record: &RoundForks, fork: &ForkRecord) -> Option<bool> {
    let answered_ms = record
        .answers
        .iter()
        .filter(|answer| {
            answer.question == fork.question
                && answer.version == fork.version
                && answer.at_ms >= fork.taken_at_ms
        })
        .map(|answer| answer.at_ms)
        .min()?;
    if fork.discarded.as_ref().is_some_and(|discard| {
        discard.reason != DiscardReason::Answered && discard.at_ms < answered_ms
    }) {
        return None;
    }
    if let Some(submitted) = fork.submitted_at_ms {
        return Some(submitted <= answered_ms);
    }
    let discard = fork.discarded.as_ref();
    let submitted = fork.turn.is_some()
        || fork.continued.is_some()
        || discard.is_some_and(|discard| discard.prepared);
    if !submitted {
        return Some(false);
    }
    discard
        .is_some_and(|discard| discard.at_ms <= answered_ms)
        .then_some(true)
}

fn reason(path: &TurnPath) -> &'static str {
    match path {
        TurnPath::Prepared { .. } => "prepared turn not settled",
        TurnPath::Plain { reason } => plain_reason(reason),
    }
}

fn plain_reason(reason: &PlainReason) -> &'static str {
    match reason {
        PlainReason::NoHooks => "agent without the plugin",
        PlainReason::Comment => "a comment",
        PlainReason::NoneOfTheAbove => "None of the above",
        PlainReason::DrawingBoard => "back to the drawing board",
        PlainReason::ChatMessage => "round conversation message",
        PlainReason::AgentBusy => "agent working",
        PlainReason::SessionMoved => "agent's session moved",
        PlainReason::UnreviewedChanged => "lines to review changed",
        PlainReason::PromptChanged => "round changed",
        other => run_ahead_reason(other),
    }
}

fn run_ahead_reason(reason: &PlainReason) -> &'static str {
    match reason {
        PlainReason::NoForks { .. } => "no fork taken",
        PlainReason::NotForked => "choice not forked",
        PlainReason::StillWorking => "fork not ready",
        PlainReason::NoTurn => "fork ended without a turn",
        PlainReason::StoppedWaiting => "stopped waiting",
        PlainReason::Unchecked { .. } => "check not made",
        PlainReason::SwitchFailed { .. } => "switch failed",
        PlainReason::Withdrawn => "withdrawn while switching",
        PlainReason::Obsolete
        | PlainReason::NoHooks
        | PlainReason::Comment
        | PlainReason::NoneOfTheAbove
        | PlainReason::DrawingBoard
        | PlainReason::ChatMessage
        | PlainReason::AgentBusy
        | PlainReason::SessionMoved
        | PlainReason::UnreviewedChanged
        | PlainReason::PromptChanged => "an earlier version's reason",
    }
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct RunAheadSummary {
    pub(crate) rounds: usize,
    pub(crate) answers: Share,
    pub(crate) not_prepared: BTreeMap<&'static str, usize>,
    pub(crate) prepared_wait: Option<Duration>,
    pub(crate) plain_wait: Option<Duration>,
    pub(crate) forks: usize,
    pub(crate) ready: Share,
    pub(crate) used_tokens: TokenUsage,
    pub(crate) discarded_tokens: TokenUsage,
}

impl RunAheadSummary {
    pub(crate) fn of<'a>(samples: impl IntoIterator<Item = &'a RunAheadSample> + Clone) -> Self {
        let mut summary = Self {
            prepared_wait: Sorted::median_time(
                samples
                    .clone()
                    .into_iter()
                    .flat_map(|sample| sample.prepared_waits_ms.iter().copied()),
            ),
            plain_wait: Sorted::median_time(
                samples
                    .clone()
                    .into_iter()
                    .flat_map(|sample| sample.plain_waits_ms.iter().copied()),
            ),
            ..Self::default()
        };
        for sample in samples {
            summary.rounds += usize::from(sample.ran);
            summary.answers = Share::sum([summary.answers, sample.answers]);
            for reason in &sample.not_prepared {
                *summary.not_prepared.entry(reason).or_default() += 1;
            }
            summary.forks += sample.forks;
            summary.ready = Share::sum([summary.ready, sample.ready]);
            summary.used_tokens += sample.used_tokens;
            summary.discarded_tokens += sample.discarded_tokens;
        }
        summary
    }
}
