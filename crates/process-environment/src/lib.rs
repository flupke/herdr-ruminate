//! The environment a running process started with, as `/proc/<pid>/environ` holds it, less the
//! variables of Herdr and of herdr-core. Those name the pane or terminal the process runs in, so
//! a process that takes the rest runs as itself, not as that pane's program. The reviewer's own
//! settings, `HERDR_REVIEWER_*` such as its MCP port, are the user's, and stay.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStringExt;

/// The variables process `pid` started with, except those of Herdr and of herdr-core.
///
/// # Errors
/// Fails when the process is gone or belongs to another user.
pub fn read(pid: u32) -> io::Result<Vec<(OsString, OsString)>> {
    Ok(parse(&fs::read(format!("/proc/{pid}/environ"))?))
}

/// Whether `name` is a variable of Herdr or of herdr-core, not one of the reviewer's settings.
fn is_host_variable(name: &[u8]) -> bool {
    (name.starts_with(b"HERDR_") && !name.starts_with(b"HERDR_REVIEWER_"))
        || name.starts_with(b"HC_")
}

/// The variables of `environ`, as `/proc/<pid>/environ` holds them, except those of Herdr and of
/// herdr-core.
fn parse(environ: &[u8]) -> Vec<(OsString, OsString)> {
    environ
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            // As the standard library reads its own: a name is never empty, so the `=` that ends
            // it comes after the first byte, and an entry with no such `=` is no variable.
            let at = 1 + entry.get(1..)?.iter().position(|byte| *byte == b'=')?;
            let (name, value) = entry.split_at(at);
            (!is_host_variable(name)).then(|| {
                (
                    OsString::from_vec(name.to_vec()),
                    OsString::from_vec(value[1..].to_vec()),
                )
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
