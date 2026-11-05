# herdr-app and herdr-core

How herdr-app and herdr-core behave where the reviewer relies on Herdr, and the reviewer's
backend for herdr-core. Checked against herdr-app 0.1.12 and 0.1.13, herdr-core (`hc`) 0.1.17 and
0.1.18, and Claude Code 2.1.318. On this page, Herdr
means `herdr`, the older terminal multiplexer whose plugin the reviewer is; herdr-app calls
itself Herdr too, and labels its terminals `herdr.*`.

## Two programs, and where each API lives

- **herdr-core** (`hc`) runs every terminal and agent, and keeps them alive across window
  restarts. Its API is JSON-RPC 2.0 over `$HC_TERMINAL_SOCKET`, one message per line;
  `hc schema` prints it as OpenRPC. Terminals, their labels, their agent status, sends and
  events live here.
- **herdr-app** is the window: spaces, tabs, panes, layouts, keys, focus, and mods (GPUI
  panels in JavaScript). `herdr-app do <command>` runs a command; `herdr-app commands`
  lists them. Focus and zoom are per screen, so the core does not know which pane the user
  looks at (`~/.config/herdr-app/AGENTS.md`, "The model").
- herdr-app has no plugin system like Herdr's: no manifest, no `plugin link`, no build
  hook, no plugin state or config directory, no plugin ID, no `HERDR_*` variables in a pane.
  A terminal in herdr-app sees `HC_TERMINAL_ID`, `HC_TERMINAL_SOCKET` and `HC_SCOPE`, and
  carries the labels `herdr.space=<id>` and `herdr.tab=<id>` (`hc get $HC_TERMINAL_ID`).
