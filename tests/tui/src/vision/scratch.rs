//! The scratch repository a vision session reviews, and the files the session shares with its
//! reviewer: its private state, its stand-in agent, browser and Jev script, and the turns it
//! records. The stand-in agent and its forks keep their transcripts there too, never in the
//! user's Claude Code configuration; the reviewer's hooks listen in the app sandbox's runtime
//! directory.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, ensure};
use fake_agent::{Directory, variables};
use herdr_client::protocol::{PluginContext, WorkspaceId};
use review_explore_page_settings::ExplorePageSettings;
use review_repository::repository::RepoType;
use review_test_support::{
    ReviewRepositoryFixture, TestPort, repository_fixture, write_reviewer_settings,
};

use super::session::AGENT;

/// The loopback interface, which stands in for a network interface in a vision session.
#[cfg(target_os = "linux")]
const LOOPBACK: &str = "lo";
#[cfg(not(target_os = "linux"))]
const LOOPBACK: &str = "lo0";

/// The files of a scratch repository: the base change, then the change under review.
#[derive(Clone, Debug, Default)]
pub(crate) struct Seed {
    pub(crate) base: BTreeMap<String, String>,
    pub(crate) change: BTreeMap<String, String>,
}

impl Seed {
    /// The default change: a rename in one file, two new files, Unicode and a long line.
    pub(crate) fn sample() -> Self {
        let file = |path: &str, content: &str| (path.to_owned(), content.to_owned());
        Self {
            base: BTreeMap::from([file("sample.rs", "pub fn before_review() {}\n")]),
            change: BTreeMap::from([
                file("sample.rs", "pub fn after_review() {}\n"),
                file(
                    "src/math.rs",
                    "/// Add two values.\npub fn add(left: i32, right: i32) -> i32 {\n    left + right\n}\n\n/// Subtract two values.\npub fn subtract(left: i32, right: i32) -> i32 {\n    left - right\n}\n",
                ),
                file(
                    "notes.md",
                    "# Review fixture\n\nUnicode: café, 日本語, 🦀.\n\nA deliberately long line to explore wrapping and horizontal scrolling in a narrow terminal, with enough text to extend beyond the default diff pane width.\n",
                ),
            ]),
        }
    }
}

/// A scratch repository and the session's files, in `directory`, which outlives the session.
pub(crate) struct Scratch {
    repository: Box<dyn ReviewRepositoryFixture>,
    /// The root of the workspace of the repository the reviewer and its agent run in: the
    /// repository's own, or one `enter_workspace` added.
    workspace: PathBuf,
    directory: PathBuf,
    /// The stand-ins' event socket's directory: a temporary directory, since a socket's path is
    /// short and the session directory's may not be.
    runtime: tempfile::TempDir,
    port: TestPort,
}

impl Scratch {
    pub(crate) fn create(kind: RepoType, seed: &Seed, directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(
            directory
                .parent()
                .context("the session directory needs a parent")?,
        )?;
        fs::create_dir(&directory).context("the session directory must be new")?;
        let repository = repository_fixture(kind);
        for (path, content) in &seed.base {
            repository.write(path, content.as_bytes());
        }
        repository.new_change("base");
        for (path, content) in &seed.change {
            repository.write(path, content.as_bytes());
        }
        let scratch = Self {
            workspace: repository.root().to_owned(),
            repository,
            directory,
            runtime: tempfile::Builder::new().prefix("vision-").tempdir()?,
            port: TestPort::new(),
        };
        fs::create_dir_all(scratch.state())?;
        fs::create_dir_all(scratch.turns())?;
        fs::create_dir_all(scratch.agent_directory().path())?;
        // The Explore page's network listener, and the pane's QR code, on the loopback
        // interface: shown as on a real network, reachable from this machine only, on any free
        // port.
        let mut settings = ExplorePageSettings::default();
        settings.network.set_interface(LOOPBACK);
        settings.network.first_port = 0;
        write_reviewer_settings(&scratch.state(), &settings);
        scratch.write_browser()?;
        Ok(scratch)
    }

    /// The root of the workspace the reviewer and its agent run in.
    pub(crate) fn root(&self) -> &Path {
        &self.workspace
    }

