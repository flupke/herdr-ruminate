use super::*;

fn variables(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).into(), (*value).into()))
        .collect()
}

#[test]
fn the_agent_adds_its_variables_and_its_values_win() {
    let own = variables(&[
        ("HC_TERMINAL_ID", "t9"),
        ("PATH", "/core/bin"),
        ("TERM", "xterm"),
    ]);
    let agent = variables(&[("PATH", "/user/bin"), ("TYPESAFE_API_KEY", "k")]);

    let adopted = adopted(own, agent).unwrap();

    let expected: BTreeMap<_, _> = variables(&[
        ("HC_TERMINAL_ID", "t9"),
        ("PATH", "/user/bin"),
        ("TERM", "xterm"),
        ("TYPESAFE_API_KEY", "k"),
    ])
    .into_iter()
    .collect();
    assert_eq!(adopted, expected);
}

#[test]
fn the_pane_keeps_its_own_folder() {
    let own = variables(&[("PWD", "/repo"), ("TERM", "xterm")]);
    let agent = variables(&[
        ("PWD", "/elsewhere"),
        ("OLDPWD", "/before"),
        ("_", "/bin/claude"),
    ]);

    assert_eq!(adopted(own, agent), None);
}

#[test]
fn nothing_changes_once_the_pane_has_the_agent_s_variables() {
    let agent = variables(&[("EDITOR", "vim"), ("PATH", "/user/bin")]);
    let first = adopted(variables(&[("PATH", "/core/bin")]), agent.clone()).unwrap();

    assert_eq!(adopted(first, agent), None);
}

#[test]
fn of_a_variable_the_agent_has_twice_the_first_wins_and_runs_once() {
    let agent = variables(&[("EDITOR", "vi"), ("EDITOR", "vim")]);
    let first = adopted(Vec::new(), agent.clone()).unwrap();

    assert_eq!(first.get(&OsString::from("EDITOR")), Some(&"vi".into()));
    assert_eq!(adopted(first, agent), None);
}
