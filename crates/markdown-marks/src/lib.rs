#[cfg(test)]
mod tests;

pub trait Mark: Copy + Sized + 'static {
    const ALL: &'static [Self];

    fn spelling(self) -> Spelling;

    fn marker(self) -> &'static str {
        self.spelling().marker
    }

    fn label(self) -> &'static str {
        self.spelling().label
    }

    fn name(self) -> &'static str {
        self.spelling().name
    }

    fn strip(text: &str) -> Option<(Self, &str)> {
        let text = text.trim_start();
        Self::ALL.iter().find_map(|&mark| {
            let marker = mark.marker();
            let opening = text.get(..marker.len())?;
            opening
                .eq_ignore_ascii_case(marker)
                .then(|| (mark, text[marker.len()..].trim_start()))
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Spelling {
    marker: &'static str,
    label: &'static str,
    name: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Callout {
    Conclusion,
    Tip,
    Warning,
    Error,
}

impl Mark for Callout {
    const ALL: &'static [Self] = &[Self::Conclusion, Self::Tip, Self::Warning, Self::Error];

    fn spelling(self) -> Spelling {
        let (marker, label, name) = match self {
            Self::Conclusion => ("[!CONCLUSION]", "Conclusion", "conclusion"),
            Self::Tip => ("[!TIP]", "Tip", "tip"),
            Self::Warning => ("[!WARNING]", "Warning", "warning"),
            Self::Error => ("[!ERROR]", "Error", "error"),
        };
        Spelling {
            marker,
            label,
            name,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusMark {
    Good,
    Bad,
    Warning,
}

impl StatusMark {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Good => "✓",
            Self::Bad => "✗",
            Self::Warning => "!",
        }
    }
}

impl Mark for StatusMark {
    const ALL: &'static [Self] = &[Self::Good, Self::Bad, Self::Warning];

    fn spelling(self) -> Spelling {
        let (marker, label, name) = match self {
            Self::Good => ("[!good]", "Good", "good"),
            Self::Bad => ("[!bad]", "Bad", "bad"),
            Self::Warning => ("[!warning]", "Warning", "warning"),
        };
        Spelling {
            marker,
            label,
            name,
        }
    }
}
