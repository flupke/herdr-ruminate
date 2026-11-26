use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageScreen {
    Current,
    PastRounds,
}

impl PageScreen {
    pub fn fragment(self) -> &'static str {
        match self {
            Self::Current => "",
            Self::PastRounds => "#past-rounds",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageNotOpened {
    pub url: Option<String>,
    pub reason: String,
}

impl fmt::Display for PageNotOpened {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Cannot open the Explore page: {}", self.reason)
    }
}

impl std::error::Error for PageNotOpened {}
