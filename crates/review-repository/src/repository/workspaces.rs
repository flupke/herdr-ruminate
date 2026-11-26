use std::path::PathBuf;

use review_types::ReviewUnit;

use super::git::GitBackend;
use super::watch::resolve_directory_or_link_file;
use super::{RepoType, Repository, RepositoryProcess};
use crate::{Error, Result};

// Linux allows 128 KiB for one argument, which a revset that names many reviews can pass.
const UNITS_PER_QUERY: usize = 512;

impl Repository {
    pub fn shared_store(&self) -> Result<PathBuf> {
        let store = match self.repo_type() {
            RepoType::Jj => resolve_directory_or_link_file(&self.root.join(".jj/repo")),
            RepoType::Git => GitBackend::git_directory(&self.root)
                .map(|git| resolve_directory_or_link_file(&git.join("commondir")).unwrap_or(git)),
        };
        store.ok_or_else(|| Error::NotRepository {
            path: self.root.clone(),
        })
    }

    pub fn workspace_roots(&self) -> Result<Vec<PathBuf>> {
        let output = match self.repo_type() {
            RepoType::Jj => self.run_jj([
                "--ignore-working-copy",
                "workspace",
                "list",
                "-T",
                r#"self.root() ++ "\0""#,
            ])?,
            RepoType::Git => self.run_plain_git(&["worktree", "list", "--porcelain", "-z"], b"")?,
        };
        let roots = output
            .stdout
            .split(|byte| *byte == 0)
            .filter_map(|field| match self.repo_type() {
                RepoType::Jj => Some(field),
                RepoType::Git => field.strip_prefix(b"worktree "),
            })
            .filter(|path| !path.is_empty())
            .filter_map(|path| {
                let path = std::str::from_utf8(path).ok()?;
                std::fs::canonicalize(self.root.join(path)).ok()
            })
            .collect();
        Ok(roots)
    }

    pub fn holds_any_review_unit<'a>(
        &self,
        units: impl IntoIterator<Item = &'a ReviewUnit>,
    ) -> Result<bool> {
        let units = units.into_iter().map(ReviewUnit::as_str);
        match self.repo_type() {
            RepoType::Jj => self.holds_any_change(
                &units
                    .filter(|unit| in_alphabet(unit, |byte| (b'k'..=b'z').contains(&byte)))
                    .collect::<Vec<_>>(),
            ),
            RepoType::Git => self.holds_any_tree(
                &units
                    .filter(|unit| {
                        in_alphabet(unit, |byte| {
                            byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                        })
                    })
                    .collect::<Vec<_>>(),
            ),
        }
    }

    fn holds_any_change(&self, changes: &[&str]) -> Result<bool> {
        for chunk in changes.chunks(UNITS_PER_QUERY) {
            // jj: the revset `<change>/0` finds a hidden change too, where a bare change ID does not.
            let present = chunk
                .iter()
                .map(|change| format!("present({change}/0)"))
                .collect::<Vec<_>>()
                .join(" | ");
            let revset = format!("({present}) ~ root()");
            let output = self.run_jj([
                "--ignore-working-copy",
                "log",
                "--no-graph",
                "--limit",
                "1",
                "-r",
                &revset,
                "-T",
                r#"change_id ++ "\n""#,
            ])?;
            if !output.stdout.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn holds_any_tree(&self, trees: &[&str]) -> Result<bool> {
        if trees.is_empty() {
            return Ok(false);
        }
        let mut input = trees.join("\n");
        input.push('\n');
        let output = self.run_plain_git(
            &["cat-file", "--batch-check=%(objecttype)"],
            input.as_bytes(),
        )?;
        Ok(output
            .stdout
            .split(|byte| *byte == b'\n')
            .any(|line| line == b"tree"))
    }

    fn run_plain_git(&self, arguments: &[&str], input: &[u8]) -> Result<std::process::Output> {
        let output =
            RepositoryProcess::new("git", &self.root, "read Git repository", &self.cancellation)
                .with_input(input)
                .output(arguments)?;
        if !output.status.success() {
            return Err(Error::CommandFailed {
                operation: "read Git repository".to_owned(),
                code: output.status.code(),
            });
        }
        Ok(output)
    }
}

fn in_alphabet(unit: &str, accepts: impl Fn(u8) -> bool) -> bool {
    !unit.is_empty() && unit.bytes().all(accepts)
}

#[cfg(test)]
#[path = "workspaces.tests.rs"]
mod tests;
