# Testing `make install`

A change to `make install` (the `Makefile`'s `install` target, `reviewer-control mcp-install`,
`codex-plugin`, `scripts/herdr-app-mod`) is tested by running it, in a scratch home: never in the
user's, whose Claude Code, Codex and herdr-app it would change.

## In a scratch home

Every home the install writes follows `HOME` once `CODEX_HOME`, `CLAUDE_CONFIG_DIR`,
`HERDR_APP_HOME`, `XDG_DATA_HOME` and `XDG_CONFIG_HOME` are unset. Keep `CARGO_HOME` and
`RUSTUP_HOME` on the real home, or the release build fetches every crate again. Set `HOME`
inside the dev shell, so Nix still finds its own. `S` is an absolute path, since it becomes
`HOME`. The mods install only into an existing herdr-app home, and the keys only into an
existing `keys.json`:

```sh
S=<scratch>; mkdir -p "$S/home/.config/herdr-app" && echo '{"global":{}}' > "$S/home/.config/herdr-app/keys.json"
scripts/dev-shell bash -c '
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$1/home"; unset CODEX_HOME CLAUDE_CONFIG_DIR HERDR_APP_HOME XDG_DATA_HOME XDG_CONFIG_HOME
make install' _ "$S"
```

It still writes the checkout's `bin/` (the binaries, and `bin/codex-plugin` with Codex).

## Without Codex

To stand for a machine without Codex, such as rayon-scout, drop from `PATH`, before
`make install` in the command above, each directory that holds a `codex`:

```sh
PATH="$(printf '%s\n' "$PATH" | tr : "\n" | while read -r d; do [ -x "$d/codex" ] || printf "%s:" "$d"; done)"; export PATH="${PATH%:}"
```

The `\n` after `$PATH` keeps its last directory: `read` drops a line with no newline. A
directory dropped for its `codex` takes its other programs with it, such as an npm global `bin`
or `~/.local/bin` that holds `claude` too: check `command -v claude make jq` after, and put a
directory of links to the missing ones first on `PATH`.

`make install` then ends with exit 0, and says it skipped Codex's MCP registration and its
plugin. The scratch home holds `.claude.json` with `herdr_reviewer`, the Claude Code plugin
(`.claude/plugins`), the link `.local/share/herdr-progressive-reviewer/reviewer`, the two mods
and their keys in `.config/herdr-app`, and no `.codex`. The install writes no
`bin/codex-plugin`: one an earlier install with Codex wrote stays until `make uninstall`.

## Uninstall

`make uninstall` also runs `herdr plugin unlink` when `herdr` is on `PATH`, which may reach the
user's Herdr: in the scratch home, run its steps one by one, such as
`bin/reviewer-control codex-plugin uninstall`, or drop `herdr` from `PATH` too. Without `codex`,
`codex-plugin uninstall` removes `bin/codex-plugin` and leaves Codex's home as it is.