- A mod reaches the window through `run()`, the live data (`focused()`, `agents()`,
  `terminals()`, `theme()`), and `exec`/`hc()` for programs. Mods are JavaScript and GPUI:
  no HTML, no web view, no Rust (`~/.config/herdr-app/AGENTS.md`, "What you can and
  can't do"). herdr-bowtie's mod `~/.config/herdr-app/mods/bowtie/main.js` starts its own
  program with `hc run`, passing `focused().tab` in its environment, and shows it in an
  overlay.
- `pane.split` takes `direction`, `pane`, `argv`, `mod` and `props`, but no folder;
  `terminal.open` takes `folder`, and runs `argv` as given, with no shell: the terminal's
  `argv` is the program's. Its `name` does not reach the core terminal's `name`, and neither
  command sets labels of the caller's (herdr-app 0.1.13).
- A key reaches a mod's `nextCommand()` only while a copy of it shows. `mod.open:<mod>` opens
  the mod as the overlay its `mod.json` describes, so a mod with no panel can still act on a
  key: its `init` reads `focused()`, acts, and closes the overlay at once. `focused()` there is
  the pane under the overlay. `herdr-app check` runs `init` with `props()` null, and writes
  `gpui-kit.d.ts` and `jsconfig.json` into the mod's folder.
- A terminal herdr-app starts that exits nonzero within two seconds shows a toast "Terminal
  failed to start" with its exit code and, by herdr-app's AGENTS.md, its last output lines;
  its pane goes.
- herdr-core gives each terminal its size, `cols` and `rows`, in `terminals()` and
  `hc get`.
- A terminal's screen is herdr-core's. When the terminal shrinks, it drops the alternate
  screen's rows above the cursor, and when it grows, it adds blank rows below: a full-screen
  program that does not paint every cell again loses the top of its last frame. herdr-app
  resizes a terminal it opens right of another to 120x40 and back within about 100 ms, so the
  program may hear one resize to the size it drew at, or none if it is still starting and
  crossterm does not listen for SIGWINCH yet. The review pane copes with both
  (`crates/reviewer/src/runtime.rs`).
- herdr-app writes `~/.config/herdr-app/view.json`: the selected space (`active`), the
  selected tab of each space (`spaces`), and the focused pane of each tab (`tabs.<tab>.focused`),
  by herdr-app's pane IDs, on each focus change and for other reasons, such as an agent's
  attention. It writes a temporary file and renames it over the view. herdr-core keeps
  herdr-app's layout as its document `herdr-app` (`doc.get`): spaces, tabs, and nested
  `row`/`column` nodes whose `block` leaves hold `content.terminal` or `content.mod`, by the same
  pane IDs. A `doc` event carries only the document's name and version (herdr-app 0.1.13).
- A terminal's environment is herdr-core's, plus its `HC_*` variables: its `HERDR_APP_HOME`
  is the one herdr-core started with, not the window's. A mod reads its window's with
  `appHome()`.
- The `herdr-gpui-browser` skill is for Herdr GPUI, a different app; on Linux it opens
  pages in the system browser.

## Agents

- An agent is a terminal that reports its status with OSC 7501: `idle`, `working`, `done`,
  `blocked`, `error`, with an `app`. A report is `ESC ] 7501 ; app=<app>:state=<state> BEL`,
  its fields joined by `:`; herdr-core ignores one joined by `;`. Claude Code reports
  `claude-code` by itself, with no hook. `herdr.d.ts` gives `codex` as an example `app`;
  whether Codex reports a status under hc is not verified. `turn` goes up each time the
  status leaves `working`.
- The core keeps no agent session ID. Herdr learned it from its own hooks,
  `~/.claude/hooks/herdr-agent-state.sh` and `~/.codex/herdr-agent-state.sh`, which exit at
  once without `HERDR_PANE_ID` and `HERDR_SOCKET_PATH`: in herdr-app the reviewer's Claude
  Code hook reports Claude Code's, as a label; nothing reports Codex's (below).
- `terminal.send` pastes and presses Enter once nobody has typed for about 3 seconds, and
  holds the keys pressed while it types (`hc schema`). Typed means a person's keys, through
  an attached window or `hc attach`: `hc keys` does not count. A terminal nobody typed in
  takes the text at once; while a person types, the send waits until about 3 s after their
  last key, however long they type (23 s seen with 0.1.18). Then it answers `started`: whether
  the program went `working` within a few seconds (null when it reports no status). With
  `wait`, it then waits for the end of that turn. It types only on an exact status with
  `if`, and refuses a `blocked` program unless `force`, also one that blocked while the send
  waited: it checks again when it types. The reviewer counts each refusal `hc schema` lists
  (`blocked`, `finished`, `not_found`, `unavailable`, `machine_unknown`, `status_mismatch`,
  `invalid_params`, `config`) as one that typed nothing: the opt-in tests show it for
  `blocked`, `unavailable` says "nothing was typed", and the others check the request or the
  terminal. `unavailable` refuses a terminal younger than herdr-core's
  startup age limit whose output does not settle within 5 s, as a person typing there keeps
  it. `hc` wraps what one terminal sends another in `<hc from="...">` tags unless `notes =
  false` in `~/.config/herdr-core/config.toml`; a request with no `caller` (the API, not the
  `hc` command) types the text as it is, even when it comes from a terminal of the core.
- Claude Code reports `blocked` while a question of its `AskUserQuestion` tool waits on the
  user (2.1.318).
- Claude Code 2.1.318 can move a conversation into a background session while the `claude`
  in the terminal stays in its foreground and takes the user's keys (seen on a herdr-bowtie
  worker; what triggers it is not known). That `claude` clears its status and starts
  `claude daemon run --origin transient`, which runs the session (`--fork-session --resume`,
  a new session ID; the old transcript ends with a `continued-in` record) on a pty of its own.
  The session's OSC 7501 goes to that pty, so the terminal has no status for as long as it
  runs. The session inherits `HC_TERMINAL_ID` and leads its pty's foreground, so the
  reviewer's hook names it in the terminal's session label. Its hook record's process is not
  the terminal's foreground leader, so `AgentHooks::hooked` does not match: the reviewer's
  prompts go through herdr-core's send unconfirmed, without the guard against text left in
  the input box. Another session the daemon starts at the same moment can win the label.
  `ESC ] 7501 ; state=clear BEL` clears a status (write it to the agent's `/proc/<pid>/fd/0`),
  which reproduces the missing status in a private core, not the daemon. A status stays when
  the child that reported it exits back to the shell.
- A terminal takes the environment of its core's server, not that of the `hc run` that starts
  it: start the program through `env` to give it other variables.
- `/proc/<pid>/environ` holds the variables a process started with: a shell's lists none it
  exported since, as from `~/.zshrc`, while a program the shell starts has them.
- `events.subscribe` answers a snapshot of the terminals it matches (by label, terminal or
  parent), then sends each fact change as an `event` notification `{type, terminal, seq,
  data}`: `created` (the whole terminal), `status`, `turn`, `foreground`, `labels`,
  `finished`, `lost`... A label change is an event; Herdr's `pane.report_agent_session` raised
  none. A program that exits first clears its status (`status` with `status: null`), then
  its terminal `finished`. A `labels` event gives the whole label set. A terminal that enters
  a subscription by its labels comes as a `labels` event with the whole terminal in `record`;
  one that leaves it, without. A kept terminal whose program died comes back as `lost`, then
  `restarted` with its whole record. An unfiltered subscription also gets `doc` events, which
  name no terminal, each time herdr-app saves its layout. Beside them come `heartbeat` every
  15 s, and `missed` for a subscriber that fell behind.
- `events.subscribe` on one terminal refuses one herdr-core does not know, or removed, with
  `not_found`; for a finished one, it answers a snapshot with `state: finished`, then nothing
  until `removed`.
- A refusal is `{error: {code, message, data: {kind, hint}}}`: `kind` is `not_found`,
  `blocked`, `finished`...
- A terminal's `foreground` names only the leader of its foreground process group (pid,
  name, argv, cwd); Herdr's `pane.process_info` listed the processes. herdr-core names it a
  moment after the program starts: a terminal just started may have none yet. Each change
  comes as a `foreground` event, with the new leader as its data, though `hc schema`'s list of
  events leaves it out (0.1.18). A Claude Code started in a shell takes the foreground, sets
  its session label and reports `idle` within about a second.
