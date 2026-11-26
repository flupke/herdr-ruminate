use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Unreviewed {
    pub directory: PathBuf,
    pub files: usize,
    pub index: Option<PathBuf>,
    pub notice: Option<String>,
}

impl fmt::Display for Unreviewed {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(notice) = &self.notice {
            writeln!(output, "\nNote: {notice}")?;
        }
        if self.files == 0 {
            return writeln!(
                output,
                "\nUnreviewed diffs: none; every changed line is reviewed."
            );
        }
        writeln!(
            output,
            "\nUnreviewed diffs: {}/<repository path>, for the {} {} with unreviewed lines",
            self.directory.display(),
            self.files,
            if self.files == 1 { "file" } else { "files" }
        )?;
        if let Some(index) = &self.index {
            writeln!(
                output,
                "Displaced diffs: {} says where the diffs that are not at their repository path are",
                index.display()
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "unreviewed.tests.rs"]
mod tests;
