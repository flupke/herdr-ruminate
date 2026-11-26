# A private herdr-app from the shell (app-sandbox)

herdr-common's `app-sandbox` (`~/src/herdr-common/skills/herdr-mod-vision/app-sandbox`, built
on first use; `--help`) keeps a private herdr-app running across shell commands, on a headless
Weston, with its own home and herdr-core:

- `start <name>` prints its `directory` (as your shell reaches it), `inside` (as its processes
  see it) and `app_home`. `--size 2400x1100` sets the window, at start only.
- `herdr-app <name> ...` and `hc <name> ...` act on it. `shot <name> <png>` takes the pixels.
  `run <name> <program>` runs a program inside. `stop <name>` ends it.

Use it to see the review pane as herdr-app draws it, outside the vision server.

## Sandbox gotchas

- In bubblewrap, the default, its `/tmp` and network are its own. Put a scratch repository
  elsewhere, outside any folder with an `.envrc`: direnv has no allowed one there, and the
  pane's language servers fail.
- Use `--plain` for what needs the network or the host's `/tmp`, such as real Claude Code.
- Never give Claude Code a scratch `XDG_DATA_HOME`, as a private core's terminals have: it
  copies itself into `$XDG_DATA_HOME/claude/versions` and points the user's
  `~/.local/bin/claude` there, even with `DISABLE_AUTOUPDATER` (Claude Code 2.1.318). Run it
  through `env` with the user's data, state and configuration folders.
- `hc run` finds no `env` on the sandbox's `PATH` (herdr-app 0.1.13): give `/usr/bin/env`.
- The sandbox's `hc keys` sends an upper-case letter as its lower-case key (`H` presses `h`):
  shortcuts on upper-case letters (Past rounds) are out of reach.
- The reviewers it starts share the checkout's MCP port: all but the first show a toast that
  the port is in use.

## Open and drive the review pane

- Open the pane as the launcher does, without installing it, in the agent's tab:
  `herdr-app <name> do terminal.open '{"argv": ["<checkout>/target/debug/reviewer",
  "--focused-terminal", "<agent's terminal>", "--herdr-app-home", "<inside>/config/herdr-app"],
  "folder": "<repository>", "place": "right"}'`. In a new tab, `terminal.open` answers its
  terminal without its pane: `herdr-app ls` lists both.
- To install the mods themselves: `HERDR_APP_HOME=<app_home> scripts/herdr-app-mod install`.
  They run this checkout's `bin/reviewer`: `make build` first.
- `hc keys` and `hc read` drive and read the pane. The pane needs 40 columns: `app.zoom_out`
  in a narrow window.
- `pane.split --direction down` and `pane.close` shrink and grow the pane; closing its
  neighbour widens it.
- Whether the pane's first frame survives depends on when herdr-app's shrink and regrow reach
  it: check it on several opens, in fresh tabs, by the first line of `hc read`.
- A click: `app-sandbox run <name> xdotool mousemove <x> <y> click 1`, at the pixels of `shot`
  (the display is X's `:0`).
- The reviewer's settings reach a pane through `XDG_CONFIG_HOME=<host folder>`, with its
  `config.toml` in `<folder>/herdr-progressive-reviewer/` (`[linear]`, `theme`).

## Several review panes

One per tab of a space, as the user runs them (the Explore page's address is recorded per tab),
each with the same space, a tab of its own, and a fresh scratch repository:

```sh
hc <name> run --bg --keep --no-parent --cwd <repository> --label herdr.space=<sp…> --label herdr.tab=<tb…> -- /usr/bin/env BROWSER=<script> <checkout>/bin/reviewer
```

- A `BROWSER` script that appends `$1` and `readlink /proc/$PPID/cwd` to a log in a host
  folder shows which pane opened which page.
- The records under `/tmp/state/herdr-progressive-reviewer/explore-page/`, read with
  `app-sandbox run`, name the review of each port.
- `e` then `s` opens the page on Start; `o` opens it again.
- A terminal without `herdr.space` and `herdr.tab` labels stops the reviewer.

## A second machine, for the launcher in another machine's space

1. Write `via.sh` in the sandbox's `/tmp`. It unsets every `HC_*` and `HERDR*` variable,
   points `HOME`, `XDG_RUNTIME_DIR` and the other XDG folders into `/tmp/remote`, `cd`s there,
   and ends with `exec sh -c "<its last argument>"`.
2. `hc <name> machine add --via /tmp/via.sh --name rs --install`.
3. Link `/tmp/remote/data/herdr-progressive-reviewer/reviewer` to a built `bin/reviewer`.
4. Start a shell in the agent's place there: `hc <name> exec --on rs -- hc run --bg --keep
   --cwd <repository there> --label herdr.space=<sp…> --label herdr.tab=<tb…> -- bash`. It shows
   in the window as `rs/t1`.
5. `do pane.focus '{"terminal": "rs/t1"}'`, then `do mod.open:herdr-progressive-reviewer`
   presses the launcher's key.

## A machine over ssh, for the page key

The sandbox shares its loopback with such a machine, where the forward and the page would want
one port, and it cannot make a namespace of its own (Ubuntu's AppArmor forbids unprivileged user
namespaces). So run the machine on the host:

1. A private `sshd -f <config>` on `127.0.0.1:2222` as the user, with a host key, an
   `AuthorizedKeysFile` and `UsePAM no`. Its `ForceCommand` unsets every `HC_*` and `HERDR*`
   variable, points `HOME` and `XDG_RUNTIME_DIR` at a short scratch folder, never the user's,
   and runs `sh -c "$SSH_ORIGINAL_COMMAND"`.
2. socat bridges a Unix socket in a folder the sandbox shares (`<checkout>/target/...`) to
   that port.
3. An ssh config there (`ProxyCommand socat - UNIX-CONNECT:<socket>`, its own key and
   `UserKnownHostsFile`) reaches it from the sandbox: `hc <name> machine add --via 'ssh -F
   <config> <host>' --name rs --install`.
4. The sandbox's herdr-app has `BROWSER=true`. Run `herdr-app-mod/explore-page/open-page.sh`
   with `env -u BROWSER` (`app-sandbox run`) to see the address its `xdg-open` stand-in logs in
   the sandbox's `/tmp/opened.log`.