    /// Run the reviewer and its agent, from their next start, in the workspace of the repository
    /// named `name`: a jj workspace at the change under review, or a Git worktree, which the
    /// first call with that name adds beside the repository. A name that is the repository's
    /// own root's goes back to it.
    pub(crate) fn enter_workspace(&mut self, name: &str) -> Result<()> {
        ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "a workspace name holds letters, digits and dashes"
        );
        let main = self.repository.root();
        let root = main.with_file_name(name);
        if root != main && !root.exists() {
            self.repository.add_workspace(name);
        }
        self.workspace = root;
        Ok(())
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn mcp_port(&self) -> u16 {
        self.port.number()
    }

    /// The reviewer's private state directory.
    fn state(&self) -> PathBuf {
        self.directory.join("state")
    }

    /// Where run-ahead says what the forks do.
    pub(crate) fn run_ahead_log(&self) -> PathBuf {
        self.state().join("run-ahead.log")
    }

    /// The stand-in agent's directory: its switches, and what its forks record.
    pub(crate) fn agent_directory(&self) -> Directory {
        Directory::new(self.directory.join("agent"))
    }

    /// The Claude Code configuration of the stand-in agent and its forks, with their transcripts.
    pub(crate) fn claude_config(&self) -> PathBuf {
        self.directory.join("claude-config")
    }

    pub(crate) fn runtime(&self) -> &Path {
        self.runtime.path()
    }

    /// The socket the stand-in agent and its forks report on.
    pub(crate) fn events_socket(&self) -> PathBuf {
        self.runtime().join("stand-in.sock")
    }

    /// The script the `jev` tool writes; the reviewer reads it in place of the paid classifier.
    pub(crate) fn jev_script(&self) -> PathBuf {
        self.directory.join("jev-script.json")
    }

    /// Where the reviewer records the Explore prompts it sent, for the scripted agent.
    pub(crate) fn turns(&self) -> PathBuf {
        self.directory.join("turns")
    }

    fn agent_program(&self) -> PathBuf {
        self.agent_bin().join("claude")
    }

    /// The directory that holds the stand-in agent as `claude`, first on the sandbox's `PATH`.
    fn agent_bin(&self) -> PathBuf {
        self.directory.join("agent-bin")
    }

    /// Installs the stand-in agent as `claude`, the name herdr-core detects Claude Code by: the
    /// one Cargo built in this workspace's profile directory, beside the running program, whose
    /// fake agent is the one this workspace speaks to on the event socket. The workspace at the
    /// root takes fake-agent from git, at a revision that may differ from the checkout's.
    pub(crate) fn install_agent(&self) -> Result<()> {
        let running = std::env::current_exe()?;
        let mut profile = running
            .parent()
            .context("the running program has no directory")?;
        // A test runs from the profile's `deps`.
        if profile.ends_with("deps") {
            profile = profile.parent().context("`deps` has no parent")?;
        }
        let program = stand_in_agent::program(profile).map_err(|error| anyhow!(error))?;
        fs::create_dir_all(self.agent_bin())?;
        stand_in_agent::install(&program, &self.agent_program())?;
        Ok(())
    }

    /// The program of the stand-in agent's terminal, on its new session `session`: it plays
    /// Claude Code, reports on the session's event socket and, with `hooks`, runs the hooks of
    /// the reviewer's Claude Code plugin through the `reviewer-control` beside `reviewer`. Each
    /// turn ends by itself, unless the driver holds turns.
    pub(crate) fn agent_command(&self, reviewer: &Path, session: &str, hooks: bool) -> Vec<String> {
        let mut variables = vec![
            (variables::EVENTS, path_string(&self.events_socket())),
            (
                variables::DIRECTORY,
                path_string(self.agent_directory().path()),
            ),
            (variables::NAME, AGENT.to_owned()),
            (variables::SESSION, session.to_owned()),
            (variables::TURNS, "auto".to_owned()),
            ("CLAUDE_CONFIG_DIR", path_string(&self.claude_config())),
            ("BROWSER", path_string(&self.browser())),
        ];
        if hooks {
            variables.push((
                variables::HOOK,
                path_string(&reviewer.with_file_name("reviewer-control")),
            ));
        }
        // The reviewer takes on its agent's environment under herdr-core: what it must not see
        // goes from the agent's too, and its browser comes from there. The browser goes here
        // rather than in the sandbox's environment, where herdr-common's app sandbox sets its
        // own after the caller's variables (a gap of herdr-common's).
        ["/usr/bin/env", "-u", "TYPESAFE_API_KEY", "-u", "NO_COLOR"]
            .into_iter()
            .map(str::to_owned)
            .chain(
                variables
                    .into_iter()
                    .map(|(name, value)| format!("{name}={value}")),
            )
            .chain([path_string(&self.agent_program())])
            .collect()
    }

