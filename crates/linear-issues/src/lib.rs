//! The Linear issues a reviewed change names, found by the keys of the teams that the `[linear]`
//! table of the reviewer's `config.toml` lists, and linked to the pages of the Linear workspace
//! it names:
//!
//! ```toml
//! [linear]
//! workspace = "acme"
//! teams = ["ENG", "OPS"]
//! ```
//!
//! An identifier is a team's key, a dash and a number, in any case, between characters that are
//! not ASCII letters or digits: `(ENG-12)` in a description, `eng-12-fix-login` as a workspace's
//! name. Matching the listed keys alone keeps `UTF-8` or `SHA-256` from reading as issues.

use reviewer_config::ReviewerConfig;
use serde::Serialize;
use ts_rs::TS;

/// A Linear issue that the reviewed change names, with the address of its page.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
pub struct LinearIssue {
    /// Its identifier, as Linear writes it: the team's key in capitals, a dash and its number.
    pub id: String,
    /// The address of its page on linear.app.
    pub url: String,
}

/// The Linear workspace whose issues the reviewer links to, and the keys of the teams whose
/// identifiers it looks for, as the `[linear]` table of the reviewer's `config.toml` names them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinearSettings {
    /// The workspace's name in its pages' addresses: `acme` in `linear.app/acme`.
    workspace: String,
    /// The teams' keys, in capitals.
    teams: Vec<String>,
}

impl LinearSettings {
    /// The settings of the `[linear]` table of `config`; none without the table.
    ///
    /// # Errors
    /// Fails when the table lacks `workspace` or `teams`, or when either is not what it must be.
    pub fn from_config(config: &ReviewerConfig) -> eyre::Result<Option<Self>> {
        let Some(table) = config.table("linear")? else {
            return Ok(None);
        };
        let file = config.path().display();
        let workspace = table
            .get("workspace")
            .and_then(toml::Value::as_str)
            .filter(|workspace| is_name(workspace))
            .ok_or_else(|| {
                eyre::eyre!(
                    "`linear.workspace` in {file} must be the name of the Linear workspace in \
                     its addresses, such as \"acme\" in linear.app/acme"
                )
            })?;
        let teams = table
            .get("teams")
            .and_then(toml::Value::as_array)
            .filter(|teams| !teams.is_empty())
            .and_then(|teams| {
                teams
                    .iter()
                    .map(|team| {
                        team.as_str()
                            .filter(|team| is_key(team))
                            .map(str::to_ascii_uppercase)
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .ok_or_else(|| {
                eyre::eyre!(
                    "`linear.teams` in {file} must list the keys of the Linear teams, such as \
                     [\"ENG\", \"OPS\"]"
                )
            })?;
        Ok(Some(Self {
            workspace: workspace.to_owned(),
            teams,
        }))
    }

    /// The issues that `texts` name, in the order they first appear, each once.
    pub fn issues<'a>(&self, texts: impl IntoIterator<Item = &'a str>) -> Vec<LinearIssue> {
        let mut issues: Vec<LinearIssue> = Vec::new();
        for text in texts {
            for id in self.identifiers(text) {
                if issues.iter().all(|issue| issue.id != id) {
                    issues.push(LinearIssue {
                        url: format!("https://linear.app/{}/issue/{id}", self.workspace),
                        id,
                    });
                }
            }
        }
        issues
    }

    /// The identifiers in `text`, in capitals, in the order they appear.
    fn identifiers<'a>(&'a self, text: &'a str) -> impl Iterator<Item = String> + 'a {
        let bytes = text.as_bytes();
        (0..bytes.len())
            .filter(move |&start| start == 0 || !bytes[start - 1].is_ascii_alphanumeric())
            .filter_map(move |start| {
                self.teams
                    .iter()
                    .find_map(|team| identifier_at(&bytes[start..], team))
            })
    }
}

/// The identifier of `team` at the start of `bytes`, in capitals: the key in any case, a dash,
/// then digits that no letter or digit follows.
fn identifier_at(bytes: &[u8], team: &str) -> Option<String> {
    let key = bytes.get(..team.len())?;
    if !key.eq_ignore_ascii_case(team.as_bytes()) || bytes.get(team.len()) != Some(&b'-') {
        return None;
    }
    let rest = &bytes[team.len() + 1..];
    let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    if digits == 0 || rest.get(digits).is_some_and(u8::is_ascii_alphanumeric) {
        return None;
    }
    // The digits are ASCII: the slice is valid UTF-8.
    let number = std::str::from_utf8(&rest[..digits]).ok()?;
    Some(format!("{team}-{number}"))
}

/// Whether `workspace` can be the name of a Linear workspace in an address.
fn is_name(workspace: &str) -> bool {
    !workspace.is_empty()
        && workspace
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Whether `team` can be the key of a Linear team: letters and digits, such as `ENG` or `3D`.
fn is_key(team: &str) -> bool {
    !team.is_empty() && team.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