- `hc --skill` says Codex must run with `--no-daemon` under hc, or its commands run outside
  hc. The reviewer hears Codex through that shared daemon (ADR 0009), so a Codex agent
  started the way hc says has no daemon to join.

## Herdr's APIs, and what herdr-app has in their place

`crates/herdr-client/src/protocol.rs` (`mod method`) and `rg 'HERDR_|herdr' crates tests
scripts nix Makefile` list the reviewer's uses.

| Herdr | In herdr-app and herdr-core |
|---|---|
| Plugin manifest, `herdr plugin link`, actions, the prefix+d binding (`herdr-plugin.toml`, `Makefile`, `README.md`, `scripts/package.sh`) | A mod, and a binding in `~/.config/herdr-app/keys.json`; no install command |
| `HERDR_PLUGIN_STATE_DIR`, `HERDR_PLUGIN_CONFIG_DIR`, `HERDR_PLUGIN_ID` | Nothing: the reviewer's own folders (`crates/reviewer-dirs`) |
| `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID` | The `herdr.space` and `herdr.tab` labels of the review pane's terminal |
| `HERDR_PLUGIN_CONTEXT_JSON` (focused pane, its folder, tab) | A mod's `focused()`; nothing reaches a program unless the mod passes it |
| `session.snapshot`, `agent.list`, `agent.get` | `terminal.list` and `terminal.get`, filtered by label, with OSC 7501 status and `app` |
| The agent's session, `pane.report_agent_session` | The reviewer's `herdr-reviewer.session` label (ADR 0016) |
| `pane.process_info` | `foreground`, the group leader only |
| `agent.prompt` | `terminal.send` |
| `agent.read` (tests only) | `terminal.read` |
| `events.subscribe`: `pane.focused` | Nothing in the core; a mod's `focused()`, and `view.json` with the `herdr-app` document, which the review pane follows |
| `events.subscribe`: `pane.agent_detected`, `pane.agent_status_changed` | `events.subscribe` status events |
| `plugin.pane.open`, `plugin.pane.focus`, `plugin.pane.close`, `agent.focus` | `pane.split` or `terminal.open`, `pane.focus`, `pane.close`, `agent.focus`, through `herdr-app do` or a mod's `run()` |
| `pane.get`, `pane.layout`, `pane.resize` | `herdr-app ls` and a mod's `panes()`, with each pane's tab; no pane size |
| `HERDR_PANE_ID`, `HERDR_SOCKET_PATH` in an agent's hooks | `HC_TERMINAL_ID`, `HC_TERMINAL_SOCKET` |
| An isolated `herdr server` in tests, the pinned Herdr in `nix/flake.nix`, agent-detection rules | A private core: its own `HC_SESSION` and scratch XDG paths (`hc --skill`); status comes from OSC 7501, not detection rules |
| The vision MCP's workspace in the user's Herdr | `herdr-app do space.new`, `pane.split`, `space.close`; `hc read`, `hc keys`, `hc send`; `herdr-app shot` for pixels |

