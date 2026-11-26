use std::ffi::OsString;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("jj-lib could not {operation}")]
    JjLibrary { operation: &'static str },

    #[error("{operation} failed at {path:?}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{operation} could not start {program:?} in {current_dir:?}: {source}")]
    Spawn {
        operation: String,
        program: OsString,
        current_dir: Option<PathBuf>,
        #[source]
        source: std::io::Error,
    },

    #[error("{operation} failed with exit code {code:?}")]
    CommandFailed {
        operation: String,
        code: Option<i32>,
    },

    #[error("several commits have this change ID (a divergent change)")]
    DivergentChange,

    #[error("{operation} produced more than 256 MiB of output in {path:?}")]
    CommandOutputTooLarge { operation: String, path: PathBuf },

    #[error("{operation} was canceled in {path:?}")]
    CommandCancelled { operation: String, path: PathBuf },

    #[error("{path} is not in a jj or Git repository")]
    NotRepository { path: PathBuf },

    #[error("{operation} returned an invalid response: {detail}")]
    Protocol {
        operation: String,
        detail: &'static str,
    },
}
