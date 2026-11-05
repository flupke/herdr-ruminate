use super::*;

fn config(source: &str) -> eyre::Result<ReviewerConfig> {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("config.toml"), source).unwrap();
    ReviewerConfig::from_dir(directory.path())
}

#[test]
fn reads_a_string_setting() {
    let config = config("theme = \"nord\"\n").unwrap();

    assert_eq!(config.string("theme").unwrap(), Some("nord"));
    assert_eq!(config.string("other").unwrap(), None);
}

#[test]
fn a_missing_file_has_no_setting() {
    let directory = tempfile::tempdir().unwrap();

    let config = ReviewerConfig::from_dir(directory.path()).unwrap();

    assert_eq!(config.string("theme").unwrap(), None);
}

#[test]
fn a_setting_of_another_type_names_the_setting_but_not_its_value() {
    let config = config("typesafe_api_key = 123456789\n").unwrap();

    let error = config.string("typesafe_api_key").unwrap_err().to_string();

    assert!(error.contains("`typesafe_api_key`"), "{error}");
    assert!(!error.contains("123456789"), "{error}");
}

#[test]
fn a_file_that_is_not_toml_names_the_line_but_not_its_text() {
    let error = config("theme = \"nord\"\ntypesafe_api_key = sk-secret-value\n")
        .err()
        .unwrap()
        .to_string();

    assert!(error.contains("at line 2"), "{error}");
    assert!(!error.contains("sk-secret-value"), "{error}");
}
