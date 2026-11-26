use std::ffi::OsString;
use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

const NAME: &str = "herdr-progressive-reviewer";

const HERDR_PLUGIN_ID: &str = "herdr.progressive-reviewer";

pub fn state_root() -> io::Result<PathBuf> {
    Dirs::from_vars(|name| std::env::var_os(name)).state_root()
}

// Herdr makes its plugin folders readable by the user alone.
pub fn create_state_root() -> io::Result<PathBuf> {
    Dirs::from_vars(|name| std::env::var_os(name)).create_state_root()
}

pub fn config_dir() -> Option<PathBuf> {
    Dirs::from_vars(|name| std::env::var_os(name)).config_dir()
}

#[derive(Debug)]
struct Dirs {
    herdr_state: Option<PathBuf>,
    herdr_config: Option<PathBuf>,
    state_home: Option<PathBuf>,
    config_home: Option<PathBuf>,
}

impl Dirs {
    // The XDG specification ignores a relative base directory.
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
            // Herdr gives its plugin these folders; herdr-app has no plugin folders.
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
            let _ = std::os::unix::fs::symlink(own, herdr);
            own.to_owned()
        }
        Err(_) if own.exists() => own.to_owned(),
        Err(_) => herdr.to_owned(),
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
