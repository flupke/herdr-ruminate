# herdr-app and herdr-core (`hc`)

herdr-app is the window; herdr-core (`hc`) is its core, which runs the terminals. Herdr, the older multiplexer, is in `herdr.md`. A version in parentheses is the one the fact was seen on.

## The two programs

- herdr-core runs every terminal and agent and keeps them alive across window restarts; its API is JSON-RPC 2.0 over `$HC_TERMINAL_SOCKET`, one message per line, and `hc schema` prints it as OpenRPC (hc 0.1.17–0.1.21).
- herdr-app holds spaces, tabs, panes, layouts, keys, focus and mods; `herdr-app do <command>` runs a command, `herdr-app commands` lists them (herdr-app 0.1.12–0.1.13).
- Focus and zoom are per screen of herdr-app, so herdr-core does not know which pane the user looks at (herdr-app 0.1.12–0.1.13).
- herdr-core keeps no agent session ID; Herdr's own agent hooks exit at once without `HERDR_PANE_ID` and `HERDR_SOCKET_PATH`, so they report nothing under herdr-app (hc 0.1.17–0.1.21).
- herdr-app has no plugin system: no manifest, no `plugin link`, no build hook, no plugin state or config folder, no plugin ID, no `HERDR_*` variables in a pane (herdr-app 0.1.12–0.1.13).
- A terminal sees `HC_TERMINAL_ID`, `HC_TERMINAL_SOCKET` and `HC_SCOPE`, and carries the labels `herdr.space=<id>` and `herdr.tab=<id>` (`hc get $HC_TERMINAL_ID`) (herdr-app 0.1.12–0.1.13).
- A terminal gets the environment of its core's server plus its `HC_*` variables, not that of the `hc run` that starts it: start the program through `env` to give it others (hc 0.1.17–0.1.21).
- `hc --skill` says Codex must run with `--no-daemon` under hc, or its commands run outside hc (hc 0.1.17–0.1.21).
- herdr-core may update itself and restart while it runs; its terminals keep running (seen from 0.1.17 to 0.1.18).
- `hc attach` needs a terminal on its standard input; `script` gives it one (hc 0.1.17–0.1.21).

## Panes, terminals and machines

- `pane.split` takes `direction`, `pane`, `argv`, `mod` and `props`, but no folder (herdr-app 0.1.12–0.1.13).
- `terminal.open` takes `folder` and runs `argv` as given, with no shell; its `name` does not reach the core terminal's `name` (herdr-app 0.1.13).
- Neither `pane.split` nor `terminal.open` copies the caller's labels (herdr-app 0.1.13).
- A split right or down starts its terminal on the machine of the pane it splits, in that pane's folder when given none (herdr-app 0.1.13; `window.rs` of 0.1.15).
- Another machine's spaces, tabs, panes and terminals carry a `<machine>/` prefix in the window (`rs/t1`), which that machine's core and its labels lack (herdr-app 0.1.13).
- `hc machine ls --json` gives each machine's `target`: `user@host:2222` for one added over ssh, the whole command for one added `--via`, which hc runs as an sh line (hc 0.1.17–0.1.21).
- A terminal herdr-app starts that exits nonzero within two seconds shows a toast "Terminal failed to start" with its exit code and last output lines, and its pane goes (herdr-app 0.1.12–0.1.13).
- herdr-core gives each terminal's size, `cols` and `rows`, in `terminals()` and `hc get` (hc 0.1.17–0.1.21).
- When a terminal shrinks, herdr-core drops the alternate screen's rows above the cursor; when it grows, it adds blank rows below: a full-screen program that does not repaint every cell loses the top of its last frame (hc 0.1.17–0.1.21).
- herdr-app resizes a terminal it opens right of another to 120x40 and back within about 100 ms: the program hears one resize, or none if it does not listen for SIGWINCH yet (herdr-app 0.1.12–0.1.13).
- `herdr-app do` acts on the space the user looks at now, which they may switch at any moment, and `space.new` switches their view: never drive the user's window (herdr-app 0.1.13).

## Focus: view.json and the layout document

