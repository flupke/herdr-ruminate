use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use review_store::{ReviewStore, WorkspaceState};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;

const MIGRATION_RECORD: &str = "migration.json";
const RESTORE: &str = "RESTORE.md";

#[derive(Deserialize, Serialize)]
struct MigrationRecord {
    version: u8,
    backup: PathBuf,
    started: String,
}

pub(super) fn before_merge(
    store: &ReviewStore,
    all: &[WorkspaceState],
    ours: &[WorkspaceState],
) -> std::io::Result<()> {
    let state_root = store.state_root();
    let record_path = state_root.join(MIGRATION_RECORD);
    let started = SystemTime::now();
    let Some(record) = read_record(&record_path)? else {
        let backup = copy(
            state_root,
            all.iter().map(WorkspaceState::path),
            started,
            true,
        )?;
        return write_record(
            &record_path,
            &MigrationRecord {
                version: 1,
                backup,
                started: rfc3339(started)?,
            },
        );
    };
    let backed_up: SystemTime = OffsetDateTime::parse(&record.started, &Rfc3339)
        .map_err(std::io::Error::other)?
        .into();
    let mut changed = Vec::new();
    for state in ours {
        let modified = state.last_modified().map_err(std::io::Error::other)?;
        if modified > backed_up {
            changed.push(state.path());
        }
    }
    if !changed.is_empty() {
        changed.push(store.directory());
        copy(state_root, changed.into_iter(), started, false)?;
    }
    Ok(())
}

fn copy<'a>(
    state_root: &Path,
    folders: impl Iterator<Item = &'a Path>,
    started: SystemTime,
    first: bool,
) -> std::io::Result<PathBuf> {
    let mut backups = state_root.as_os_str().to_owned();
    backups.push(".backups");
    let backups = PathBuf::from(backups);
    fs::create_dir_all(&backups)?;
    let name = OffsetDateTime::from(started)
        .format(format_description!(
            "[year]-[month]-[day]T[hour]-[minute]-[second]Z"
        ))
        .map_err(std::io::Error::other)?;
    let mut target = backups.join(&name);
    let mut attempt = 1;
    while target.exists() {
        attempt += 1;
        target = backups.join(format!("{name}-{attempt}"));
    }
    let mut partial = target.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }
    fs::create_dir(&partial)?;
    let copied = (|| {
        for folder in folders {
            let relative = folder
                .strip_prefix(state_root)
                .map_err(std::io::Error::other)?;
            copy_tree(folder, &partial.join(relative))?;
        }
        fs::write(
            partial.join(RESTORE),
            restore_steps(state_root, &target, first).as_bytes(),
        )?;
        // One sync of the file system writes every copied file; syncing each takes seconds for
        // thousands of files.
        sync_file_system(&partial)?;
        fs::rename(&partial, &target)
    })();
    if copied.is_err() {
        let _ = fs::remove_dir_all(&partial);
    }
    copied?;
    sync(&backups)?;
    Ok(target)
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
            fs::File::options()
                .write(true)
                .open(&target)?
                .set_modified(entry.metadata()?.modified()?)?;
        }
    }
    Ok(())
}

fn restore_steps(state_root: &Path, backup: &Path, first: bool) -> String {
    let root = state_root.display();
    let backup = backup.display();
    let scope = if first {
        "This backup holds every review state that each jj workspace or Git worktree kept for \
         itself, as it was before the reviewer first merged them into the state that the \
         workspaces of a repository share."
    } else {
        "This backup holds the per-workspace review states that changed after the first backup, \
         which a reviewer of an earlier build can do, and the shared state they were merged \
         into. To restore, first restore the first backup, then copy this one and every later \
         one over it in time order (step 3)."
    };
    format!(
        "# Restore the review state\n\n{scope}\n\n\
         1. Close every review pane.\n\
         2. In `{root}`, remove `repositories/`, `merged-workspaces/`, `migration.json` and \
         `migration.lock`.\n\
         3. Copy the backup back, leaving out this file:\n\n   \
         ```sh\n   cp -a {backup}/. {root}/\n   rm {root}/{RESTORE}\n   ```\n\n\
         4. Then either install a build from before the shared state, to go back to one state \
         per workspace, or keep this build: its next open merges the states again, after a new \
         backup.\n"
    )
}

fn read_record(path: &Path) -> std::io::Result<Option<MigrationRecord>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).map_err(std::io::Error::other)?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_record(path: &Path, record: &MigrationRecord) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    let temporary = path.with_extension("json.partial");
    write_synced(&temporary, &bytes)?;
    fs::rename(&temporary, path)?;
    sync(path.parent().unwrap_or(Path::new(".")))
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn sync_file_system(path: &Path) -> std::io::Result<()> {
    rustix::fs::syncfs(fs::File::open(path)?).map_err(std::io::Error::from)
}

fn sync(directory: &Path) -> std::io::Result<()> {
    fs::File::open(directory)?.sync_all()
}

fn rfc3339(time: SystemTime) -> std::io::Result<String> {
    OffsetDateTime::from(time)
        .format(&Rfc3339)
        .map_err(std::io::Error::other)
}