    /// The program of the reviewer's terminal: `reviewer`, following the agent's terminal
    /// `agent` and the focus of the app whose home is `app_home`, as the launcher mod opens it,
    /// without the paid classifier's key, in colors whatever the user's terminal says, with the
    /// session's browser.
    pub(crate) fn reviewer_command(
        &self,
        reviewer: &Path,
        agent: &str,
        app_home: &str,
    ) -> Vec<String> {
        ["/usr/bin/env", "-u", "TYPESAFE_API_KEY", "-u", "NO_COLOR"]
            .into_iter()
            .map(str::to_owned)
            .chain([
                format!("BROWSER={}", path_string(&self.browser())),
                path_string(reviewer),
                "--focused-terminal".to_owned(),
                agent.to_owned(),
                "--herdr-app-home".to_owned(),
                app_home.to_owned(),
            ])
            .collect()
    }

    /// The variables every process of the app sandbox gets: the reviewer's in a vision session,
    /// with private state, and a `PATH` that finds the stand-in agent as `claude` first.
    pub(crate) fn sandbox_environment(&self) -> Vec<(OsString, OsString)> {
        let mut path = OsString::from(self.agent_bin());
        if let Some(inherited) = std::env::var_os("PATH") {
            path.push(":");
            path.push(inherited);
        }
        [
            ("HERDR_REVIEWER_VISION", OsString::from("1")),
            ("HERDR_PLUGIN_STATE_DIR", self.state().into()),
            (
                "HERDR_REVIEWER_MCP_PORT",
                self.mcp_port().to_string().into(),
            ),
            ("HERDR_REVIEWER_JEV_SCRIPT", self.jev_script().into()),
            ("HERDR_REVIEWER_VISION_TURNS", self.turns().into()),
            ("PATH", path),
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), value))
        .collect()
    }

    /// Run the Herdr action that opens the Explore page, as Herdr would run it in the reviewer's
    /// space `space`, with a browser that records the address it opens. Returns that address,
    /// with its token.
    pub(crate) fn open_explore_page(&self, reviewer: &Path, space: &str) -> Result<String> {
        let opened = self.directory.join("explore-page-url");
        let _ = fs::remove_file(&opened);
        let browser = self.directory.join("browser");
        write_script(
            &browser,
            &format!("#!/bin/sh\nprintf %s \"$1\" > '{}'\n", opened.display()),
        )?;
        let context = PluginContext {
            workspace_id: Some(WorkspaceId(space.to_owned())),
            tab_id: None,
            focused_pane_id: None,
            focused_pane_cwd: Some(self.root().to_owned()),
        };
        let output = std::process::Command::new(reviewer.with_file_name("reviewer-control"))
            .arg("explore-page")
            .env(
                "HERDR_PLUGIN_CONTEXT_JSON",
                serde_json::to_string(&context)?,
            )
            .env("HERDR_PLUGIN_STATE_DIR", self.state())
            .env("BROWSER", &browser)
            .current_dir(self.root())
            .output()?;
        ensure!(
            output.status.success(),
            "the action failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(fs::read_to_string(&opened)?)
    }

    /// The reviewer's browser, which [`Self::write_browser`] writes.
    fn browser(&self) -> PathBuf {
        self.directory.join("stand-in-browser")
    }

    /// The reviewer's browser: it appends each address it opens to `opened-pages` in the
    /// session directory, and fails with exit status 3 while a file `browser-fails` exists
    /// there. A vision session never opens the user's browser.
    fn write_browser(&self) -> Result<()> {
        write_script(
            &self.browser(),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{opened}'\n[ ! -e '{fails}' ] || exit 3\n",
                opened = self.directory.join("opened-pages").display(),
                fails = self.directory.join("browser-fails").display(),
            ),
        )
    }
}

fn write_script(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