- herdr-app writes `view.json` in its home on each focus change and for other reasons, such as an agent's attention, as a temporary file renamed over the view (herdr-app 0.1.13).
- `view.json` is herdr-app's own file, with no promise of its format (herdr-app 0.1.13).
- herdr-core keeps the layout as its document `herdr-app` (`doc.get`): spaces, tabs, nested `row`/`column` nodes whose `block` leaves hold `content.terminal` or `content.mod` (herdr-app 0.1.12–0.1.13).
- A `doc` event carries only the document's name and version (herdr-app 0.1.13).
- A terminal's `HERDR_APP_HOME` is the one herdr-core started with, not the window's; a mod reads its window's with `appHome()` (herdr-app 0.1.12–0.1.13).
- Publishing the focus as a label from a mod fails: a copy of a mod lives only while it shows, so only a mod kept on screen could keep it up to date (herdr-app 0.1.12–0.1.13).

## Mods

- Mods are JavaScript and GPUI: no HTML, no web view, no Rust; they reach the window through `run()`, the live data (`focused()`, `agents()`, `terminals()`, `panes()`, `theme()`) and `exec`/`hc()` (herdr-app 0.1.12–0.1.13).
- A key reaches a mod's `nextCommand()` only while a copy of it shows (herdr-app 0.1.12–0.1.13).
- `mod.open:<mod>` opens the mod as the overlay its `mod.json` describes and passes no props; `focused()` in its `init` is the pane under the overlay (herdr-app 0.1.12–0.1.13).
- A copy of a mod lives only while it shows, and its tasks end with it: a promise of a copy whose overlay closed never resumes (herdr-app 0.1.12–0.1.13).
- A popup overlay takes the keyboard while it shows, and a click, Esc or the key again closes it; a `window` overlay does neither (herdr-app 0.1.12–0.1.13).
- `herdr-app check <mod folder>` runs `init` with `props()` null and writes `gpui-kit.d.ts` and `jsconfig.json` into the folder (herdr-app 0.1.12–0.1.13).
- A mod's `exec` returns once its program exits and its standard streams close, and kills it at its timeout: what must outlive it runs in the background with its streams on `/dev/null` or a file; its `$PPID` is herdr-app's process (herdr-app 0.1.12–0.1.13).
- A mod's `exec` with `on: <machine>` runs on that machine, in its home, with its core's environment (herdr-app 0.1.12–0.1.13).
- The window's keyboard focus does not tell whether someone types in a terminal: it misses `hc attach`, another window, and a key typed just before the keyboard left (herdr-app 0.1.12–0.1.13).

## Agents and events

- An agent is a terminal that reports `ESC ] 7501 ; app=<app>:state=<state> BEL`, state `idle`, `working`, `done`, `blocked` or `error`, fields joined by `:`; herdr-core ignores a report joined by `;` (hc 0.1.17–0.1.21).
- `turn` goes up each time the status leaves `working` (hc 0.1.17–0.1.21).
- `ESC ] 7501 ; state=clear BEL` clears a status; a status stays when the child that reported it exits back to the shell (hc 0.1.17–0.1.21).
- A terminal's `foreground` names only the leader of its foreground process group (pid, name, argv, cwd), a moment after the program starts (hc 0.1.18).
- Each foreground change comes as a `foreground` event, though `hc schema`'s list of events leaves it out (hc 0.1.18).
- `events.subscribe` first answers a snapshot of the terminals it matches (by label, terminal or parent), then sends `event` notifications `{type, terminal, seq, data}` (hc 0.1.17–0.1.21).
- A program that exits first clears its status (`status: null`), then its terminal `finished` (hc 0.1.17–0.1.21).
- A `labels` event gives the whole label set; a terminal that enters a subscription by its labels comes with the whole terminal in `record`, one that leaves it without (hc 0.1.17–0.1.21).
- A kept terminal whose program died comes back as `lost`, then `restarted` with its whole record (hc 0.1.17–0.1.21).
- An unfiltered subscription also gets `doc` events, which name no terminal, each time herdr-app saves its layout (herdr-app 0.1.13).
- Beside events come `heartbeat` every 15 s, and `missed` for a subscriber that fell behind (hc 0.1.17–0.1.21).
- `events.subscribe` on one terminal refuses an unknown or removed one with `not_found`; for a finished one it answers a snapshot with `state: finished`, then nothing until `removed` (hc 0.1.17–0.1.21).

