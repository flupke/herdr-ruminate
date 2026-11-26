use std::fmt;
use std::time::Duration;

use agent_fork::Exit;

const INSTALL: &str =
    "https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TunnelFailure {
    Missing { program: String },
    NotStarted(String),
    Exited(Exit),
    NoAddress(Duration),
    Ended(Exit),
}

impl fmt::Display for TunnelFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { program } => write!(
                formatter,
                "{program} is not installed, install cloudflared from your package manager or \
                 {INSTALL}"
            ),
            Self::NotStarted(error) => write!(formatter, "cloudflared did not start ({error})"),
            Self::Exited(exit) => write!(
                formatter,
                "cloudflared stopped before it gave an address ({})",
                last_words(exit)
            ),
            Self::NoAddress(wait) => write!(
                formatter,
                "cloudflared gave no address within {} seconds",
                wait.as_secs()
            ),
            Self::Ended(exit) => write!(formatter, "cloudflared stopped ({})", last_words(exit)),
        }
    }
}

impl std::error::Error for TunnelFailure {}

fn last_words(exit: &Exit) -> &str {
    exit.stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or(&exit.status)
}
