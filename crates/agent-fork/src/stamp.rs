use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessStamp {
    pub pid: u32,
    pub started: u64,
}

impl ProcessStamp {
    // Linux keeps a zombie (Z) or dead (X) process in /proc until its parent reaps it.
    pub fn of(pid: u32) -> Option<Self> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let (state, started) = state_and_start_time(&stat)?;
        (!matches!(state, "Z" | "X")).then_some(Self { pid, started })
    }

    pub fn read(pid: u32) -> Self {
        Self::of(pid).unwrap_or(Self { pid, started: 0 })
    }

    pub fn is_running(&self) -> bool {
        Self::of(self.pid).is_some_and(|now| now.started == self.started)
    }

    pub fn runs_with(&self, needle: &str) -> bool {
        self.is_running()
            && fs::read(format!("/proc/{}/cmdline", self.pid)).is_ok_and(|command| {
                command
                    .split(|byte| *byte == 0)
                    .any(|argument| argument == needle.as_bytes())
            })
    }
}

pub fn leads_terminal_foreground(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| leads_terminal(&stat))
}

// Linux's /proc/<pid>/stat: field 5 is the process group and field 8 the foreground group of
// the terminal (-1 without one); the command name, field 2, may hold spaces and parentheses.
fn leads_terminal(stat: &str) -> bool {
    let (Some((pid, _)), Some((_, after_name))) = (stat.split_once(' '), stat.rsplit_once(')'))
    else {
        return false;
    };
    let fields: Vec<&str> = after_name.split_whitespace().take(6).collect();
    let [_state, _parent, group, _session, _terminal, foreground] = fields[..] else {
        return false;
    };
    pid == group && group == foreground
}

// Linux's /proc/<pid>/stat: field 3 is the state and field 22 the start time; the command
// name, field 2, is in parentheses and may hold spaces and parentheses.
fn state_and_start_time(stat: &str) -> Option<(&str, u64)> {
    let (_, after_name) = stat.rsplit_once(')')?;
    let mut fields = after_name.split_whitespace();
    let state = fields.next()?;
    Some((state, fields.nth(18)?.parse().ok()?))
}

#[cfg(test)]
#[path = "stamp.tests.rs"]
mod tests;
