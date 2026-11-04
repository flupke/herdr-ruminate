use test_case::test_case;

use super::*;

#[test]
fn the_start_time_is_read_after_a_command_name_with_spaces_and_parentheses() {
    let stat = "4242 (a (strange) name) S 1 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 987654 1000 10";

    assert_eq!(state_and_start_time(stat), Some(("S", 987_654)));
}

#[test_case("4242 (a (strange) name) S 1 4242 4100 34834 4242 0", true; "the leader of the foreground group of its terminal")]
#[test_case("4250 (claude) S 4242 4242 4100 34834 4242 0", false; "a child in the agent's group, as an MCP server")]
#[test_case("4242 (claude) S 1 4242 4100 34834 4300 0", false; "another group in the foreground")]
#[test_case("4242 (bash) S 1 4242 4242 0 -1 0", false; "no terminal, as a tool Claude Code runs")]
#[test_case("4242 (bash)", false; "a truncated line")]
fn a_process_leads_its_terminal_when_its_group_is_in_the_foreground(stat: &str, leads: bool) {
    assert_eq!(leads_terminal(stat), leads);
}

#[test]
fn this_process_runs_and_its_stamp_names_it() {
    let stamp = ProcessStamp::of(std::process::id()).unwrap();

    assert!(stamp.is_running());
    assert!(
        !ProcessStamp {
            started: stamp.started + 1,
            ..stamp
        }
        .is_running()
    );
}

#[test]
fn a_process_runs_with_an_argument_only_when_its_command_line_holds_it_whole() {
    use nix::sys::wait::{Id, WaitPidFlag, waitid};

    use std::io::BufRead;
    use std::process::{Command, Stdio};

    // The start may return before the kernel shows the new command line; the shell prints once
    // it runs its script.
    let mut child = Command::new("sh")
        .args(["-c", "echo ready; read line", "30"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready, "ready\n");
    let stamp = ProcessStamp::of(child.id()).unwrap();

    assert!(stamp.runs_with("30"));
    assert!(!stamp.runs_with("3"));
    child.kill().unwrap();
    // Wait until it ended, but leave it unreaped: a zombie no longer runs.
    waitid(
        Id::Pid(nix::unistd::Pid::from_raw(
            i32::try_from(child.id()).unwrap(),
        )),
        WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT,
    )
    .unwrap();
    assert!(!stamp.is_running());
    child.wait().unwrap();
}
