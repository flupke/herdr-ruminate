use std::process::Command;

use test_case::test_case;

#[test_case("wide"; "a lone pane opens the review beside it")]
#[test_case("beside_a_neighbour"; "a pane with a neighbour opens the review below it")]
fn newly_opened_review_pane_fits_its_split_without_interactive_resize(scenario: &str) {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/pane_geometry.py"
        ))
        .arg(directory.path())
        .arg(env!("CARGO_BIN_EXE_reviewer-control"))
        .arg(scenario)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
