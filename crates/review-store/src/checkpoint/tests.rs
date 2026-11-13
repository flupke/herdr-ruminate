use super::*;

#[test]
fn clearing_all_marks_is_scoped_to_file_records_of_one_review() {
    let state = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = ReviewStore::open(state.path(), repository.path()).unwrap();
    let unit = "current".into();
    for path in [b"present.rs".as_slice(), b"no-longer-present.rs"] {
        store
            .mark(&unit, path, "aabb", &MarkAuthor::Reviewer)
            .unwrap();
    }
    let other = store
        .mark(
            &"another".into(),
            b"present.rs",
            "ccdd",
            &MarkAuthor::Reviewer,
        )
        .unwrap();
    let discussion = store.repository_dir.join("changes/current/discussion.json");
    fs::write(&discussion, "retained discussion").unwrap();
    store.save_file_pane_width(42).unwrap();

    store.unreview_all(&unit).unwrap();
    store.unreview_all(&unit).unwrap();

    for path in [b"present.rs".as_slice(), b"no-longer-present.rs"] {
        assert_eq!(store.load(&unit, path).unwrap(), LoadResult::Unreviewed);
    }
    assert_eq!(
        store.load(&"another".into(), b"present.rs").unwrap(),
        LoadResult::Reviewed(other)
    );
    assert_eq!(
        fs::read_to_string(discussion).unwrap(),
        "retained discussion"
    );
    assert_eq!(store.file_pane_width().unwrap(), Some(42));
    assert!(store.unreview_all(&"../another".into()).is_err());
    store
        .mark(&unit, b"present.rs", "eeff", &MarkAuthor::Reviewer)
        .unwrap();
    assert!(matches!(
        store.load(&unit, b"present.rs").unwrap(),
        LoadResult::Reviewed(_)
    ));
}

#[test]
fn an_expired_mark_is_removed_but_a_mark_written_since_stays() {
    let state = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = ReviewStore::open(state.path(), repository.path()).unwrap();
    let unit = "current".into();
    let expired = store
        .mark(
            &unit,
            b"a.rs",
            &"a".repeat(40),
            &review_types::MarkAuthor::Reviewer,
        )
        .unwrap();

    store
        .unreview_expired(&unit, b"a.rs", "2000-01-01T00:00:00Z")
        .unwrap();
    assert!(matches!(
        store.load(&unit, b"a.rs").unwrap(),
        LoadResult::Reviewed(_)
    ));
    store
        .unreview_expired(&unit, b"a.rs", &expired.reviewed_at)
        .unwrap();
    assert_eq!(store.load(&unit, b"a.rs").unwrap(), LoadResult::Unreviewed);
}

#[test]
fn a_review_takes_the_valid_mark_of_each_path_that_one_origin_marked() {
    let state = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = ReviewStore::open(state.path(), repository.path()).unwrap();
    let (first, second, copy) = ("first".into(), "second".into(), "copy".into());
    let only_first = store
        .mark(&first, b"first.rs", "aa", &MarkAuthor::Reviewer)
        .unwrap();
    store
        .mark(&first, b"both.rs", "aa", &MarkAuthor::Reviewer)
        .unwrap();
    store
        .mark(&second, b"both.rs", "bb", &MarkAuthor::Reviewer)
        .unwrap();
    let only_second = store
        .mark(&second, b"broken-in-first.rs", "bb", &MarkAuthor::Jev)
        .unwrap();
    // A partial mark that lost its versions: `load` reads it as unreviewed.
    fs::write(
        store.record_path(&first, b"broken-in-first.rs"),
        r#"{"schema_version":2,"review_unit":"first","path_encoding":"utf8","path":"broken-in-first.rs","baseline_commit_id":"aa","reviewed_at":"2026-01-01T00:00:00Z"}"#,
    )
    .unwrap();

    let lock = store.lock_marks(&copy).unwrap();
    assert!(store.take_marks(&lock, &copy, &[first, second]).unwrap());

    assert_eq!(
        store.load(&copy, b"first.rs").unwrap(),
        LoadResult::Reviewed(only_first)
    );
    assert_eq!(
        store.load(&copy, b"broken-in-first.rs").unwrap(),
        LoadResult::Reviewed(only_second)
    );
    assert_eq!(
        store.load(&copy, b"both.rs").unwrap(),
        LoadResult::Unreviewed
    );
    assert!(store.marks_stamp(&copy).unwrap().is_some());
}

#[test]
fn a_review_with_a_mark_written_before_stamps_takes_none() {
    let state = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = ReviewStore::open(state.path(), repository.path()).unwrap();
    let (origin, copy) = ("origin".into(), "copy".into());
    store
        .mark(&origin, b"a.rs", "aa", &MarkAuthor::Reviewer)
        .unwrap();
    let own = store
        .mark(&copy, b"b.rs", "bb", &MarkAuthor::Reviewer)
        .unwrap();
    fs::remove_file(store.stamp_path(&copy)).unwrap();

    let lock = store.lock_marks(&copy).unwrap();
    assert!(
        !store
            .take_marks(&lock, &copy, std::slice::from_ref(&origin))
            .unwrap()
    );

    assert_eq!(store.load(&copy, b"a.rs").unwrap(), LoadResult::Unreviewed);
    assert_eq!(
        store.load(&copy, b"b.rs").unwrap(),
        LoadResult::Reviewed(own)
    );
}
