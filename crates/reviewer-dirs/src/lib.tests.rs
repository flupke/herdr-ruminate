use std::path::Path;

use tempfile::TempDir;

use super::*;

/// The folders of an environment whose variables are `set`.
fn dirs(set: &[(&str, &Path)]) -> Dirs {
    Dirs::from_vars(|name| {
        set.iter()
            .find(|(variable, _)| *variable == name)
            .map(|(_, value)| value.as_os_str().to_owned())
    })
}

/// Herdr's state folder for the plugin under `state_home`, holding one file.
fn herdr_state(state_home: &Path) -> PathBuf {
    let herdr = state_home.join("herdr/plugins/herdr.progressive-reviewer");
    fs::create_dir_all(&herdr).unwrap();
    fs::write(herdr.join("settings.json"), "{}").unwrap();
    herdr
}

#[test]
fn herdr_s_plugin_folders_win_when_herdr_gives_them() {
    let home = TempDir::new().unwrap();
    let herdr = herdr_state(&home.path().join(".local/state"));
    let given = home.path().join("given");
    let dirs = dirs(&[
        ("HOME", home.path()),
        ("HERDR_PLUGIN_STATE_DIR", &given),
        ("HERDR_PLUGIN_CONFIG_DIR", Path::new("/herdr/config")),
    ]);

    assert_eq!(dirs.state_root().unwrap(), given);
    assert_eq!(dirs.config_dir(), Some(PathBuf::from("/herdr/config")));
    // Under Herdr, the state Herdr kept stays where it is.
    assert!(herdr.join("settings.json").is_file());
}

#[test]
fn the_state_herdr_kept_moves_to_the_reviewer_s_own_folder_and_a_link_takes_its_place() {
    let home = TempDir::new().unwrap();
    let herdr = herdr_state(&home.path().join(".local/state"));

    let root = dirs(&[("HOME", home.path())]).state_root().unwrap();

    assert_eq!(
        root,
        home.path().join(".local/state/herdr-progressive-reviewer")
    );
    assert_eq!(
        fs::read_to_string(root.join("settings.json")).unwrap(),
        "{}"
    );
    assert_eq!(fs::read_link(&herdr).unwrap(), root);
}

#[test]
fn a_moved_state_stays_put_the_next_time() {
    let home = TempDir::new().unwrap();
    let herdr = herdr_state(&home.path().join(".local/state"));
    let dirs = dirs(&[("HOME", home.path())]);
    let root = dirs.state_root().unwrap();
    fs::write(root.join("settings.json"), "{\"moved\": true}").unwrap();

    assert_eq!(dirs.state_root().unwrap(), root);
    assert_eq!(
        fs::read_to_string(herdr.join("settings.json")).unwrap(),
        "{\"moved\": true}"
    );
}

#[test]
fn an_existing_own_state_is_kept_and_herdr_s_left_alone() {
    let home = TempDir::new().unwrap();
    let state_home = home.path().join(".local/state");
    let herdr = herdr_state(&state_home);
    let own = state_home.join("herdr-progressive-reviewer");
    fs::create_dir_all(&own).unwrap();

    assert_eq!(dirs(&[("HOME", home.path())]).state_root().unwrap(), own);
    assert!(fs::symlink_metadata(&herdr).unwrap().is_dir());
    assert!(!own.join("settings.json").exists());
}

#[test]
fn without_a_state_herdr_kept_the_own_folder_is_used_and_nothing_is_made() {
    let home = TempDir::new().unwrap();
    let root = dirs(&[("HOME", home.path())]).state_root().unwrap();

    assert_eq!(
        root,
        home.path().join(".local/state/herdr-progressive-reviewer")
    );
    assert!(!home.path().join(".local/state").exists());
}

#[test]
fn xdg_base_directories_place_the_folders_and_a_relative_one_is_ignored() {
    let home = TempDir::new().unwrap();
    let state_home = home.path().join("state");
    let herdr = herdr_state(&state_home);
    let config_home = Path::new("relative/config");

    let dirs = dirs(&[
        ("HOME", home.path()),
        ("XDG_STATE_HOME", &state_home),
        ("XDG_CONFIG_HOME", config_home),
    ]);

    assert_eq!(
        dirs.state_root().unwrap(),
        state_home.join("herdr-progressive-reviewer")
    );
    assert!(fs::read_link(herdr).is_ok());
    assert_eq!(
        dirs.config_dir(),
        Some(home.path().join(".config/herdr-progressive-reviewer"))
    );
}

#[test]
fn without_a_home_there_is_no_state_root_nor_config_folder() {
    let dirs = dirs(&[]);

    assert_eq!(
        dirs.state_root().unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(dirs.config_dir(), None);
}

#[test]
fn a_herdr_folder_linked_elsewhere_by_the_user_stays_the_state_root() {
    let home = TempDir::new().unwrap();
    let elsewhere = herdr_state(&home.path().join("disk"));
    let herdr = home
        .path()
        .join(".local/state/herdr/plugins/herdr.progressive-reviewer");
    fs::create_dir_all(herdr.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &herdr).unwrap();

    assert_eq!(dirs(&[("HOME", home.path())]).state_root().unwrap(), herdr);
    assert_eq!(fs::read_link(&herdr).unwrap(), elsewhere);
    assert!(
        !home
            .path()
            .join(".local/state/herdr-progressive-reviewer")
            .exists()
    );
}

#[test]
fn a_state_that_cannot_move_stays_the_state_root_where_herdr_kept_it() {
    use std::os::unix::fs::PermissionsExt;

    let home = TempDir::new().unwrap();
    let state_home = home.path().join(".local/state");
    let herdr = herdr_state(&state_home);
    // Nothing can be made in the state home: the rename to the reviewer's folder fails.
    fs::set_permissions(&state_home, fs::Permissions::from_mode(0o555)).unwrap();

    let root = dirs(&[("HOME", home.path())]).state_root();
    fs::set_permissions(&state_home, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(root.unwrap(), herdr);
    assert!(fs::symlink_metadata(&herdr).unwrap().is_dir());
    assert!(!state_home.join("herdr-progressive-reviewer").exists());
}

#[test]
fn a_new_state_root_is_made_readable_by_the_user_alone() {
    use std::os::unix::fs::PermissionsExt;

    let home = TempDir::new().unwrap();
    let root = dirs(&[("HOME", home.path())]).create_state_root().unwrap();

    assert_eq!(
        root,
        home.path().join(".local/state/herdr-progressive-reviewer")
    );
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn a_link_left_by_the_move_to_a_removed_own_folder_starts_a_new_state_there() {
    let home = TempDir::new().unwrap();
    let state_home = home.path().join(".local/state");
    herdr_state(&state_home);
    let dirs = dirs(&[("HOME", home.path())]);
    let own = dirs.state_root().unwrap();
    // The user removes the reviewer's state to start over; Herdr's path still links to it.
    fs::remove_dir_all(&own).unwrap();

    assert_eq!(dirs.create_state_root().unwrap(), own);
    assert!(own.is_dir());
}
