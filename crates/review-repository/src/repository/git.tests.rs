use crate::repository::{ChangeKind, ChangedFile, FileKind};

#[test]
fn git_type_change_status_is_preserved() {
    let [file] = ChangedFile::parse_git(b":100644 100644 old new T\0file\0")
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(file.change, ChangeKind::TypeChanged);
}

#[test]
fn git_mode_change_overrides_modified_status() {
    let [file] = ChangedFile::parse_git(b":100644 120000 old new M\0file\0")
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(file.change, ChangeKind::TypeChanged);
}

#[test]
fn git_submodule_mode_is_parsed_as_a_gitlink() {
    let [file] = ChangedFile::parse_git(b":000000 160000 old new A\0module\0")
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(file.old_kind, FileKind::Absent);
    assert_eq!(file.new_kind, FileKind::Gitlink);
    assert_eq!(file.change, ChangeKind::Added);
}

#[test]
fn a_cancelled_git_snapshot_clears_a_stale_index_lock() {
    use crate::repository::RepositoryBackend;
    use review_test_support::{GitFixture, ReviewRepositoryFixture};

    let files = GitFixture::new();
    files.write("file.txt", b"changed\n");
    let state = tempfile::tempdir().unwrap();
    let repository = super::Repository::discover(files.root())
        .unwrap()
        .with_state_root(state.path());
    let independent = repository.without_cancellation();
    assert!(repository.current_identity().is_ok());

    let cache =
        super::GitBackend::discover(repository.root(), &super::Cancellation::default()).unwrap();
    cache.set_state_root(repository.root(), state.path());
    let cache = cache.cache().unwrap();
    let lock = cache.lock_path();
    std::fs::write(&lock, b"").unwrap();

    repository.cancel();
    let cancelled = super::GitBackend::run_with_cache(&repository, &cache, ["write-tree"]);
    assert!(matches!(
        cancelled,
        Err(crate::Error::CommandCancelled { .. })
    ));
    assert!(!lock.exists());
    assert!(independent.current_identity().is_ok());
}
