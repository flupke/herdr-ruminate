use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Wrapper {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
}

#[cfg(target_os = "linux")]
fn exec_with_parent_death(
    parent: u32,
    program: &OsString,
    arguments: &[OsString],
) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    // SIGTERM rather than SIGKILL: Claude Code closes its MCP servers itself on SIGTERM.
    if let Err(error) = nix::sys::prctl::set_pdeathsig(nix::sys::signal::Signal::SIGTERM) {
        return error.into();
    }
    // Linux sends no parent-death signal for a parent that died before prctl armed it.
    if u32::try_from(nix::unistd::getppid().as_raw()).ok() != Some(parent) {
        return std::io::Error::other("the reviewer that started this fork is gone");
    }
    std::process::Command::new(program).args(arguments).exec()
}

#[cfg(not(target_os = "linux"))]
fn exec_with_parent_death(
    _parent: u32,
    _program: &OsString,
    _arguments: &[OsString],
) -> std::io::Error {
    std::io::Error::other("forks run only on Linux, which has a parent-death signal")
}

pub fn run_wrapper(mut arguments: impl Iterator<Item = OsString>) -> std::io::Error {
    let parent = arguments
        .next()
        .and_then(|parent| parent.into_string().ok())
        .and_then(|parent| parent.parse().ok());
    let (Some(parent), Some(program)) = (parent, arguments.next()) else {
        return std::io::Error::other("usage: <reviewer pid> <program> [arguments...]");
    };
    let arguments: Vec<_> = arguments.collect();
    exec_with_parent_death(parent, &program, &arguments)
}
