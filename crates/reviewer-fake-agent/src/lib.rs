use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

#[allow(
    clippy::needless_update,
    reason = "herdr-common's next field of App would otherwise break the build of tests/tui, which takes fake-agent by path"
)]
pub fn app() -> fake_agent::App {
    fake_agent::App {
        hook_arguments: &[claude_hooks::SUBCOMMAND],
        read_prompt: review_test_support::turn::with_turn,
        ..fake_agent::App::default()
    }
}

const PROGRAM: &str = "reviewer-fake-agent";

pub fn program(profile: &Path) -> Result<PathBuf, String> {
    let program = profile.join(PROGRAM);
    if program.is_file() {
        Ok(program)
    } else {
        Err(format!(
            "{} is not built: run `cargo build -p {PROGRAM}`, as `make test` does",
            program.display()
        ))
    }
}

// Herdr detects an agent by the name of its program.
pub fn install(program: &Path, agent: &Path) -> std::io::Result<()> {
    symlink(program, agent)
}
