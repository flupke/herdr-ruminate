use crate::{Alternative, Question, TopicStatus};
use std::sync::LazyLock;

static NONE_OF_THE_ABOVE: LazyLock<Alternative> = LazyLock::new(|| Alternative {
    id: "none-of-the-above".into(),
    text: "None of the above".into(),
    outcome: TopicStatus::Open,
    recommendation: None,
});

impl Question {
    pub fn choices(&self) -> impl Iterator<Item = &Alternative> {
        self.alternatives
            .iter()
            .chain(std::iter::once(&*NONE_OF_THE_ABOVE))
    }

    pub fn choice(&self, id: &str) -> Option<&Alternative> {
        self.choices().find(|choice| choice.id == id)
    }
}

impl Alternative {
    pub fn is_none_of_the_above(&self) -> bool {
        self.id == NONE_OF_THE_ABOVE.id
    }

    pub(crate) fn recommended_mark(&self) -> Option<&str> {
        const MARK: &str = "recommended";
        let text = self.text.trim_end().trim_end_matches(['.', '!']).trim_end();
        let open = text.strip_suffix(')')?.rfind('(')?;
        let inside = text[open + 1..text.len() - 1].trim();
        let rest = inside
            .get(..MARK.len())
            .filter(|word| word.eq_ignore_ascii_case(MARK))
            .map(|_| &inside[MARK.len()..])?;
        rest.chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric() && !next.is_whitespace())
            .then(|| &text[open..])
    }
}
