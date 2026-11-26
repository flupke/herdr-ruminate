use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use crate::stamp::ProcessStamp;

#[derive(Clone, Copy, Debug)]
pub(crate) struct StopWaits {
    pub(crate) term: Duration,
    pub(crate) kill: Duration,
}

impl Default for StopWaits {
    fn default() -> Self {
        Self {
            term: Duration::from_secs(3),
            kill: Duration::from_secs(2),
        }
    }
}

const RECORDED_POLL: Duration = Duration::from_millis(50);

pub(crate) fn signal(stamp: ProcessStamp, signal: Signal) {
    if !stamp.is_running() {
        return;
    }
    if let Ok(pid) = i32::try_from(stamp.pid) {
        let _ = kill(Pid::from_raw(pid), signal);
    }
}

pub fn stop_recorded(stamp: ProcessStamp, needle: &str) -> bool {
    if !stamp.runs_with(needle) {
        return false;
    }
    let waits = StopWaits::default();
    signal(stamp, Signal::SIGTERM);
    if !ended_within(stamp, waits.term) {
        signal(stamp, Signal::SIGKILL);
        ended_within(stamp, waits.kill);
    }
    true
}

// Only a process's parent can wait for it; a Linux (5.3 on) pidfd turns readable once its
// process ends.
fn ended_within(stamp: ProcessStamp, limit: Duration) -> bool {
    #[cfg(target_os = "linux")]
    if let Some(ended) = descriptor::ended_within(stamp, limit) {
        return ended;
    }
    let deadline = Instant::now() + limit;
    while stamp.is_running() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(RECORDED_POLL);
    }
    true
}

#[cfg(target_os = "linux")]
mod descriptor {
    use std::time::{Duration, Instant};

    use rustix::event::{PollFd, PollFlags, Timespec, poll};
    use rustix::io::Errno;
    use rustix::process::{Pid, PidfdFlags, pidfd_open};

    use crate::stamp::ProcessStamp;

    pub(super) fn ended_within(stamp: ProcessStamp, limit: Duration) -> Option<bool> {
        let pid = Pid::from_raw(i32::try_from(stamp.pid).ok()?)?;
        let process = match pidfd_open(pid, PidfdFlags::empty()) {
            Ok(process) => process,
            Err(Errno::SRCH) => return Some(true),
            Err(_) => return None,
        };
        // Linux may have reused the PID: the pidfd names the stamped process only if that one
        // still runs after pidfd_open.
        if !stamp.is_running() {
            return Some(true);
        }
        let deadline = Instant::now() + limit;
        loop {
            let left =
                Timespec::try_from(deadline.saturating_duration_since(Instant::now())).ok()?;
            let mut descriptors = [PollFd::new(&process, PollFlags::IN)];
            match poll(&mut descriptors, Some(&left)) {
                Ok(ready) => return Some(ready > 0),
                Err(Errno::INTR) => {}
                Err(_) => return None,
            }
        }
    }
}
