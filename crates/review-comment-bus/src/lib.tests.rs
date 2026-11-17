use std::path::Path;
use std::sync::{Arc, Mutex};

use herdr_bus::{MessageQuery, Stored};

use super::*;

const ID: &str = "0b7f3f3e-8f0e-4d55-9a57-5b8c2f0c9a11";

fn comment(id: &str, to: Option<&str>) -> PostedComment {
    PostedComment {
        id: serde_json::from_value(serde_json::json!(id)).unwrap(),
        text: "Why a lock here?\nThe caller holds one already.".into(),
        review_unit: "change-1".into(),
        thread: serde_json::from_value(serde_json::json!("thread-1")).unwrap(),
        round: None,
        to: to.map(|pane| PaneId(pane.into())),
    }
}

/// Publishes `comments` on the bus file at `path`, and returns the failures reported, once the
/// publisher is done.
fn publish(path: &Path, comments: Vec<PostedComment>) -> Vec<String> {
    let failures = Arc::new(Mutex::new(Vec::new()));
    let reported = failures.clone();
    let bus = CommentBus::start(path.to_owned(), move |error| {
        reported.lock().unwrap().push(error);
    });
    for comment in comments {
        bus.publish(comment);
    }
    bus.close();
    Arc::try_unwrap(failures).unwrap().into_inner().unwrap()
}

fn words(path: &Path) -> Vec<Stored> {
    Bus::open(path)
        .unwrap()
        .messages(&MessageQuery {
            output: Some(OUTPUT.into()),
            ..MessageQuery::default()
        })
        .unwrap()
}

/// The format herdr-bowtie reads: a rename here breaks its routing of the mentions.
#[test]
fn a_comment_goes_on_the_words_output_as_herdr_words_with_its_pane_and_place() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bus.db");
    let mut in_a_round = comment(ID, Some("w1-3"));
    in_a_round.round = Some("round-7".into());

    assert_eq!(publish(&path, vec![in_a_round]), Vec::<String>::new());

    let stored = words(&path);
    assert_eq!(stored.len(), 1);
    let message = &stored[0].message;
    assert_eq!(message.kind, "herdr.words");
    assert_eq!(message.id.to_string(), ID);
    assert_eq!(message.subject.as_deref(), Some("Why a lock here?"));
    assert_eq!(
        message.data,
        serde_json::json!({
            "author": "user",
            "to": "pane:w1-3",
            "text": "Why a lock here?\nThe caller holds one already.",
            "review": "change-1",
            "thread": "thread-1",
            "round": "round-7",
        })
    );
}

#[test]
fn a_comment_that_wakes_no_agent_goes_to_no_one() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bus.db");

    assert_eq!(
        publish(&path, vec![comment(ID, None)]),
        Vec::<String>::new()
    );

    let data = &words(&path)[0].message.data;
    assert_eq!(data["to"], serde_json::Value::Null);
    assert_eq!(data.get("round"), None);
}

/// The id stays the same across versions of the reviewer, so that a retry by a newer one still
/// publishes the comment once: the expected id is Python's `uuid5` of the namespace and the ID.
#[test]
fn a_message_id_that_is_no_uuid_goes_as_a_stable_uuid_v5() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bus.db");

    publish(
        &path,
        vec![
            comment("legacy-message-1", None),
            comment("legacy-message-1", None),
        ],
    );

    let stored = words(&path);
    assert_eq!(stored.len(), 1);
    assert_eq!(
        stored[0].message.id.to_string(),
        "8ee33edc-9bbd-5c9c-b222-3fcaaa073f1d"
    );
}

#[test]
fn a_bus_that_cannot_be_written_reports_each_comment_it_missed() {
    let directory = tempfile::tempdir().unwrap();
    // The bus file's folder is a plain file: neither the open nor any retry can create it.
    let blocker = directory.path().join("not-a-folder");
    std::fs::write(&blocker, "").unwrap();

    let failures = publish(
        &blocker.join("bus.db"),
        vec![
            comment(ID, None),
            comment("1b7f3f3e-8f0e-4d55-9a57-5b8c2f0c9a11", None),
        ],
    );

    // The open at start, then one retry for each comment.
    assert_eq!(failures.len(), 3, "{failures:?}");
    assert!(
        failures
            .iter()
            .all(|failure| failure.contains("not-a-folder"))
    );
    assert!(failures[0].starts_with("Review comments will not reach herdr-bus"));
    assert!(
        failures[1..]
            .iter()
            .all(|failure| failure.starts_with("The comment is posted"))
    );
}