The MCP bridge and the Explore page's browser use no Herdr API.

## The reviewer's hc backend

`crates/hc-client` talks to herdr-core behind `herdr-client`'s `AgentPort` and `AgentHost`.
The review pane takes Herdr when `HERDR_SOCKET_PATH` is set, else herdr-core when
`HC_TERMINAL_SOCKET` is (`crates/reviewer/src/runtime/host.rs`): a Herdr started from a
herdr-app terminal may pass that terminal's `HC_*` variables on to its panes (not verified),
and its agents are Herdr's.
[ADR 0014](../adr/0014-talk-to-herdr-core-behind-the-host-traits-beside-herdr.md) says why
both stay.

- Under herdr-core the pane first runs its program again with the environment of the
  foreground process of the terminal the mod names (`runtime/agent_environment.rs`), as it had
  the user's shell's under Herdr: herdr-core gives it the core server's, which lacks what the
  user's shell exports, such as `TYPESAFE_API_KEY` or `EDITOR`. The agent's values win; the
  pane keeps its own `HERDR_*` (but takes the agent's `HERDR_REVIEWER_*` settings, such as
  `HERDR_REVIEWER_MCP_PORT`), `HC_*`, `PWD`, `OLDPWD`, `_`; its `HOME`, `XDG_STATE_HOME`
  and `XDG_CONFIG_HOME`, so its state and `config.toml` stay where every pane keeps them; and
  its temporary folders, since a nix-shell deletes its own when it exits, while the pane may
  outlive it. The rest of the agent's dev shell comes along: `IN_NIX_SHELL`, `DIRENV_*`. It
  runs again once at most (`HERDR_REVIEWER_ADOPTED_AGENT_ENVIRONMENT`). A plain shell's
  process gives only what it started with (above), and one it cannot read leaves the pane's
  own.
- Under herdr-core the pane's first target is the terminal the mod names with
  `--focused-terminal`; it follows the focus of the window whose home the mod names with
  `--herdr-app-home`, else `~/.config/herdr-app`, through `view.json` and the layout
  (`crates/herdr-app-focus`, [ADR 0017](../adr/0017-follow-herdr-app-s-focus-through-its-view-file.md)).
  It tells only the focus of this computer's spaces: another machine's ids carry a `<machine>/`
  prefix that its layout lacks. A home it cannot watch, or a layout it cannot read, leaves the
  focus unknown, not the agents' events. The watch ignores the reads of `view.json`: notify's
  inotify backend reports each open, the pane's own included.
