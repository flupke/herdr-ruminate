use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStringExt;

pub fn read(pid: u32) -> io::Result<Vec<(OsString, OsString)>> {
    Ok(parse(&fs::read(format!("/proc/{pid}/environ"))?))
}

// Herdr sets `HERDR_*` variables, and herdr-core `HC_*` ones.
fn is_host_variable(name: &[u8]) -> bool {
    (name.starts_with(b"HERDR_") && !name.starts_with(b"HERDR_REVIEWER_"))
        || name.starts_with(b"HC_")
}

fn parse(environ: &[u8]) -> Vec<(OsString, OsString)> {
    environ
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            // As std splits environ entries: a name is never empty, so the `=` that ends it
            // comes after the first byte.
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
