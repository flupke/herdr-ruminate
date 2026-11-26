use commands::Refusal;
use shell::ShellError;

mod commands;
mod sed;
mod shell;

// Claude Code and Codex name an MCP tool `mcp__<server>__<tool>`.
pub const SUBMITS: &[&str] = &[
    "mcp__herdr_reviewer__submit_question",
    "mcp__herdr_reviewer__submit_conclusion",
];

pub fn command_refusal(command: &str) -> Option<String> {
    let refusal = match shell::split_pipe(command) {
        Ok(pipe) => pipe.iter().find_map(|words| commands::refusal(words))?,
        Err(error) => return Some(shell_refusal(command, &error)),
    };
    Some(match refusal {
        Refusal::NotReading => format!(
            "`{command}` is not allowed: this session only reads code (jj and git log, diff, \
             show; jj file show and list; rg; cat; sed -n with p; ls and the like), one \
             command or a pipe."
        ),
        Refusal::Snapshot => format!(
            "`{command}` is not allowed: run jj with --ignore-working-copy, as in \
             `jj --ignore-working-copy log`, so that it records no operation."
        ),
    })
}

fn shell_refusal(command: &str, error: &ShellError) -> String {
    let reason = match error {
        ShellError::Form('`') => "a backtick outside single quotes".to_owned(),
        ShellError::Form('$') => "`$` outside single quotes".to_owned(),
        ShellError::Form(form) => format!("`{form}` outside quotes"),
        ShellError::LeadingGlob(glob) => {
            format!("`{glob}` outside quotes at the start of a word or in an option")
        }
        ShellError::Control => "a newline or another control character".to_owned(),
        ShellError::Unclosed => "an unclosed quote".to_owned(),
        ShellError::Empty => "an empty command".to_owned(),
    };
    format!(
        "`{command}` is not allowed: it holds {reason}. This session runs one reading command \
         or a pipe, with no list, substitution or redirection but 2>/dev/null and 2>&1; quote \
         a pattern that holds one of these characters."
    )
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
