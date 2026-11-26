use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartBlock {
    NothingToReview,
}

impl StartBlock {
    pub fn of(unreviewed: impl IntoIterator<Item = bool>) -> Option<Self> {
        let mut changed = false;
        for unreviewed in unreviewed {
            if unreviewed {
                return None;
            }
            changed = true;
        }
        changed.then_some(Self::NothingToReview)
    }

    pub fn reason(self) -> &'static str {
        match self {
            Self::NothingToReview => {
                "Nothing is left to review: every changed line is marked as reviewed. Unmark \
                 lines or files to start a round."
            }
        }
    }
}

impl fmt::Display for StartBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

#[cfg(test)]
mod tests {
    use super::StartBlock;

    #[test]
    fn a_round_needs_a_changed_file_with_unreviewed_lines() {
        assert_eq!(
            StartBlock::of([false, false]),
            Some(StartBlock::NothingToReview)
        );
        assert_eq!(StartBlock::of([false, true]), None);
        assert_eq!(StartBlock::of([true]), None);
        assert_eq!(StartBlock::of([]), None, "a change with no file");
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RoundChoices {
    pub challenger: bool,
    pub author: crate::ChangeAuthor,
    pub reviewer: Option<String>,
}

impl RoundChoices {
    pub const NAME_LENGTH: usize = 80;

    pub fn new(challenger: bool, author: crate::ChangeAuthor, name: &str) -> Self {
        let name: String = name
            .split(['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}'])
            .next()
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_control())
            .collect::<String>()
            .trim()
            .chars()
            .take(Self::NAME_LENGTH)
            .collect();
        Self {
            challenger,
            author,
            reviewer: (author == crate::ChangeAuthor::Other && !name.is_empty()).then_some(name),
        }
    }
}

impl crate::Exploration {
    pub fn choose(&mut self, choices: RoundChoices) {
        self.challenger = choices.challenger;
        self.author = choices.author;
        self.reviewer = choices.reviewer;
    }
}
