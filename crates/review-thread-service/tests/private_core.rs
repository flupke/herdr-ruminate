//! The thread service's delivery through the hc backend, against a real herdr-core of its own
//! ([`hc_test_support::PrivateCore`]), never the user's core. Opt-in, since each test starts a
//! core of its own: `cargo nextest run -p review-thread-service --run-ignored only`, with the dev
//! shell's pinned `hc` (`TEST_HC_BIN_PATH`), or else `hc` on the `PATH`.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use hc_test_support::{PrivateCore, line_agent, wait_until_read, written};
use herdr_client::client::EventCanceller;
use herdr_client::protocol::{AgentHost, AgentPort, AgentStatus, AgentTarget, PaneId};
use review_source::{AnchorKind, DiffRangeAnchor};
use review_thread_service::{Command, Event, PinnedAgent, Worker};
use review_threads::{Post, ThreadCommand};

/// Waits until the agent of `pane` reports itself idle.
fn wait_until_idle(client: &hc_client::HcClient, pane: &PaneId) {
    client
        .forward_agent_status(pane, &EventCanceller::default(), &mut |reported| {
            reported != AgentStatus::Idle
        })
        .unwrap();
}

/// The statuses the agent of `pane` reports from now on, its current one first: returns once
/// herdr-core holds the subscription.
fn statuses(client: &hc_client::HcClient, pane: &PaneId) -> mpsc::Receiver<AgentStatus> {
    let (client, pane) = (client.clone(), pane.clone());
    let (reported, statuses) = mpsc::channel();
    thread::spawn(move || {
        let _ = client.forward_agent_status(&pane, &EventCanceller::default(), &mut |status| {
            reported.send(status).is_ok()
        });
    });
    statuses
        .recv()
        .expect("herdr-core reported no status of the agent");
    statuses
}

/// The lines the agent in `folder` submitted that hold `text`.
fn lines_with(folder: &std::path::Path, text: &str) -> Vec<String> {
    written(folder, "lines")
        .lines()
        .filter(|line| line.contains(text))
        .map(str::to_owned)
        .collect()
}

/// The lines of a comment on the change under review.
fn anchor() -> DiffRangeAnchor {
    DiffRangeAnchor {
        source_checkpoint: "initial".into(),
        old_path: None,
        new_path: Some("lib.rs".into()),
        old_lines: None,
        new_lines: Some(0..1),
        target_kind: AnchorKind::Lines,
        source_hunk_count: 1,
        old_content: None,
        new_content: Some(b"original\n".to_vec()),
        diff_hash: "hash".into(),
    }
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn text_left_in_one_agents_line_holds_its_prompt_but_not_a_wakeup_to_another_agent() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let (folder_a, folder_b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let client = core.client();
    let a = core.run(folder_a.path(), &line_agent(""));
    let b = core.run(folder_b.path(), &line_agent(""));
    wait_until_idle(&client, &a);
    wait_until_idle(&client, &b);
    // herdr-core types in a terminal that just started only once its output settled.
    thread::sleep(Duration::from_secs(3));
    let mut person = core.person_at(&a);
    person.types("my own", Duration::from_millis(50));
    wait_until_read(folder_a.path(), 'n');
    let agent_a = client.get_agent(&a).unwrap().unwrap();
    let store_folder = tempfile::tempdir().unwrap();
    let store =
        review_store::ReviewStore::open(store_folder.path().join("state"), store_folder.path())
            .unwrap();
    let port = review_test_support::TestPort::new();
    let endpoint =
        review_mcp::Endpoint::for_repository(store_folder.path(), Some(port.number())).unwrap();
    // The review pane's agent is B, as when the user focused it last.
    let worker = Worker::start(
        store,
        client.clone(),
        None,
        AgentTarget::new(agent_a.tab_id.clone(), Some(b.clone())),
        Ok(endpoint),
        |_| Err("No Explore session in this test".into()),
        move |event: Event| match event {
            Event::Error(error) => eprintln!("error: {error}"),
            Event::Wakeup { notice, .. } => eprintln!("wakeup: {notice:?}"),
            _ => {}
        },
    );

    // A prompt to A, as an Explore turn's, which waits for the text to leave A's line.
    let (to_a, _to_a) = worker
        .prompt_sender()
        .send(PinnedAgent::new(agent_a), "Explore the change".into());
    let b_statuses = statuses(&client, &b);
    worker.send(Command::Thread(ThreadCommand::Load("review".into())));
    worker.send(Command::Thread(ThreadCommand::Post {
        review_unit: "review".into(),
        post: Post::start(anchor(), "+original".into(), "Rename this".into()),
    }));

    // B's agent writes the line it got, then goes working.
    while b_statuses.recv().unwrap() != AgentStatus::Working {}
    assert_eq!(lines_with(folder_b.path(), "get_new_messages").len(), 1);
    assert_eq!(written(folder_a.path(), "lines"), "");
    person.presses_enter();
    to_a.wait().unwrap();
    assert_eq!(
        written(folder_a.path(), "lines"),
        "my own\nExplore the change\n"
    );
    assert_eq!(
        lines_with(folder_b.path(), "get_new_messages").len(),
        1,
        "the wakeup reached B twice"
    );
}
