use serde::Serialize;

use crate::{Assessments, Consequence, Question};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QuestionSection {
    pub title: &'static str,
    pub lead: String,
    pub details: String,
}

impl QuestionSection {
    pub fn body(&self) -> String {
        if self.details.is_empty() {
            return self.lead.clone();
        }
        format!("{}\n\n{}", self.lead, self.details)
    }
}

impl Question {
    pub fn context(&self) -> String {
        [&self.rationale, &self.visual]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl Assessments {
    pub fn sections(&self) -> [QuestionSection; 2] {
        [
            QuestionSection {
                title: "Door",
                lead: format!("{} — {}", self.door.label(), self.reversibility.summary),
                details: Self::details(&self.reversibility),
            },
            QuestionSection {
                title: "Blast radius",
                lead: self.blast_radius.summary.clone(),
                details: Self::details(&self.blast_radius),
            },
        ]
    }

    fn details(lens: &Consequence) -> String {
        let unknowns = lens
            .unknowns
            .iter()
            .map(|unknown| format!("Unknown: {unknown}"));
        (!lens.details.trim().is_empty())
            .then(|| lens.details.clone())
            .into_iter()
            .chain(unknowns)
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}
