use std::path::Path;
use std::sync::{Arc, Mutex};

use herdr_bus::{MessageQuery, Stored};

use super::*;

const ID: &str = "0b7f3f3e-8f0e-4d55-9a57-5b8c2f0c9a11";

fn comment(id: &str, to: Option<&str>) -> UserWords {
    UserWords {
        id: id.into(),
        text: "Why a lock here?\nThe caller holds one already.".into(),
        review_unit: "change-1".into(),
        place: Place::Thread {
            thread: serde_json::from_value(serde_json::json!("thread-1")).unwrap(),
            round: None,
        },
        to: to.map(|pane| PaneId(pane.into())),
    }
}

/// Publishes `comments` on the bus file at `path`, and returns the failures reported, once the
/// publisher is done.
fn publish(path: &Path, comments: Vec<UserWords>) -> Vec<String> {
    let failures = Arc::new(Mutex::new(Vec::new()));
    let reported = failures.clone();
    let bus = WordsBus::start(path.to_owned(), move |error| {
        reported.lock().unwrap().push(error);
    });
    for comment in comments {
        bus.publish(comment);
    }
    bus.close();
    Arc::try_unwrap(failures).unwrap().into_inner().unwrap()
}

fn words_of(path: &Path) -> Vec<Stored> {
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
    in_a_round.place = Place::Thread {
        thread: serde_json::from_value(serde_json::json!("thread-1")).unwrap(),
        round: Some("round-7".into()),
    };

    assert_eq!(publish(&path, vec![in_a_round]), Vec::<String>::new());

    let stored = words_of(&path);
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
            "place": "thread",
            "thread": "thread-1",
            "round": "round-7",
        })
    );
}

/// The format herdr-bowtie reads for an Explore answer: the round, the question and the choice
/// beside the text, and a failure that names the answer.
#[test]
fn an_answer_goes_on_the_words_output_with_its_round_question_and_choice() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bus.db");
    let answer = |id: &str, choice: Option<&str>, drawing_board| UserWords {
        id: id.into(),
        text: "Keep it, @David checks the timeout".into(),
        review_unit: "change-1".into(),
        place: Place::Answer {
            round: "round-7".into(),
            question: Some("cache-eviction".into()),
            choice: choice.map(Into::into),
            drawing_board,
        },
        to: Some(PaneId("w1-3".into())),
    };

    let failures = publish(
        &path,
        vec![
            answer(ID, Some("keep"), false),
            answer("1b7f3f3e-8f0e-4d55-9a57-5b8c2f0c9a11", None, true),
        ],
    );

    assert_eq!(failures, Vec::<String>::new());
    let mut stored = words_of(&path);
    stored.reverse();
    let data = stored
        .iter()
        .map(|stored| stored.message.data.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        data,
        [
            serde_json::json!({
                "author": "user",
                "to": "pane:w1-3",
                "text": "Keep it, @David checks the timeout",
                "review": "change-1",
                "place": "answer",
                "round": "round-7",
                "question": "cache-eviction",
                "choice": "keep",
            }),
            serde_json::json!({
                "author": "user",
                "to": "pane:w1-3",
                "text": "Keep it, @David checks the timeout",
                "review": "change-1",
                "place": "answer",
                "round": "round-7",
                "question": "cache-eviction",
                "drawing_board": true,
            }),
        ]
    );
    assert_eq!(stored[0].message.id.to_string(), ID);

    let blocker = directory.path().join("not-a-folder");
    std::fs::write(&blocker, "").unwrap();
    let failures = publish(&blocker.join("bus.db"), vec![answer(ID, None, false)]);
    assert!(
        failures[1].starts_with("The answer is sent, but herdr-bus did not get it"),
        "{failures:?}"
    );
}

/// The format herdr-bowtie reads for the list of Implement and the name given at Start.
#[test]
fn a_list_to_implement_and_a_name_go_with_their_place_and_round() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bus.db");
    let words = |id: &str, text: &str, place| UserWords {
        id: id.into(),
        text: text.into(),
        review_unit: "change-1".into(),
        place,
        to: Some(PaneId("w1-3".into())),
    };

    let failures = publish(
        &path,
        vec![
            words(
                ID,
                "\n- Drop the lock, @David",
                Place::Implementation {
                    round: "round-7".into(),
                    conclusion: "request-3".into(),
                },
            ),
            words(
                "1b7f3f3e-8f0e-4d55-9a57-5b8c2f0c9a11",
                "Ada",
                Place::Start {
                    round: "round-7".into(),
                },
            ),
        ],
    );

    assert_eq!(failures, Vec::<String>::new());
    let mut stored = words_of(&path);
    stored.reverse();
    assert_eq!(
        stored[0].message.subject.as_deref(),
        Some("- Drop the lock, @David")
    );
    let data = stored
        .iter()
        .map(|stored| stored.message.data.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        data,
        [
            serde_json::json!({
                "author": "user",
                "to": "pane:w1-3",
                "text": "\n- Drop the lock, @David",
                "review": "change-1",
                "place": "implementation",
                "round": "round-7",
                "conclusion": "request-3",
            }),
            serde_json::json!({
                "author": "user",
                "to": "pane:w1-3",
                "text": "Ada",
                "review": "change-1",
                "place": "start",
                "round": "round-7",
            }),
        ]
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

    let data = &words_of(&path)[0].message.data;
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

    let stored = words_of(&path);
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
    assert!(failures[0].starts_with("What you write to agents will not reach herdr-bus"));
    assert!(
        failures[1..]
            .iter()
            .all(|failure| failure.starts_with("The comment is posted"))
    );
}
