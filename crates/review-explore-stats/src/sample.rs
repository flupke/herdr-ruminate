use crate::{proposals::ProposalCounts, run_ahead::RunAheadSample, summary::Share, words::Words};
use review_explore::{Exploration, ExploreRound, Question, ReviewerAnswer, TopicStatus};
use review_run_ahead::RoundForks;
use review_store::SavedRound;
use std::time::UNIX_EPOCH;

pub(crate) struct AgentTurn {
    pub(crate) milliseconds: u64,
    pub(crate) after_answer: bool,
    pub(crate) prepared: bool,
}

pub(crate) struct RoundSample {
    pub(crate) started_at_ms: u64,
    pub(crate) challenger: bool,
    pub(crate) answered: bool,
    pub(crate) concluded: bool,
    pub(crate) questions: usize,
    pub(crate) change_requests: Share,
    pub(crate) declined_recommendations: Share,
    pub(crate) agent_turns: Vec<AgentTurn>,
    pub(crate) reviewer_answers_ms: Vec<u64>,
    pub(crate) question_words: Vec<usize>,
    pub(crate) proposals: ProposalCounts,
    pub(crate) run_ahead: RunAheadSample,
}

impl RoundSample {
    pub(crate) fn of(saved: &SavedRound) -> Self {
        let round = &saved.round;
        let exploration = &round.exploration;
        let saved_at_ms = saved
            .saved_at
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
            });
        Self {
            started_at_ms: round
                .turns
                .values()
                .filter_map(|turn| turn.started_at_ms)
                .min()
                .unwrap_or(saved_at_ms),
            challenger: exploration.challenger,
            answered: !exploration.answers.is_empty(),
            concluded: exploration.conclusion.is_some(),
            questions: exploration.questions.len(),
            change_requests: Self::change_requests(exploration),
            declined_recommendations: Self::declined_recommendations(exploration),
            agent_turns: Self::agent_turns(exploration, &saved.run_ahead),
            reviewer_answers_ms: exploration
                .answers
                .iter()
                .filter_map(|answer| Self::reviewer_answer(round, answer))
                .collect(),
            question_words: exploration
                .questions
                .iter()
                .map(|question| Self::question_words(exploration, question))
                .collect(),
            proposals: ProposalCounts::of(exploration),
            run_ahead: RunAheadSample::of(round, &saved.run_ahead),
        }
    }

    fn change_requests(exploration: &Exploration) -> Share {
        let mut share = Share::default();
        for answer in exploration.answers.iter().filter(|a| a.question.is_some()) {
            if exploration.took_up(&answer.id) {
                share.add(
                    exploration
                        .interpretation(&answer.id)
                        .is_some_and(|interpretation| {
                            interpretation.status == TopicStatus::NeedsFollowUp
                        }),
                );
            }
        }
        share
    }

    fn declined_recommendations(exploration: &Exploration) -> Share {
        let mut share = Share::default();
        for answer in &exploration.answers {
            let recommends = answer.question.as_ref().is_some_and(|question| {
                question
                    .alternatives
                    .iter()
                    .any(|alternative| alternative.recommendation.is_some())
            });
            if recommends && !answer.drawing_board {
                share.add(
                    answer
                        .option
                        .as_ref()
                        .is_none_or(|option| option.recommendation.is_none()),
                );
            }
        }
        share
    }

    fn agent_turns(exploration: &Exploration, run_ahead: &RoundForks) -> Vec<AgentTurn> {
        exploration
            .agent_elapsed_ms
            .iter()
            .map(|(request, &milliseconds)| AgentTurn {
                milliseconds,
                prepared: run_ahead.answers.iter().any(|answer| {
                    &answer.request == request && run_ahead.used_a_prepared_turn(answer)
                }),
                after_answer: exploration
                    .conversation
                    .iter()
                    .find(|turn| &turn.update.request == request)
                    .is_some_and(|turn| turn.answer.is_some()),
            })
            .collect()
    }

    fn reviewer_answer(round: &ExploreRound, answer: &ReviewerAnswer) -> Option<u64> {
        let asked = round.turns.get(&answer.in_reply_to)?.started_at_ms?
            + round
                .exploration
                .agent_elapsed_ms
                .get(&answer.in_reply_to)?;
        let sent = round
            .turns
            .values()
            .find(|turn| {
                turn.request
                    .answer
                    .as_ref()
                    .is_some_and(|carried| carried.id == answer.id)
            })?
            .started_at_ms?;
        sent.checked_sub(asked)
    }

    fn question_words(exploration: &Exploration, question: &Question) -> usize {
        let reply = exploration
            .conversation
            .iter()
            .find(|turn| turn.update.next.as_ref() == Some(question))
            .filter(|turn| turn.revision.is_none())
            .and_then(|turn| turn.update.reply.as_ref());
        reply.words() + question.words()
    }
}