## How hc types a prompt (`terminal.send`)

- When hc types a send it holds: once nobody pressed a key in the terminal for its quiet pause of about 3 s (3.1 s after the last key), however long they type (23 s seen on 0.1.18); with the draft hold on, also not before the typing there is sent or erased, at most 60 s after the last key (hc 0.1.21).
- A terminal nobody typed in takes the send at once; one that just started takes it once its output settled, about 1 s (hc 0.1.17–0.1.21).
- Typing means a person's keys through an attached window or `hc attach`; `hc keys` does not count (hc 0.1.17–0.1.21).
- The send pastes the text and presses Enter, and holds the keys pressed while it types (hc 0.1.17–0.1.21).
- The draft hold (hc 0.1.20 and later) counts the characters typed less the Backspaces, until Enter, Ctrl-C or Ctrl-U; control keys, Alt+key, Delete and Esc count nothing (hc 0.1.20).
- So text erased with Claude Code's Esc Esc or Ctrl-W still holds a send while the input line is empty: seen for 134 s and 92 s (hc 0.1.20).
- `draft_wait = 0` in `~/.config/herdr-core/config.toml` turns the draft hold off; the user runs it off for every sender (hc 0.1.21).
- `HC_DRAFT_MS` in the core's environment sets the draft hold's limit (hc 0.1.20).
- With the draft hold off, a send types into a half-written line 3.1 s after the person's last key (hc 0.1.21).
- Ctrl+U beside a held send ends the draft hold: the send typed 0.2 s after the key (hc 0.1.21).
- A send held behind a person's typing types 0.2 s after their Enter (hc 0.1.20).
- A client that hangs up cancels a send hc still holds: nothing is typed (hc 0.1.20 and later; checked on 0.1.21, 1 s after the key).
- hc answers a send once the program went `working`, or a few seconds after it typed; its `started` says whether it went `working` (null when it reports no status) (hc 0.1.21).
- So hanging up a send hc did not answer yet, then sending it again, can type it twice (hc 0.1.21).
- `terminal.send` has no option to skip the draft hold for a caller that read the line empty (hc 0.1.21).
- With `wait`, a send waits for the end of the turn it started; with `if`, it types only on that exact status (hc 0.1.17–0.1.21).
- A send refuses a `blocked` program unless `force`, also one that blocked while the send waited: hc checks again when it types (hc 0.1.17–0.1.21).
- Refusal kinds: `blocked`, `finished`, `not_found`, `unavailable`, `machine_unknown`, `status_mismatch`, `invalid_params`, `config`; each typed nothing (hc 0.1.20).
- `unavailable` refuses a terminal younger than the core's startup age limit whose output does not settle within 5 s, as a person typing there keeps it (hc 0.1.20).
- `hc send` from one terminal to another wraps the text in `<hc from="...">` tags, unless `notes = false` in `~/.config/herdr-core/config.toml`; an API request with no `caller` types the text as it is (hc 0.1.20).
- `terminal.keys` with no key presses nothing and answers at once when nobody typed for the quiet pause, else when the pause ends; it does not wait for the draft hold (hc 0.1.20 and 0.1.21).
- hc holds a press of no key while it types a send (hc 0.1.21).
- With the draft hold on, a line read empty and an answered press of no key do not mean the send goes: the send can wait 60 s more (hc 0.1.21).
- `hc keys` refuses an empty key list; only the API sends no key (hc 0.1.21).

## Reading a terminal's screen

- `terminal.read` gives no styles, so it reads a dim placeholder (Claude Code's first one, `Try "edit <filepath> to…"`) as typed text (hc 0.1.21; Claude Code 2.1.329, Codex 0.166).
- `terminal.attach` with `observe: true` watches a terminal without typing or resizing, on a framed connection; its credit window is 256 KiB (hc 0.1.21).
- A new watcher gets a whole paint at the terminal's own size, whatever size it names: CAN and `ESC [!p`, tab stops, then each row ended by CR LF with SGR styles (hc 0.1.21).
- The paint comes in 16 KiB frames but its last (185 KiB in 12 frames within 1 ms), so a frame shorter than 16 KiB ends it; a print right after the paint, even 6 ms later, is a frame of its own, and a watcher that waits for a gap takes it for paint (hc 0.1.21).
