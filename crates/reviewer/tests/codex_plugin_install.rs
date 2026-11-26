use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Machine {
    directory: tempfile::TempDir,
}

impl Machine {
    fn new() -> Self {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        fs::create_dir(directory.path().join("bin")).unwrap();
        fs::create_dir(directory.path().join("path")).unwrap();
        fs::hard_link(
            env!("CARGO_BIN_EXE_reviewer-control"),
            directory.path().join("bin/reviewer-control"),
        )
        .unwrap();
        Self { directory }
    }

    fn path(&self, path: &str) -> PathBuf {
        self.directory.path().join(path)
    }

    fn with_failing_codex(self) -> Self {
        let codex = self.path("path/codex");
        fs::write(
            &codex,
            "#!/bin/sh\necho \"codex: refused $*\" >&2\nexit 3\n",
        )
        .unwrap();
        fs::set_permissions(&codex, fs::Permissions::from_mode(0o755)).unwrap();
        self
    }

    fn plugin(&self, action: &str) -> Output {
        Command::new(self.path("bin/reviewer-control"))
            .args(["codex-plugin", action])
            .env("PATH", self.path("path"))
            .env("CODEX_HOME", self.path("codex-home"))
            .output()
            .unwrap()
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn exists(path: &Path) -> bool {
    path.try_exists().unwrap()
}

#[test]
fn without_codex_on_the_path_the_plugin_is_skipped_with_a_note() {
    let machine = Machine::new();

    let output = machine.plugin("install");

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("no `codex` on PATH, skipped the plugin"),
        "{}",
        stderr(&output)
    );
    assert!(!exists(&machine.path("bin/codex-plugin")));
    assert!(!exists(&machine.path("codex-home")));

    fs::create_dir_all(machine.path("bin/codex-plugin/plugin")).unwrap();
    let output = machine.plugin("uninstall");

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!exists(&machine.path("bin/codex-plugin")));
}

#[test]
fn a_codex_that_fails_fails_the_installation() {
    let machine = Machine::new().with_failing_codex();

    let output = machine.plugin("install");

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("codex: refused plugin marketplace remove"),
        "{}",
        stderr(&output)
    );
    assert!(!stderr(&output).contains("skipped"), "{}", stderr(&output));

    let output = machine.plugin("uninstall");

    assert!(!output.status.success());
}