- An agent is a terminal that reports a status, or that runs Claude Code in its foreground
  on a session its session label names (above: a background session), with status unknown
  (`crates/hc-client/src/terminal.rs`, `running`). Snapshots and events use that one rule, so
  target resolution and arrivals agree. A Claude Code that has not started its session yet is
  no agent; one without the reviewer's plugin, which sets no label, is none while its session
  runs in the background. A label left by a Claude Code killed before its `SessionEnd`, or by a
  background session that outlives the `claude` that started it, makes the next `claude` in
  that shell an agent from its start, on that stale session until its own starts.
- The agent's session is a label of its terminal, which the reviewer's Claude Code hook sets,
  and the agents' hooks reach the reviewers of a core as those of a Herdr server
  ([ADR 0016](../adr/0016-report-the-agent-s-session-as-a-label-of-its-herdr-core-terminal.md)).
  A subscription that starts again, after its stream dropped, tells each agent it holds as a
  session change, since its session may have changed, or it may have gone, meanwhile.
  The Bash tool of Claude Code 2.1.318 runs its commands in a session of their own, with no
  terminal (`/proc/<pid>/stat`: tty 0, tpgid -1), while its MCP servers run in its own process
  group, in the terminal's foreground, with its variables: the hook tells a `claude` either
  runs from the agent by whether it leads its terminal's foreground group.
