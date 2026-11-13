use review_repository::repository::{Repository, Snapshot};
use review_store::ReviewStore;
use review_test_support::{JjFixture, JjLayout, complete_repository_snapshot};
use review_types::MarkAuthor;

use crate::{ReviewStatus, ReviewTracker};

/// A jj repository with a review tracker over a private state, and the base every change of
/// a test starts from: `a.txt` and `b.txt`, each three lines.
struct History {
    jj: JjFixture,
    state: tempfile::TempDir,
    repository: Repository,
    tracker: ReviewTracker,
}

impl History {
    fn new() -> Self {
        let jj = JjFixture::new(JjLayout::NonColocated);
        jj.write("a.txt", b"a1\na2\na3\n");
        jj.write("b.txt", b"b1\nb2\nb3\n");
        jj.jj(["describe", "-m", "base"]);
        let state = tempfile::tempdir().unwrap();
        let repository = Repository::discover(jj.root())
            .unwrap()
            .with_state_root(state.path());
        let tracker = ReviewTracker::new(
            repository.clone(),
            ReviewStore::open(state.path(), jj.root()).unwrap(),
        );
        Self {
            jj,
            state,
            repository,
            tracker,
        }
    }

    /// A new change on `parent` that writes `files`, as the working copy; its change ID.
    fn change(&self, parent: &str, description: &str, files: &[(&str, &[u8])]) -> String {
        self.jj.jj(["new", parent, "-m", description]);
        for (path, content) in files {
            self.jj.write(path, content);
        }
        self.jj.change_id()
    }

    /// The working copy, as the reviewer reads it.
    fn snapshot(&self) -> Snapshot {
        complete_repository_snapshot(&self.repository)
    }

    /// Mark every changed file of the working copy reviewed, as the reviewer does.
    fn mark_all(&self) {
        let snapshot = self.snapshot();
        for file in &snapshot.files {
            self.tracker
                .mark(&snapshot, file, &MarkAuthor::Reviewer)
                .unwrap();
        }
    }

    /// The review status of each changed file of the working copy, by path.
    fn statuses(&self) -> Vec<(String, ReviewStatus)> {
        self.statuses_of(&self.tracker)
    }

    /// [`Self::statuses`], as a reviewer started again reads them: with a tracker of its own
    /// over the same state, which searched no commit yet.
    fn statuses_after_restart(&self) -> Vec<(String, ReviewStatus)> {
        let tracker = ReviewTracker::new(
            self.repository.clone(),
            ReviewStore::open(self.state.path(), self.jj.root()).unwrap(),
        );
        self.statuses_of(&tracker)
    }

    fn statuses_of(&self, tracker: &ReviewTracker) -> Vec<(String, ReviewStatus)> {
        let snapshot = self.snapshot();
        let states = tracker.statuses(&snapshot).unwrap();
        snapshot
            .files
            .iter()
            .zip(states)
            .map(|(file, state)| (file.review_path().display(), state.status))
            .collect()
    }

    /// Duplicate `change` onto `destination`; the change ID of the copy.
    fn duplicate(&self, change: &str, destination: &str) -> String {
        let before = self.visible_changes();
        self.jj
            .jj(["duplicate", change, "--destination", destination]);
        let after = self.visible_changes();
        let mut added = after.into_iter().filter(|id| !before.contains(id));
        let duplicate = added.next().expect("jj duplicate adds a change");
        assert_eq!(added.next(), None, "jj duplicate adds one change");
        duplicate
    }

