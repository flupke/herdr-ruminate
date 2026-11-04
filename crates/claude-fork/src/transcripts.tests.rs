use super::*;

fn project() -> (tempfile::TempDir, Transcripts, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("-work-repository");
    fs::create_dir_all(&directory).unwrap();
    let transcripts = Transcripts::at(root.path().to_owned());
    (root, transcripts, directory)
}

#[test]
fn the_last_entry_is_the_last_conversation_line_and_the_model_the_last_answer_s() {
    let (_root, transcripts, directory) = project();
    fs::write(
        directory.join("s1.jsonl"),
        concat!(
            r#"{"type":"user","uuid":"u1","message":{"role":"user"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a1","message":{"model":"claude-sonnet-5-5"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a2","message":{"model":"<synthetic>"}}"#,
            "\n",
            r#"{"type":"last-prompt","uuid":"x"}"#,
            "\n",
        ),
    )
    .unwrap();

    assert_eq!(
        transcripts.last_entry("s1"),
        Some(LastEntry {
            entry: Some("a2".into()),
            model: Some("claude-sonnet-5-5".into()),
            ends_turn: false,
        })
    );
    assert_eq!(transcripts.last_entry("s2"), None);
}

/// A turn of Claude Code 2.1.318 up to a tool call, whose result is the last entry.
const MID_TURN: &str = concat!(
    r#"{"type":"user","uuid":"u1","message":{"role":"user"}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a1","message":{"stop_reason":"tool_use"}}"#,
    "\n",
    r#"{"type":"user","uuid":"r1","message":{"role":"user"}}"#,
    "\n",
);

/// The end of that turn, which Claude Code writes after it reports the turn ended.
const TURN_END: &str = concat!(
    r#"{"type":"assistant","uuid":"a2","message":{"stop_reason":"tool_use"}}"#,
    "\n",
    r#"{"type":"user","uuid":"r2","message":{"role":"user"}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a3","message":{"stop_reason":"end_turn"}}"#,
    "\n",
    r#"{"type":"system","subtype":"turn_duration","uuid":"d1"}"#,
    "\n",
);

#[test]
fn a_fork_point_waits_for_the_end_of_the_turn_written_after_the_agent_went_idle() {
    let (_root, transcripts, directory) = project();
    let transcript = directory.join("s1.jsonl");
    fs::write(&transcript, MID_TURN).unwrap();

    // A rendezvous: a change is taken only once the waiter read the transcript and waits.
    let (changes, heard) = mpsc::sync_channel(0);
    let waiting = std::thread::spawn({
        let transcripts = transcripts.clone();
        let deadline = Instant::now() + Duration::from_secs(60);
        move || transcripts.turn_end_on("s1", deadline, &heard)
    });
    let change = || Ok(notify::Event::new(notify::EventKind::Any));
    changes
        .send(change())
        .expect("the waiter waits for a change, the turn not ended");
    let mut file = fs::File::options().append(true).open(&transcript).unwrap();
    std::io::Write::write_all(&mut file, TURN_END.as_bytes()).unwrap();
    // The waiter may have read the end already, after the first change.
    let _ = changes.send(change());
    let last = waiting.join().unwrap().unwrap();

    assert_eq!(last.entry.as_deref(), Some("a3"));
    assert!(last.ends_turn);
}

#[test]
fn a_transcript_that_shows_no_end_of_turn_by_the_deadline_gives_its_last_entry() {
    let (_root, transcripts, directory) = project();
    fs::write(directory.join("s1.jsonl"), MID_TURN).unwrap();

    let last = transcripts.turn_end("s1", Duration::ZERO).unwrap();

    assert_eq!(last.entry.as_deref(), Some("r1"));
    assert!(!last.ends_turn);
}

#[test]
fn deleting_a_session_removes_its_transcript_and_its_side_directory_only() {
    let (_root, transcripts, directory) = project();
    fs::write(directory.join("fork.jsonl"), "{}\n").unwrap();
    fs::create_dir_all(directory.join("fork/tool-results")).unwrap();
    fs::write(directory.join("parent.jsonl"), "{}\n").unwrap();

    transcripts.delete("fork");

    assert!(transcripts.find("fork").is_none());
    assert!(!directory.join("fork").exists());
    assert!(transcripts.find("parent").is_some());
}
