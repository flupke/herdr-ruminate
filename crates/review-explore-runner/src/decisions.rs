use crate::input::{Outcome, Quoted};
use review_explore::{Exploration, Interpretation, ReviewerAnswer, TopicStatus};
use std::fmt;

#[derive(Clone, Debug, Default)]
pub struct EarlierDecisions {
    reset_rounds: usize,
    decisions: Vec<Decision>,
}

#[derive(Clone, Debug)]
struct Decision {
    answer: ReviewerAnswer,
    interpretation: Option<Interpretation>,
}

impl EarlierDecisions {
    pub fn new<'a>(rounds: impl IntoIterator<Item = &'a Exploration>) -> Self {
        Self {
            reset_rounds: 0,
            decisions: rounds
                .into_iter()
                .flat_map(|round| {
                    round
                        .answers
                        .iter()
                        .filter_map(|answer| Decision::of(round, answer))
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn after_reset(self, reset_rounds: usize) -> Self {
        Self {
            reset_rounds,
            ..self
        }
    }
}

impl Decision {
    fn of(round: &Exploration, answer: &ReviewerAnswer) -> Option<Self> {
        answer.question.as_ref()?;
        let interpretation = round.interpretation(&answer.id);
        let decides = match interpretation {
            Some(interpretation) => interpretation.status != TopicStatus::Open,
            None if round.took_up(&answer.id) => false,
            None => answer
                .option
                .as_ref()
                .is_some_and(|option| option.outcome != TopicStatus::Open),
        };
        decides.then(|| Self {
            answer: answer.clone(),
            interpretation: interpretation.cloned(),
        })
    }
}

impl fmt::Display for EarlierDecisions {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.reset_rounds > 0 {
            writeln!(output, "\nReset rounds: {}", self.reset_rounds)?;
            writeln!(
                output,
                "The reviewer reset these rounds and started over: whatever this conversation \
                 holds of them is void. Build this round from the change alone."
            )?;
        }
        if self.decisions.is_empty() {
            return Ok(());
        }
        writeln!(
            output,
            "\nEarlier decisions of this review, oldest first (text quoted):"
        )?;
        writeln!(
            output,
            "The reviewer settled these in earlier rounds. Build on them, and ask one again only \
             when the change now contradicts it. Their Answer IDs are those rounds': name such a \
             decision by its subject."
        )?;
        for decision in &self.decisions {
            write!(output, "{decision}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Decision {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let answer = &self.answer;
        let Some(question) = &answer.question else {
            return Ok(());
        };
        write!(
            output,
            "\nDecided answer: {} (question {} version {})\n{}",
            answer.id,
            question.id,
            question.version,
            Quoted(&question.text)
        )?;
        match &answer.option {
            Some(option) => write!(output, "Choice: {}\n{}", option.id, Quoted(&option.text))?,
            None => writeln!(output, "Choice: none")?,
        }
        if !answer.text.trim().is_empty() {
            write!(output, "Comment:\n{}", Quoted(&answer.text))?;
        }
        match (&self.interpretation, &answer.option) {
            (Some(interpretation), _) => {
                writeln!(output, "Outcome: {}", Outcome(interpretation.status))?;
                for follow_up in &interpretation.follow_ups {
                    write!(output, "Follow-up:\n{}", Quoted(follow_up))?;
                }
            }
            (None, Some(option)) => writeln!(
                output,
                "Outcome: {} (the option's; never interpreted)",
                Outcome(option.outcome)
            )?,
            (None, None) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "decisions.tests.rs"]
mod tests;
