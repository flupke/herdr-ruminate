//! Where the reviewer keeps its state and reads its configuration.
//!
//! Herdr gives its plugin a state folder and a configuration folder, in
//! `HERDR_PLUGIN_STATE_DIR` and `HERDR_PLUGIN_CONFIG_DIR`; herdr-app has no plugin folders. The
//! reviewer takes Herdr's when they are set, under Herdr and in its tests, and folders of its own
//! under the XDG base directories otherwise. The first time it uses its own state folder, it
//! moves there the state Herdr kept for the plugin, and leaves a link in its place, so that a
//! review pane under Herdr shares it.

use std::ffi::OsString;
use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

/// The name of the reviewer's own folders.
const NAME: &str = "herdr-progressive-reviewer";

/// Herdr's ID of the plugin, which names the folders Herdr gives it.
const HERDR_PLUGIN_ID: &str = "herdr.progressive-reviewer";

/// The folder the reviewer keeps its state in, the state root: Herdr's plugin state folder
/// under Herdr, else the reviewer's own, into which the state Herdr kept moves first.
///
/// # Errors
/// Fails when neither Herdr nor the environment gives a state folder: no home directory.
pub fn state_root() -> io::Result<PathBuf> {
    Dirs::from_vars(|name| std::env::var_os(name)).state_root()
}

/// The state root, as [`state_root`] gives it, made when missing: readable by the user alone, as
/// Herdr made its plugin folders.
///
/// # Errors
/// Fails without a state folder, or when the folder cannot be made.
pub fn create_state_root() -> io::Result<PathBuf> {
    Dirs::from_vars(|name| std::env::var_os(name)).create_state_root()
}

/// The folder of the reviewer's `config.toml`: Herdr's plugin configuration folder under
/// Herdr, else the reviewer's own; `None` without a home directory.
pub fn config_dir() -> Option<PathBuf> {
    Dirs::from_vars(|name| std::env::var_os(name)).config_dir()
}

/// The folders the environment gives the reviewer.
#[derive(Debug)]
struct Dirs {
    herdr_state: Option<PathBuf>,
    herdr_config: Option<PathBuf>,
    /// `$XDG_STATE_HOME`, else `~/.local/state`.
    state_home: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME`, else `~/.config`.
    config_home: Option<PathBuf>,
}

impl Dirs {
    /// The folders, from the environment variables `var` reads. The XDG specification ignores a
    /// relative base directory.
    fn from_vars(var: impl Fn(&str) -> Option<OsString>) -> Self {
        let set = |name: &str| {
            var(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        let home = set("HOME");
        let base = |name: &str, default: &str| {
            set(name)
                .filter(|directory| directory.is_absolute())
                .or_else(|| home.as_ref().map(|home| home.join(default)))
        };
        Self {
            herdr_state: set("HERDR_PLUGIN_STATE_DIR"),
            herdr_config: set("HERDR_PLUGIN_CONFIG_DIR"),
            state_home: base("XDG_STATE_HOME", ".local/state"),
            config_home: base("XDG_CONFIG_HOME", ".config"),
        }
    }

    fn state_root(&self) -> io::Result<PathBuf> {
        if let Some(herdr) = &self.herdr_state {
            return Ok(herdr.clone());
        }
        let state_home = self.state_home.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no state folder for the reviewer: set HOME or XDG_STATE_HOME",
            )
        })?;
        Ok(adopt_herdr_state(
            &state_home.join("herdr/plugins").join(HERDR_PLUGIN_ID),
            &state_home.join(NAME),
        ))
    }

    fn create_state_root(&self) -> io::Result<PathBuf> {
        let root = self.state_root()?;
        if !root.exists() {
            DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&root)?;
        }
        Ok(root)
    }

    fn config_dir(&self) -> Option<PathBuf> {
        self.herdr_config
            .clone()
            .or_else(|| self.config_home.as_ref().map(|home| home.join(NAME)))
    }
}

/// Moves the state Herdr kept for the plugin in `herdr` to `own`, the first time: when `own`
/// does not exist and `herdr` is a folder. Leaves a link to `own` at `herdr`, for the review
/// panes Herdr starts. Returns the state root: `herdr` when the move fails, as across file
/// systems, or when `herdr` is a link the user made to a folder elsewhere, which stays theirs.
fn adopt_herdr_state(herdr: &Path, own: &Path) -> PathBuf {
    if own.exists() {
        return own.to_owned();
    }
    match fs::symlink_metadata(herdr) {
        Ok(metadata) if metadata.is_symlink() => {
            return if fs::read_link(herdr).is_ok_and(|target| target == own) {
                own.to_owned()
            } else {
                herdr.to_owned()
            };
        }
        Ok(metadata) if metadata.is_dir() => {}
        _ => return own.to_owned(),
    }
    let moved = own
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::rename(herdr, own));
    match moved {
        Ok(()) => {
            // Without the link, a review pane under Herdr starts on an empty state; the
            // reviewer still works.
            let _ = std::os::unix::fs::symlink(own, herdr);
            own.to_owned()
        }
        // Another reviewer moved it first.
        Err(_) if own.exists() => own.to_owned(),
        Err(_) => herdr.to_owned(),
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
