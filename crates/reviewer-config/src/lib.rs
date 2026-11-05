//! The reviewer's `config.toml`, in the folder [`reviewer_dirs::config_dir`] gives. It may hold
//! a secret, `typesafe_api_key`: its errors name the file, the line and the setting, never a
//! value, and it has no `Debug`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The settings of the reviewer's `config.toml`; none when the file is missing.
pub struct ReviewerConfig {
    path: PathBuf,
    table: toml::Table,
}

impl ReviewerConfig {
    /// Reads the reviewer's `config.toml`; empty without a home directory or a file.
    ///
    /// # Errors
    /// Fails when the file cannot be read or is not TOML.
    pub fn from_env() -> eyre::Result<Self> {
        match reviewer_dirs::config_dir() {
            Some(directory) => Self::from_dir(&directory),
            None => Ok(Self {
                path: PathBuf::from("config.toml"),
                table: toml::Table::new(),
            }),
        }
    }

    /// Reads `config.toml` in `directory`; empty without the file.
    ///
    /// # Errors
    /// Fails when the file cannot be read or is not TOML.
    pub fn from_dir(directory: &Path) -> eyre::Result<Self> {
        let path = directory.join("config.toml");
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(eyre::eyre!("cannot read {}: {error}", path.display()));
            }
        };
        let table = source.parse::<toml::Table>().map_err(|error| {
            // toml's own message quotes the line, which may hold a secret.
            let line = error
                .span()
                .map_or(1, |span| source[..span.start].matches('\n').count() + 1);
            eyre::eyre!(
                "{} is not valid TOML, at line {line}: {}",
                path.display(),
                error.message()
            )
        })?;
        Ok(Self { path, table })
    }

    /// The file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The string setting `name`, if the file has it.
    ///
    /// # Errors
    /// Fails when the setting is not a string.
    pub fn string(&self, name: &str) -> eyre::Result<Option<&str>> {
        self.table
            .get(name)
            .map(|value| {
                value.as_str().ok_or_else(|| {
                    eyre::eyre!("`{name}` in {} must be a string", self.path.display())
                })
            })
            .transpose()
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
