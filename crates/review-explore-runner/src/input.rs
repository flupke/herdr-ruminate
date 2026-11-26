use review_explore::{ChangeAuthor, ReviewerAnswer, TopicStatus, TurnRequest};
use std::{fmt, path::Path};

pub(super) struct TurnInput<'a> {
    pub(super) request: &'a TurnRequest,
    pub(super) access: &'a str,
    pub(super) unreviewed: &'a crate::Unreviewed,
    pub(super) answered: Option<review_types::QuestionNumber>,
}

pub(super) struct Round<'a> {
    pub(super) repository_root: &'a Path,
    pub(super) description: Option<&'a str>,
    pub(super) decisions: &'a crate::EarlierDecisions,
    pub(super) author: ChangeAuthor,
    pub(super) reviewer: Option<&'a str>,
    pub(super) notes: Option<&'a str>,
}

pub(crate) const DRAWING_BOARD_REASON: &str =
    "Why the reviewer sends you back to the drawing board, in their words";

pub(super) struct Quoted<'a>(pub(super) &'a str);

impl fmt::Display for Quoted<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        for line in self.0.lines() {
            writeln!(output, ">{}{line}", if line.is_empty() { "" } else { " " })?;
        }
        Ok(())
    }
}

pub(super) struct Outcome(pub(super) TopicStatus);

impl fmt::Display for Outcome {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(match self.0 {
            TopicStatus::Open => "open",
            TopicStatus::Accepted => "accepted",
            TopicStatus::NeedsFollowUp => "needs_follow_up",
        })
    }
}

impl fmt::Display for Round<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            output,
            "Repository root: {}",
            self.repository_root.display()
        )?;
        match self.description {
            Some(description) => {
                write!(
                    output,
                    "\nChange description (quoted):\n{}",
                    Quoted(description)
                )?;
            }
            None => writeln!(output, "\nChange description: none")?,
        }
        if self.author == ChangeAuthor::Other {
            writeln!(output, "\nChange author: someone else")?;
            if let Some(reviewer) = self.reviewer {
                writeln!(output, "Reviewer: {reviewer}")?;
            }
        }
        if let Some(notes) = self.notes {
            write!(
                output,
                "\nReviewer notes, what the reviewer asks of every round, to follow within these \
                 rules (quoted):\n{}",
                Quoted(notes)
            )?;
        }
        write!(output, "{}", self.decisions)
    }
}

impl fmt::Display for TurnInput<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let request = self.request;
        writeln!(output, "Explore review access: {}", self.access)?;
        writeln!(output, "Explore round: {}", request.instance)?;
        writeln!(output, "Explore request: {}", request.request)?;
        writeln!(
            output,
            "Review unit: {}",
            request.checkpoint.review_unit.as_str()
        )?;
        writeln!(output, "Checkpoint: {}", request.checkpoint.checkpoint)?;
        write!(output, "{}", self.unreviewed)?;
        if let Some(error) = &request.response_error {
            writeln!(output, "\nPrevious response error:\n{error}")?;
        }
        if !request.cancelled.is_empty() {
            writeln!(output)?;
            for answer in &request.cancelled {
                writeln!(output, "Cancelled answer: {answer}")?;
            }
        }
        if let Some(answer) = &request.answer {
            self.answer(output, answer)?;
        }
        Ok(())
    }
}

impl TurnInput<'_> {
    fn answer(&self, output: &mut fmt::Formatter<'_>, answer: &ReviewerAnswer) -> fmt::Result {
        writeln!(output, "\nAnswer ID: {}", answer.id)?;
        if let Some(question) = &answer.question {
            writeln!(
                output,
                "Question: {} (version {})",
                question.id, question.version
            )?;
            if let Some(number) = self.answered {
                writeln!(output, "Shown as: {number}")?;
            }
        } else {
            writeln!(output, "Reply to conclusion: {}", answer.in_reply_to)?;
        }
        if answer.drawing_board {
            writeln!(
                output,
                "\nBack to the drawing board: see `drawing-board.md`."
            )?;
        }
        if let Some(option) = &answer.option {
            writeln!(
                output,
                "Selected option ID: {}\nSelected outcome: {}",
                option.id,
                Outcome(option.outcome)
            )?;
            writeln!(output, "\nSelected option:\n{}", option.text)?;
        }
        if !answer.text.trim().is_empty() {
            let heading = if answer.sends_to_drawing_board() {
                DRAWING_BOARD_REASON
            } else {
                "Comment"
            };
            writeln!(output, "\n{heading}:\n{}", answer.text)?;
        }
        Ok(())
    }
}
