# Move the state off Herdr's plugin folder, and link it back

The reviewer kept its state in the folder Herdr gives its plugin,
`~/.local/state/herdr/plugins/herdr.progressive-reviewer`, which it learned from
`HERDR_PLUGIN_STATE_DIR`. herdr-app has no plugin folders, and the reviewer runs there in a plain
terminal; the user's reviews, threads, Explore rounds and settings, hundreds of megabytes, are
in Herdr's folder. Herdr stays meanwhile: its tests run the pane on a private Herdr, and the user
may still open the pane under Herdr.

The state root is now the reviewer's own, `$XDG_STATE_HOME/herdr-progressive-reviewer`
(`crates/reviewer-dirs`), unless Herdr gives `HERDR_PLUGIN_STATE_DIR`, which wins, so the tests
keep their private state. The first reviewer that uses its own folder while it does not exist
renames Herdr's folder to it, and leaves a symbolic link to it at the old path: a pane under
Herdr then shares the same state. When the rename fails, as across file systems, or when Herdr's
folder is a link the user made to another place, the reviewer uses Herdr's folder as it is. A
pane that fails to find its host or its repository moves nothing. The backups of ADR 0013 stay
beside the old path, which `migration.json` names. The configuration (`config.toml`, the theme) moves the same way, to
`$XDG_CONFIG_HOME/herdr-progressive-reviewer`, without moving anything: the user had none, and
a `config.toml` left in Herdr's plugin folder is no longer read outside Herdr.

Considered:
- Keep Herdr's folder as the default outside Herdr: nothing moves, but the reviewer would keep
  living in a folder named after a program on its way out, and the later removal of Herdr would
  have to move it then.
- Copy the state in place of moving it: two states that drift apart as soon as a pane under
  Herdr writes, and twice the disk.
- Move without the link: a pane under Herdr would start on an empty state.
