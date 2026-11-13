use super::*;

fn config(source: &str) -> eyre::Result<Option<LinearSettings>> {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("config.toml"), source).unwrap();
    LinearSettings::from_config(&ReviewerConfig::from_dir(directory.path())?)
}

fn settings(teams: &[&str]) -> LinearSettings {
    LinearSettings {
        workspace: "acme".to_owned(),
        teams: teams.iter().map(|team| (*team).to_owned()).collect(),
    }
}

fn ids(issues: &[LinearIssue]) -> Vec<&str> {
    issues.iter().map(|issue| issue.id.as_str()).collect()
}

#[test]
fn an_issue_links_to_its_page_in_the_workspace() {
    let issues = settings(&["ENG"]).issues(["Open models sooner (ENG-2243)"]);

    assert_eq!(
        issues,
        [LinearIssue {
            id: "ENG-2243".to_owned(),
            url: "https://linear.app/acme/issue/ENG-2243".to_owned(),
        }]
    );
}

#[test]
fn finds_a_lower_case_identifier_in_a_workspace_name_and_writes_it_in_capitals() {
    let issues = settings(&["ENG"]).issues(["foundations.eng-2243-sync-outage-open"]);

    assert_eq!(ids(&issues), ["ENG-2243"]);
}

#[test]
fn finds_an_identifier_at_the_start_and_at_the_end_of_a_text() {
    let issues = settings(&["ENG"]).issues(["ENG-1 then ENG-2"]);

    assert_eq!(ids(&issues), ["ENG-1", "ENG-2"]);
}

#[test]
fn ignores_keys_of_teams_it_does_not_list() {
    let issues = settings(&["ENG"]).issues(["Read UTF-8 and SHA-256, see OPS-4"]);

    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn ignores_a_key_inside_a_word_or_a_number() {
    let issues =
        settings(&["ENG", "3D"]).issues(["REENG-12 ENG-12x ENG-x 13D-4 ENG- 3D-", "ENG-1a2"]);

    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn tells_apart_a_key_that_begins_another() {
    let issues = settings(&["RAY", "RAYENG"]).issues(["RAYENG-7 and RAY-8"]);

    assert_eq!(ids(&issues), ["RAYENG-7", "RAY-8"]);
}

#[test]
fn lists_each_issue_once_in_the_order_the_texts_first_name_it() {
    let issues = settings(&["ENG", "OPS"]).issues([
        "Fix the sync (OPS-3)\n\nAlso ENG-9, after OPS-3.",
        "eng-9-fix-sync",
        "luper/eng-10",
    ]);

    assert_eq!(ids(&issues), ["OPS-3", "ENG-9", "ENG-10"]);
}

#[test]
fn finds_an_identifier_beside_non_ascii_text() {
    let issues = settings(&["ENG"]).issues(["Réparé «ENG-5»"]);

    assert_eq!(ids(&issues), ["ENG-5"]);
}

#[test]
fn reads_the_workspace_and_the_team_keys_in_capitals() {
    let settings = config("[linear]\nworkspace = \"acme\"\nteams = [\"eng\", \"3D\"]\n")
        .unwrap()
        .unwrap();

    let issues = settings.issues(["ENG-1 3d-2"]);

    assert_eq!(ids(&issues), ["ENG-1", "3D-2"]);
    assert_eq!(issues[0].url, "https://linear.app/acme/issue/ENG-1");
}

#[test]
fn has_no_settings_without_the_table() {
    assert_eq!(config("theme = \"nord\"\n").unwrap(), None);
}

#[test]
fn refuses_a_table_without_a_workspace_or_with_one_unfit_for_an_address() {
    for source in [
        "[linear]\nteams = [\"ENG\"]\n",
        "[linear]\nworkspace = \"\"\nteams = [\"ENG\"]\n",
        "[linear]\nworkspace = \"acme/issue\"\nteams = [\"ENG\"]\n",
    ] {
        let error = config(source).unwrap_err().to_string();

        assert!(error.contains("`linear.workspace`"), "{source}: {error}");
    }
}

#[test]
fn refuses_a_table_without_team_keys_or_with_one_that_is_not_a_key() {
    for source in [
        "[linear]\nworkspace = \"acme\"\n",
        "[linear]\nworkspace = \"acme\"\nteams = []\n",
        "[linear]\nworkspace = \"acme\"\nteams = [\"ENG\", \"OP-S\"]\n",
        "[linear]\nworkspace = \"acme\"\nteams = \"ENG\"\n",
    ] {
        let error = config(source).unwrap_err().to_string();

        assert!(error.contains("`linear.teams`"), "{source}: {error}");
    }
}
