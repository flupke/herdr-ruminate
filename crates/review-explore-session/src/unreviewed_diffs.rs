use std::collections::BTreeMap;
use std::fmt;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use eyre::WrapErr;
use review_repository::repository::{ChangedFile, Snapshot};
use review_state::{OpenRow, ReviewTracker, RowChange};
use tempfile::TempDir;

#[derive(Debug)]
pub(crate) struct UnreviewedDiffs {
    directory: TempDir,
    displaced: Vec<(String, String)>,
}

const INDEX: &str = "__herdr_reviewer_index__";
const RESERVED: &str = "__herdr_reviewer_";

struct FileDiff<'a> {
    name: &'a str,
    hunks: &'a [Vec<OpenRow>],
}

impl UnreviewedDiffs {
    pub(crate) fn write<'a>(
        tracker: &ReviewTracker,
        snapshot: &Snapshot,
        files: impl Iterator<Item = &'a ChangedFile>,
    ) -> eyre::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("herdr-review-unreviewed-")
            .permissions(Permissions::from_mode(0o700))
            .tempdir()?;
        let mut diffs = Self {
            directory,
            displaced: Vec::new(),
        };
        for file in files {
            let name = file.review_path().display();
            let hunks = tracker
                .open_rows(snapshot, file)
                .wrap_err_with(|| name.clone())?;
            let diff = FileDiff {
                name: &name,
                hunks: &hunks,
            };
            diffs
                .place(&name, &diff.to_string())
                .wrap_err_with(|| name.clone())?;
        }
        diffs.write_index()?;
        Ok(diffs)
    }

    pub(crate) fn directory(&self) -> &Path {
        self.directory.path()
    }

    pub(crate) fn same_lines(&self, other: &Self) -> bool {
        files(self.directory())
            .is_ok_and(|ours| files(other.directory()).is_ok_and(|theirs| ours == theirs))
    }

    pub(crate) fn index(&self) -> Option<PathBuf> {
        (!self.displaced.is_empty()).then(|| self.directory().join(INDEX))
    }

    fn place(&mut self, name: &str, text: &str) -> std::io::Result<()> {
        if !Path::new(name)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        if !name.starts_with(RESERVED) && !self.taken(name) {
            return self.write_at(name, text);
        }
        let displaced = format!("{RESERVED}displaced_{}__", self.displaced.len() + 1);
        std::fs::write(self.directory().join(&displaced), text)?;
        self.displaced.push((name.to_owned(), displaced));
        Ok(())
    }

    fn taken(&self, name: &str) -> bool {
        let path = self.directory().join(name);
        path.exists()
            || path
                .ancestors()
                .skip(1)
                .take_while(|ancestor| *ancestor != self.directory())
                .any(Path::is_file)
    }

    fn write_at(&self, name: &str, text: &str) -> std::io::Result<()> {
        let path = self.directory().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)
    }

    fn write_index(&self) -> std::io::Result<()> {
        let Some(index) = self.index() else {
            return Ok(());
        };
        let mut text = String::from(
            "These diffs are not at their repository path: another diff uses it as a file or as \
             a directory, or its name is the reviewer's own:\n",
        );
        for (name, displaced) in &self.displaced {
            text.push_str(name);
            text.push_str(" -> ");
            text.push_str(displaced);
            text.push('\n');
        }
        std::fs::write(index, text)
    }
}

fn files(directory: &Path) -> std::io::Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut found = BTreeMap::new();
    let mut directories = vec![directory.to_owned()];
    while let Some(current) = directories.pop() {
        for entry in std::fs::read_dir(&current)? {
            let path = entry?.path();
            if path.is_dir() {
                directories.push(path);
            } else {
                let contents = std::fs::read(&path)?;
                let relative = path.strip_prefix(directory).unwrap_or(&path).to_owned();
                found.insert(relative, contents);
            }
        }
    }
    Ok(found)
}

impl fmt::Display for FileDiff<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(output, "{}: unreviewed lines", self.name)?;
        if self.hunks.is_empty() {
            return writeln!(
                output,
                "No text lines to show: a binary, mode or whole-file change."
            );
        }
        writeln!(
            output,
            "old = line in the base, new = line in the current file. A - row without a number\n\
             rewrites a reviewed line: mark it with the numbered rows of its change, or the\n\
             whole file when its change has none."
        )?;
        let width = self
            .hunks
            .iter()
            .flatten()
            .filter_map(|row| row.base_line.max(row.current_line))
            .max()
            .unwrap_or(0)
            .to_string()
            .len()
            .max(3);
        let number = |line: Option<u32>| line.map_or_else(String::new, |line| line.to_string());
        for hunk in self.hunks {
            writeln!(output, "\n{:>width$} {:>width$}", "old", "new")?;
            for row in hunk {
                let (marker, old) = match row.change {
                    RowChange::Unchanged => (' ', row.base_line),
                    RowChange::Removed => ('-', row.base_line),
                    RowChange::Added => ('+', None),
                };
                writeln!(
                    output,
                    "{:>width$} {:>width$} {marker} {}",
                    number(old),
                    number(row.current_line),
                    row.text,
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "unreviewed_diffs.tests.rs"]
mod tests;
