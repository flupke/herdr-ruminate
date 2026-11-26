use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, ensure};
use fake_agent::{Directory, variables};
use herdr_client::protocol::{PluginContext, TabId, WorkspaceId};
use review_explore_page_settings::ExplorePageSettings;
use review_repository::repository::RepoType;
use review_test_support::{
    ReviewRepositoryFixture, TestPort, repository_fixture_in, write_reviewer_settings,
};

use super::session::AGENT;

// The bubblewrap sandbox shows the host's `/var/tmp` but not its `/tmp`; inside it, direnv
// refuses an `.envrc`, which fails the language servers.
const REPOSITORIES: &str = "/var/tmp";

#[cfg(target_os = "linux")]
const LOOPBACK: &str = "lo";
#[cfg(not(target_os = "linux"))]
const LOOPBACK: &str = "lo0";

#[derive(Clone, Debug, Default)]
pub(crate) struct Seed {
    pub(crate) base: BTreeMap<String, String>,
    pub(crate) change: BTreeMap<String, String>,
}

impl Seed {
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

pub(crate) struct Scratch {
    repository: Box<dyn ReviewRepositoryFixture>,
    workspace: PathBuf,
    directory: PathBuf,
    port: TestPort,
    page_port: TestPort,
}

impl Scratch {
    pub(crate) fn create(kind: RepoType, seed: &Seed, directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(
            directory
                .parent()
                .context("the session directory needs a parent")?,
        )?;
        fs::create_dir(&directory).context("the session directory must be new")?;
        let repository = repository_fixture_in(kind, Path::new(REPOSITORIES));
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
            port: TestPort::new(),
            page_port: TestPort::new(),
        };
        fs::create_dir_all(scratch.state())?;
        fs::create_dir_all(scratch.turns())?;
        fs::create_dir_all(scratch.agent_directory().path())?;
        let mut settings = ExplorePageSettings::default();
        settings.network.set_interface(LOOPBACK);
        settings.network.first_port = scratch.page_port.number();
        write_reviewer_settings(&scratch.state(), &settings);
        scratch.write_browser()?;
        Ok(scratch)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.workspace
    }

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

    pub(crate) fn page_port(&self) -> u16 {
        self.page_port.number()
    }

    fn state(&self) -> PathBuf {
        self.directory.join("state")
    }

    fn bus(&self) -> PathBuf {
        self.directory.join("bus.db")
    }

    pub(crate) fn run_ahead_log(&self) -> PathBuf {
        self.state().join("run-ahead.log")
    }

    pub(crate) fn agent_directory(&self) -> Directory {
        Directory::new(self.directory.join("agent"))
    }

    pub(crate) fn claude_config(&self) -> PathBuf {
        self.directory.join("claude-config")
    }

    pub(crate) fn jev_script(&self) -> PathBuf {
        self.directory.join("jev-script.json")
    }

    pub(crate) fn turns(&self) -> PathBuf {
        self.directory.join("turns")
    }

    fn agent_program(&self) -> PathBuf {
        self.agent_bin().join("claude")
    }

    fn agent_bin(&self) -> PathBuf {
        self.directory.join("agent-bin")
    }

    pub(crate) fn install_agent(&self) -> Result<()> {
        let running = std::env::current_exe()?;
        let mut profile = running
            .parent()
            .context("the running program has no directory")?;
        // Cargo runs a test binary from the profile's `deps` directory.
        if profile.ends_with("deps") {
            profile = profile.parent().context("`deps` has no parent")?;
        }
        let program = reviewer_fake_agent::program(profile).map_err(|error| anyhow!(error))?;
        fs::create_dir_all(self.agent_bin())?;
        reviewer_fake_agent::install(&program, &self.agent_program())?;
        Ok(())
    }

    pub(crate) fn agent_command(
        &self,
        reviewer: &Path,
        session: &str,
        hooks: bool,
        events: &Path,
    ) -> Vec<String> {
        let mut variables = vec![
            (variables::EVENTS, path_string(events)),
            (
                variables::DIRECTORY,
                path_string(self.agent_directory().path()),
            ),
            (variables::NAME, AGENT.to_owned()),
            (variables::SESSION, session.to_owned()),
            (variables::TURNS, "auto".to_owned()),
            ("CLAUDE_CONFIG_DIR", path_string(&self.claude_config())),
        ];
        if hooks {
            variables.push((
                variables::HOOK,
                path_string(&reviewer.with_file_name("reviewer-control")),
            ));
        }
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

    pub(crate) fn reviewer_command(reviewer: &Path, agent: &str, app_home: &str) -> Vec<String> {
        ["/usr/bin/env", "-u", "TYPESAFE_API_KEY", "-u", "NO_COLOR"]
            .into_iter()
            .map(str::to_owned)
            .chain([
                path_string(reviewer),
                "--focused-terminal".to_owned(),
                agent.to_owned(),
                "--herdr-app-home".to_owned(),
                app_home.to_owned(),
            ])
            .collect()
    }

    pub(crate) fn sandbox_environment(&self) -> Vec<(OsString, OsString)> {
        let mut path = OsString::from(self.agent_bin());
        if let Some(inherited) = std::env::var_os("PATH") {
            path.push(":");
            path.push(inherited);
        }
        [
            ("HERDR_REVIEWER_VISION", OsString::from("1")),
            ("HERDR_PLUGIN_STATE_DIR", self.state().into()),
            ("HERDR_BUS_DB", self.bus().into()),
            (
                "HERDR_REVIEWER_MCP_PORT",
                self.mcp_port().to_string().into(),
            ),
            ("HERDR_REVIEWER_JEV_SCRIPT", self.jev_script().into()),
            ("HERDR_REVIEWER_VISION_TURNS", self.turns().into()),
            ("BROWSER", self.browser().into()),
            ("PATH", path),
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), value))
        .collect()
    }

    pub(crate) fn open_explore_page(
        &self,
        mut action: std::process::Command,
        space: &str,
        tab: &str,
    ) -> Result<String> {
        let opened = self.directory.join("explore-page-url");
        let _ = fs::remove_file(&opened);
        let browser = self.directory.join("browser");
        write_script(
            &browser,
            &format!("#!/bin/sh\nprintf %s \"$1\" > '{}'\n", opened.display()),
        )?;
        let context = PluginContext {
            workspace_id: Some(WorkspaceId(space.to_owned())),
            tab_id: Some(TabId(tab.to_owned())),
            focused_pane_id: None,
            focused_pane_cwd: Some(self.root().to_owned()),
        };
        let output = action
            .arg("explore-page")
            .env(
                "HERDR_PLUGIN_CONTEXT_JSON",
                serde_json::to_string(&context)?,
            )
            .env("BROWSER", &browser)
            .output()?;
        ensure!(
            output.status.success(),
            "the action failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(fs::read_to_string(&opened)?)
    }

    fn browser(&self) -> PathBuf {
        self.directory.join("stand-in-browser")
    }

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