- Prompts go through `terminal.send`. To an agent that runs the reviewer's hooks, the reviewer
  arms the hook before it sends, and the send may wait for seconds while the user types there.
  So under herdr-core the hook lets a prompt without the reviewer's text go on meanwhile
  (`AgentPort::types_prompts_whole`): herdr-core types the reviewer's text whole, never among
  the user's keys. Under Herdr, which types at once, it still blocks any other prompt
  ([Review threads](threads.md#delivery)). A refused send reached nothing: the pane says so,
  and Retry sends again. Two waits stay as they were: run-ahead's `/resume` (`claude-fork`,
  `codex-fork`) still blocks every prompt while its send waits, and the one courier that
  sends all prompts holds every agent's while a send waits for the user typing in one.
- Nothing names a Codex agent's session under herdr-core: Codex's hook runs in its daemon,
  which cannot tell the terminal, so the Codex resume, which reports a session only when the
  host reports one, reports none there.
- The pane-opening action (`reviewer-control`, `HerdrReader`, `HerdrWriter`) stays Herdr's;
  under herdr-app, `herdr-app-mod/` opens the pane. It tells a review pane by its terminal's
  argv, a program ending in `/bin/reviewer` and `--focused-terminal`: keep the two in step with
  `host.rs`. No check parses the mod; `herdr-app check herdr-app-mod` does. Never drive it in the
  user's window: `herdr-app do` acts on the space the user looks at now, which they may switch
  at any moment, and `space.new` switches their view. A copy of a mod lives only while it shows,
  and the launcher's closes at once.

The tests of the backend against a real herdr-core are opt-in (Checks, Opt-in tests), on a
private core of `crates/hc-test-support`. Its `Person` types as the user does in a window:
`hc attach` wants a terminal, which `script` gives it. A private core needs its own
`HC_SESSION`, scratch `XDG_*_HOME` folders, and an `XDG_RUNTIME_DIR` short
enough for its socket: herdr-core refuses a socket path over 107 bytes, which a scratch folder
under `$TMPDIR` can exceed; the tests use `/tmp`. herdr-core may update itself and restart
while it runs (0.1.17 to 0.1.18 during one session): its terminals keep running.

## A private herdr-app

`scripts/herdr-app-private start [WIDTHxHEIGHT]` starts a herdr-app off the user's screen, with
its own home and its own herdr-core, and prints an env file; `. <env file>` points `herdr-app`
and `hc` at them, and `scripts/herdr-app-private stop <env file>` ends both and removes the
folder. Use it to see the review pane as herdr-app draws it: `hc read` gives the core's screen,
`herdr-app shot` the pixels.

- Its display is a headless sway (`WLR_RENDERER=gles2`: Vulkan finds no DRM device there);
  `swaymsg output HEADLESS-1 resolution 2400x1100` resizes the window. The launcher splits
  right of a terminal with at least two and a half times as many columns as rows, as the
  default 1600x1000 window gives. Xvfb works too: `herdr-app` with `DISPLAY` on it and
  `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json` (software Vulkan).
- `HERDR_APP_HOME` must be a short folder under `/tmp`: the window's control socket lives there,
  and a longer path leaves `herdr-app do`, `ls` and `shot` with no window to reach.
- `scripts/herdr-app-mod install` with the env sourced installs the launcher there, running
  this checkout's `bin/reviewer` (`make build`). `herdr-app do mod.open --name
  herdr-progressive-reviewer` is what the launcher's key runs; `herdr-app do tab.new --folder
  <checkout>` first gives it a terminal to open beside. `hc keys` and `hc read` drive and read
  the review pane. The pane needs 40 columns: `app.zoom_out` in a narrow window.
- `pane.split --direction down` and `pane.close` shrink and grow the pane; closing its
  neighbour widens it.
- The window opens on its welcome overlay, over the middle of the window. `overlay.close {id:
  "frame-welcome"}` may answer ok and leave it: read what it covers with `hc read`.
- Whether the pane's first frame survives depends on when herdr-app's shrink and regrow reach
  it (above): check it on several opens, in fresh tabs, by the first line of `hc read`.
- The reviewers it starts share the checkout's MCP port, so all but the first show a toast
  that the port is in use. Its `XDG_*` folders are its own: direnv has no allowed `.envrc`,
  and the language servers fail.

Never give Claude Code a scratch `XDG_DATA_HOME`, as a private core's terminals have: 2.1.318
copies itself into `$XDG_DATA_HOME/claude/versions` and points the owner's `~/.local/bin/claude`
there, even with `DISABLE_AUTOUPDATER`, which leaves the owner a `claude` that is gone with the
scratch folder. Run it there through `env` with the owner's data, state and configuration
folders, as the contract check does (below).

## The contract check

`crates/reviewer/tests/claude_code_under_hc.rs` checks that real Claude Code takes the
reviewer's prompts through herdr-core as the reviewer expects: its hook reports a sent prompt
as typed, a long one too, and the text left in its input box joined to it; it runs a prompt
the user submits while the send waits for them; it reports `working` after a send, and
`blocked` while a question waits on its user, when herdr-core types nothing. It runs the
owner's `claude` on the owner's login and configuration, with the reviewer's plugin set to
this build's `reviewer-control` by `--settings`: the plugin must be installed (`make install`).
Unlike the run-ahead check ([Run-ahead](run-ahead.md#the-contract-check)), which never reads
`~/.claude`, it needs no token of `claude setup-token`: the owner chose their own login. It
leaves in the owner's configuration the session of each run, and the trust of its folder,
`~/.cache/herdr-reviewer/hc-contract-check`, recorded once; it runs their other plugins,
hooks and MCP servers, on a private core. About 30 s and five Sonnet turns. Run it after an
upgrade of Claude Code or herdr-core, or a change to the prompts' delivery
(`review-thread-service`'s `delivery.rs`, `hc-client`'s `send`, `agent-hooks`'s expectations):

```sh
scripts/dev-shell cargo nextest run -p reviewer --test claude_code_under_hc --run-ignored only
```

It passed with Claude Code 2.1.318 and herdr-core 0.1.18.

## The review pane outside Herdr

The pane reads its space and tab from its terminal's `herdr.space` and `herdr.tab` labels,
and keeps its state in its own folder ([Review state](review-state.md)), so it runs in any terminal of herdr-app
that herdr-core labels: `hc run --label herdr.space=<id> --label herdr.tab=<id> --
bin/reviewer`. A terminal without the labels stops it with `terminal <id> has no herdr.space
and herdr.tab labels`.
