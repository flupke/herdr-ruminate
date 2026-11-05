use super::*;
use crate::{Post, ReviewThreads};

fn failure(through: u64) -> WakeupFailure {
    WakeupFailure {
        through,
        error: "Focus an agent".into(),
    }
}

/// A review with one open thread whose comment waits for a reply.
fn waiting() -> ReviewThreads {
    let mut book = ReviewThreads::new("change".into());
    book.post(Post::to_round("round-1", "Why a lock?".into(), None, None))
        .unwrap();
    book
}

#[test]
fn a_failed_wakeup_covers_an_open_thread_whose_comment_it_reached_back_to() {
    let book = waiting();
    let thread = &book.threads()[0];
    let comment = thread.last_comment().unwrap().sequence();

    assert!(failure(comment).covers(thread));
    assert!(
        !failure(comment - 1).covers(thread),
        "a comment posted after it"
    );
}

#[test]
fn a_failed_wakeup_covers_no_resolved_thread() {
    let mut book = waiting();
    let id = book.threads()[0].id.clone();
    book.set_resolution(&id, Resolution::Resolved).unwrap();
    let thread = &book.threads()[0];

    assert!(!failure(thread.last_comment().unwrap().sequence()).covers(thread));
}
