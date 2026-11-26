---
name: install-check
description: Load before changing or testing `make install` or `make uninstall` (reviewer-control mcp-install, codex-plugin, scripts/herdr-app-mod), or fixing the MCP bridge's port conflict.
---

# Testing `make install`

A change to `make install` (the `Makefile`'s `install` target, `reviewer-control mcp-install`,
`codex-plugin`, `scripts/herdr-app-mod`) is tested by running it in a scratch home. Never run
it in the user's home: it would change their Claude Code, Codex and herdr-app.

## In a scratch home

Every home the install writes follows `HOME` once `CODEX_HOME`, `CLAUDE_CONFIG_DIR`,
`HERDR_APP_HOME`, `XDG_DATA_HOME` and `XDG_CONFIG_HOME` are unset.

- Keep `CARGO_HOME` and `RUSTUP_HOME` on the real home, or the release build fetches every
  crate again.
- Set `HOME` inside the dev shell, so Nix still finds its own.
- Make `S` an absolute path, since it becomes `HOME`.
- The mods install only into an existing herdr-app home, and the keys only into an existing
  `keys.json`: create them first.

```sh
S=<scratch>; mkdir -p "$S/home/.config/herdr-app" && echo '{"global":{}}' > "$S/home/.config/herdr-app/keys.json"
scripts/dev-shell bash -c '
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$1/home"; unset CODEX_HOME CLAUDE_CONFIG_DIR HERDR_APP_HOME XDG_DATA_HOME XDG_CONFIG_HOME
make install' _ "$S"
```

It still writes the checkout's `bin/` (the binaries, and `bin/codex-plugin` with Codex).

## Without Codex

To stand for a machine without Codex, such as rayon-scout, drop from `PATH` each directory
that holds a `codex`. Put this before `make install` in the command above:

```sh
OLD_PATH=$PATH; PATH="$(printf '%s\n' "$PATH" | tr : "\n" | while read -r d; do [ -x "$d/codex" ] || printf "%s:" "$d"; done)"; export PATH="${PATH%:}"
```

The `\n` after `$PATH` keeps its last directory: `read` drops a line with no newline. A
directory dropped for its `codex` takes its other programs with it, such as an npm global `bin`
or `~/.local/bin` that holds `claude` too, and then `make install` fails with `claude: command
not found`. So, after the filter and before `make install`, inside the same command, put links
to the programs it lost first on `PATH` (`$1` is the scratch folder there):

```sh
mkdir -p "$1/links"
for p in claude make jq; do command -v "$p" >/dev/null || ln -sf "$(PATH=$OLD_PATH command -v "$p")" "$1/links/$p"; done
export PATH="$1/links:$PATH"
```

A run after a partial install is safe: what is already registered says so.

`make install` then ends with exit 0, and says it skipped Codex's MCP registration and its
plugin. The scratch home holds:

- `.claude.json` with `herdr_reviewer`;
- the Claude Code plugin (`.claude/plugins`);
- the link `.local/share/herdr-progressive-reviewer/reviewer`;
- the two mods and their keys in `.config/herdr-app`;
- no `.codex`.

The install writes no `bin/codex-plugin`. One that an earlier install with Codex wrote stays
until `make uninstall`.

## Uninstall

`make uninstall` also runs `herdr plugin unlink` when `herdr` is on `PATH`, which may reach the
user's Herdr. In the scratch home, run its steps one by one, such as
`bin/reviewer-control codex-plugin uninstall`, or drop `herdr` from `PATH` too. Without
`codex`, `codex-plugin uninstall` removes `bin/codex-plugin` and leaves Codex's home as it is.

## A port conflict

One agent and one reviewer are supported per repository. For a port conflict, set
`HERDR_REVIEWER_MCP_PORT` to the same nonzero port in the reviewer's and the agent's
environment. Or give the bridge the port as its argument: `mcp_servers.herdr_reviewer.args=["59123"]`
for Codex, `"args":["59123"]` in a `--mcp-config` for Claude Code. A busy port is reported; the
reviewer never switches address silently.
