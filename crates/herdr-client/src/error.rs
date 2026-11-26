use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("required environment variable {name} is not set")]
    Environment { name: &'static str },

    #[error("{operation} failed at {path:?}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{operation} returned invalid JSON: {source}")]
    Json {
        operation: &'static str,
        #[source]
        source: serde_json::Error,
    },

    #[error("the agent did not start on the prompt: {message}")]
    AgentNotStarted { message: String },

    #[error("nothing reached the agent: {reason}")]
    NotTyped { reason: String },

    #[error("cancelled before the host acted on it")]
    Cancelled,

    #[error("{operation} failed: {message}")]
    Herdr {
        operation: &'static str,
        message: String,
    },

    #[error("{operation} returned an invalid response: {detail}")]
    Protocol {
        operation: String,
        detail: &'static str,
    },
}
