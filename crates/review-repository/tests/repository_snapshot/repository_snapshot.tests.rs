use super::*;

#[test_case::test_case(RepoType::Git; "git")]
#[test_case::test_case(RepoType::Jj; "jj")]
fn reads_current_and_base_file_content(repository_type: RepoType) {
    let context = RepositoryTestContext::new(repository_type);
    context
        .repository_files
        .write("file.txt", b"base content\n");
    context.repository_files.new_change("edit file");
    context
        .repository_files
        .write("file.txt", b"current content\n");
    let snapshot = complete_repository_snapshot(&context.repository);
    let path = snapshot.files[0].review_path();

    assert_eq!(
        context
            .repository
            .file_at(snapshot.identity.snapshot_id(), path)
            .unwrap()
            .as_deref(),
        Some(&b"current content\n"[..])
    );
    assert_eq!(
        context.repository.base_file_at(&snapshot, path).unwrap(),
        b"base content\n"
    );
}

#[test_case::test_case(RepoType::Git; "git")]
#[test_case::test_case(RepoType::Jj; "jj")]
fn reads_a_path_the_revision_lacks_as_no_file(repository_type: RepoType) {
    let context = RepositoryTestContext::new(repository_type);
    let files = &context.repository_files;
    // A wildcard or a leading colon in a path would read as a pattern or pathspec magic to
    // `git show` and `git ls-tree`; `colon` is what `:colon` would match.
    for path in ["deleted.txt", "replaced", "star*.txt", ":colon", "colon"] {
        files.write(path, b"base content\n");
    }
    files.new_change("delete files");
    for path in ["deleted.txt", "replaced", "star*.txt", ":colon"] {
        files.remove(path);
    }
    files.write("replaced/inner.txt", b"a directory now\n");
    let snapshot = complete_repository_snapshot(&context.repository);
    let revision = snapshot.identity.snapshot_id();

    for path in ["deleted.txt", "replaced", "star*.txt", ":colon"] {
        assert_eq!(
            context
                .repository
                .file_at(revision, &RepoPath::from_bytes(path.as_bytes()))
                .unwrap(),
            None,
            "{path}"
        );
    }
    assert!(
        context
            .repository
            .file_at(
                "0123456789abcdef0123456789abcdef01234567",
                &RepoPath::from_bytes(b"colon")
            )
            .is_err()
    );
}

#[test_case::test_case(RepoType::Git; "git")]
#[test_case::test_case(RepoType::Jj; "jj")]
fn renamed_and_edited_files_diff_both_paths(repository_type: RepoType) {
    let context = RepositoryTestContext::new(repository_type);
    context
        .repository_files
        .write("old.txt", b"one\nbefore\nthree\n");
    context.repository_files.new_change("rename and edit");
    context.repository_files.rename("old.txt", "new.txt");
    context
        .repository_files
        .write("new.txt", b"one\nafter\nthree\n");
    let snapshot = complete_repository_snapshot(&context.repository);
    let file = &snapshot.files[0];
    let diff = String::from_utf8(context.repository.diff(&snapshot, file).unwrap()).unwrap();

    assert!(diff.contains("-before"), "{diff}");
    assert!(diff.contains("+after"), "{diff}");
}
