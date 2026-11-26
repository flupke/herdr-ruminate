use test_case::test_case;

use super::*;
use crate::{Post, ReviewThreads};

fn failed(through: u64) -> WakeupNotice {
    WakeupNotice::Failed(WakeupFailure {
        through,
        error: "Focus an agent".into(),
    })
}

fn waits(through: u64) -> WakeupNotice {
    WakeupNotice::Waiting(WakeupWait {
        through,
        reason: "the agent's input line shows text someone typed".into(),
    })
}

fn waiting() -> ReviewThreads {
    let mut book = ReviewThreads::new("change".into());
    book.post(Post::to_round("round-1", "Why a lock?".into(), None, None))
        .unwrap();
    book
}

#[test_case(failed; "a failed wakeup")]
#[test_case(waits; "a wakeup that waits")]
fn a_wakeup_covers_an_open_thread_whose_comment_it_reached_back_to(
    notice: fn(u64) -> WakeupNotice,
) {
    let book = waiting();
    let thread = &book.threads()[0];
    let comment = thread.last_comment().unwrap().sequence();

    assert!(notice(comment).covers(thread));
    assert!(
        !notice(comment - 1).covers(thread),
        "a comment posted after it"
    );
}

#[test_case(failed; "a failed wakeup")]
#[test_case(waits; "a wakeup that waits")]
fn a_wakeup_covers_no_resolved_thread(notice: fn(u64) -> WakeupNotice) {
    let mut book = waiting();
    let id = book.threads()[0].id.clone();
    book.set_resolution(&id, Resolution::Resolved).unwrap();
    let thread = &book.threads()[0];

    assert!(!notice(thread.last_comment().unwrap().sequence()).covers(thread));
}

#[test]
fn a_notice_reads_as_a_wait_or_a_failure() {
    assert_eq!(
        waits(1).text(),
        "Waiting to reach the agent: the agent's input line shows text someone typed"
    );
    assert_eq!(failed(1).text(), "Not delivered: Focus an agent");
}