    fn visible_changes(&self) -> Vec<String> {
        let output = self.jj.jj([
            "log",
            "--no-graph",
            "-r",
            "all()",
            "-T",
            r#"change_id ++ "\n""#,
        ]);
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

#[test]
fn a_duplicate_takes_the_marks_of_the_change_it_copies() {
    let history = History::new();
    let base = history.jj.change_id();
    let destination = history.change(&base, "destination", &[("c.txt", b"c\n")]);
    let reviewed = history.change(
        &base,
        "reviewed",
        &[
            ("a.txt", b"a1\nsnapshot_cost\na3\n"),
            ("b.txt", b"b1\nb2 reviewed\nb3\n"),
        ],
    );
    history.mark_all();

    let duplicate = history.duplicate(&reviewed, &destination);
    history.jj.jj(["edit", &duplicate]);
    history.jj.write("a.txt", b"a1\nload_cost\na3\n");
    history.jj.jj(["abandon", &reviewed]);

    assert_eq!(
        history.statuses(),
        [
            ("a.txt".to_owned(), ReviewStatus::ChangedSinceReview),
            ("b.txt".to_owned(), ReviewStatus::Reviewed),
        ]
    );
}

#[test]
fn a_change_with_marks_of_its_own_takes_none_of_the_change_it_copies() {
    let history = History::new();
    let base = history.jj.change_id();
    let reviewed = history.change(
        &base,
        "reviewed",
        &[
            ("a.txt", b"a1\na2 reviewed\na3\n"),
            ("b.txt", b"b1\nb2 reviewed\nb3\n"),
        ],
    );
    history.mark_all();
    let duplicate = history.duplicate(&reviewed, &base);
    history.jj.jj(["edit", &duplicate]);
    let snapshot = history.snapshot();
    history
        .tracker
        .mark(&snapshot, &snapshot.files[1], &MarkAuthor::Reviewer)
        .unwrap();

    assert_eq!(
        history.statuses(),
        [
            ("a.txt".to_owned(), ReviewStatus::Unreviewed),
            ("b.txt".to_owned(), ReviewStatus::Reviewed),
        ]
    );
}

#[test]
fn marks_taken_once_and_reopened_stay_reopened() {
    let history = History::new();
    let base = history.jj.change_id();
    let reviewed = history.change(&base, "reviewed", &[("a.txt", b"a1\na2 reviewed\na3\n")]);
    history.mark_all();
    let duplicate = history.duplicate(&reviewed, &base);
    history.jj.jj(["edit", &duplicate]);
    assert_eq!(
        history.statuses(),
        [("a.txt".to_owned(), ReviewStatus::Reviewed)]
    );

    let snapshot = history.snapshot();
    history
        .tracker
        .unreview(&snapshot, &snapshot.files[0])
        .unwrap();

    assert_eq!(
        history.statuses_after_restart(),
        [("a.txt".to_owned(), ReviewStatus::Unreviewed)]
    );
}

#[test]
fn a_file_two_squashed_changes_marked_takes_neither_mark() {
    let history = History::new();
    let base = history.jj.change_id();
    let first = history.change(
        &base,
        "first",
        &[
            ("a.txt", b"a1\na2 first\na3\n"),
            ("b.txt", b"b1\nb2 first\nb3\n"),
        ],
    );
    history.mark_all();
    let second = history.change(&first, "second", &[("a.txt", b"a1\na2 second\na3\n")]);
    history.mark_all();
    let into = history.change(&base, "into", &[]);

    history.jj.jj([
        "squash", "-u", "--from", &first, "--from", &second, "--into", &into,
    ]);
    history.jj.jj(["edit", &into]);

    assert_eq!(
        history.statuses(),
        [
            ("a.txt".to_owned(), ReviewStatus::Unreviewed),
            ("b.txt".to_owned(), ReviewStatus::Reviewed),
        ]
    );
}

#[test]
fn a_duplicate_of_a_duplicate_takes_the_nearest_marks() {
    let history = History::new();
    let base = history.jj.change_id();
    let original = history.change(&base, "original", &[("a.txt", b"a1\na2 original\na3\n")]);
    history.mark_all();
    let copy = history.duplicate(&original, &base);
    history.jj.jj(["edit", &copy]);
    history.jj.write("a.txt", b"a1\na2 copy\na3\n");
    history.mark_all();

    let second_copy = history.duplicate(&copy, &base);
    history.jj.jj(["edit", &second_copy]);
    history.jj.jj(["abandon", &original, &copy]);

    assert_eq!(
        history.statuses(),
        [("a.txt".to_owned(), ReviewStatus::Reviewed)]
    );
}

#[test]
fn a_copy_read_before_its_origin_was_reviewed_takes_the_marks_made_since() {
    let history = History::new();
    let base = history.jj.change_id();
    let original = history.change(&base, "original", &[("a.txt", b"a1\na2 original\na3\n")]);
    let copy = history.duplicate(&original, &base);
    history.jj.jj(["edit", &copy]);
    assert_eq!(
        history.statuses(),
        [("a.txt".to_owned(), ReviewStatus::Unreviewed)]
    );

    history.jj.jj(["edit", &original]);
    history.mark_all();
    history.jj.jj(["edit", &copy]);

    assert_eq!(
        history.statuses(),
        [("a.txt".to_owned(), ReviewStatus::Reviewed)]
    );
}

#[test]
fn marks_reopened_on_a_copy_stay_reopened_on_its_copy() {
    let history = History::new();
    let base = history.jj.change_id();
    let original = history.change(&base, "original", &[("a.txt", b"a1\na2 original\na3\n")]);
    history.mark_all();
    let copy = history.duplicate(&original, &base);
    history.jj.jj(["edit", &copy]);
    history.statuses();
    let snapshot = history.snapshot();
    history
        .tracker
        .unreview(&snapshot, &snapshot.files[0])
        .unwrap();

    let second_copy = history.duplicate(&copy, &base);
    history.jj.jj(["edit", &second_copy]);
    history.jj.jj(["abandon", &original, &copy]);

    assert_eq!(
        history.statuses_after_restart(),
        [("a.txt".to_owned(), ReviewStatus::Unreviewed)]
    );
}

#[test]
fn a_copy_takes_marks_past_a_copy_never_read() {
    let history = History::new();
    let base = history.jj.change_id();
    let original = history.change(&base, "original", &[("a.txt", b"a1\na2 original\na3\n")]);
    history.mark_all();
    let copy = history.duplicate(&original, &base);
    let second_copy = history.duplicate(&copy, &base);
    history.jj.jj(["edit", &second_copy]);

    assert_eq!(
        history.statuses(),
        [("a.txt".to_owned(), ReviewStatus::Reviewed)]
    );
}

#[test]
fn each_snapshot_of_a_copy_keeps_the_lines_of_the_ones_before() {
    let history = History::new();
    let base = history.jj.change_id();
    let original = history.change(
        &base,
        "original",
        &[
            ("a.txt", b"a1\na2 original\na3\n"),
            ("b.txt", b"b1\nb2 original\nb3\n"),
        ],
    );
    let copy = history.duplicate(&original, &base);
    history.jj.jj(["edit", &copy]);
    history.statuses();
    history.jj.write("b.txt", b"b1\nb2 copy\nb3\n");
    history.statuses();
    history.jj.write("b.txt", b"b1\nb2 copy again\nb3\n");
    history.statuses();

    history.jj.jj(["edit", &original]);
    history.mark_all();
    history.jj.jj(["edit", &copy]);

    assert_eq!(
        history.statuses(),
        [
            ("a.txt".to_owned(), ReviewStatus::Reviewed),
            ("b.txt".to_owned(), ReviewStatus::ChangedSinceReview),
        ]
    );
}

#[test]
fn a_copy_would_take_marks_once_its_origin_has_some_and_until_it_took_them() {
    let history = History::new();
    let base = history.jj.change_id();
    let original = history.change(&base, "original", &[("a.txt", b"a1\na2 original\na3\n")]);
    let copy = history.duplicate(&original, &base);
    history.jj.jj(["edit", &copy]);
    history.statuses();
    let would_take_marks = || {
        let snapshot = history.snapshot();
        history.tracker.would_take_marks(
            snapshot.identity.review_unit(),
            snapshot.identity.snapshot_id(),
        )
    };
    assert!(!would_take_marks());

    history.jj.jj(["edit", &original]);
    history.mark_all();
    history.jj.jj(["edit", &copy]);
    assert!(would_take_marks());

    history.statuses();
    assert!(!would_take_marks());
}

#[test]
fn origins_that_disagree_are_searched_again_once_their_marks_change() {
    let history = History::new();
    let base = history.jj.change_id();
    let first = history.change(&base, "first", &[("a.txt", b"a1\na2 first\na3\n")]);
    history.mark_all();
    let second = history.change(&first, "second", &[("a.txt", b"a1\na2 second\na3\n")]);
    history.mark_all();
    let into = history.change(&base, "into", &[]);
    history.jj.jj([
        "squash", "-u", "--from", &first, "--from", &second, "--into", &into,
    ]);
    history.jj.jj(["edit", &into]);
    let snapshot = history.snapshot();
    let would_take_marks = || {
        history.tracker.would_take_marks(
            snapshot.identity.review_unit(),
            snapshot.identity.snapshot_id(),
        )
    };
    assert_eq!(
        history.statuses(),
        [("a.txt".to_owned(), ReviewStatus::Unreviewed)]
    );
    assert!(!would_take_marks());

    ReviewStore::open(history.state.path(), history.jj.root())
        .unwrap()
        .unreview(&first.as_str().into(), b"a.txt")
        .unwrap();

    assert!(would_take_marks());
    // The mark of `second` comes over: it reviewed its lines on top of `first`'s, so they
    // differ from it on the base of `into`.
    assert_eq!(
        history.statuses(),
        [("a.txt".to_owned(), ReviewStatus::ChangedSinceReview)]
    );
}
