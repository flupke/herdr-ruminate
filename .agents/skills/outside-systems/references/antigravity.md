# Antigravity CLI (`agy`)

What Antigravity CLI (`agy`) was seen to do, with the version seen on.

## CLI and environment

- The CLI binary is `agy`, with `--print` (`-p`) for a single non-interactive prompt, `--prompt-interactive` (`-i`) for an initial interactive prompt, `--resume` (`-r`, `--conversation`) and `--continue` (`-c`) to resume a conversation, `--model`, `--effort` (`low|medium|high|xhigh|max`), `--mode` (`accept-edits|auto|plan`), `--add-dir` and `--settings` (1.5.18).
- `agy` runs on the user's Google account stored in the system keyring (e.g. `GNOME_KEYRING_CONTROL`), so a child `agy` started with a scratch `HOME` can still authenticate and run turns without touching the user's `~/.gemini` (1.5.18).
- Under a parent `agy` session, unset inherited `ANTIGRAVITY_*` variables when starting an isolated child `agy` (1.5.18).
- Global CLI settings live in `~/.gemini/antigravity-cli/settings.json`; exit the TUI with `/exit`, `/quit` or `Ctrl+D Ctrl+D` (1.5.18).

## Status reporting and hosts

- Under herdr-core (`hc`), `agy` reports its state with OSC 7501 as `app=antigravity` (`state=idle`, `state=working`, `state=done`, `state=blocked:kind=permission:app=antigravity:msg=...`), terminated by `ESC \` (1.5.18, hc 0.1.20).
- On an untrusted folder, `agy` opens a folder-trust dialog ("Accessing workspace ... > Yes, I trust this folder") and reports `state=blocked:kind=permission` until Enter accepts it (1.5.18, hc 0.1.20).
- In a fresh `HOME`, interactive `agy` shows a first-run "Terms of Service & Data Use" dialog reporting `state=blocked`, where `Down`, `Right`, `Enter` moves focus from the opt-in checkbox to `[Done]` (1.5.18, hc 0.1.20).
- In `auto` mode, `agy` asks for permission (`Allow calling this tool?`) and reports `state=blocked` before calling an MCP tool unless the user chooses to persist approval to `settings.json` (1.5.18, hc 0.1.20).
- `hc` `terminal.send` types a prompt into `agy`'s input box and submits it; `agy` transitions to `state=working` within herdr-core's window (`started: true`) and reports `state=done` with `turn` incremented once the turn finishes (1.5.18, hc 0.1.20).
- Herdr 0.9.5 has no built-in agent-detection manifest for `agy` (0.9.5).

## Its terminal screen and input line

- The input line is a box between two horizontal rules of `─` drawn in muted grey (`SGR 38;5;8`), starting with `>` in bright blue (`SGR 38;5;12`) (1.5.18, hc 0.1.20).
- The empty input line shows its placeholder (e.g. `Auto mode: edits & safe commands auto-approved (shift+tab to cycle)`) in the rule's muted grey (`SGR 38;5;8`), not `SGR 2` dim; typed text is drawn in the terminal's default foreground (1.5.18, hc 0.1.20).
- Multi-line typed text occupies additional rows inside the two rules before the bottom rule (1.5.18, hc 0.1.20).
- A long bracketed paste collapses in the input box to `[Pasted text #1 +<N> lines]` in the default foreground (1.5.18, hc 0.1.20).
- Earlier prompts in the transcript also start with `>` (bold blue `SGR 1;38;5;4`) under a shorter rule above the current input box, so the last `>` on the screen is the live box (1.5.18, hc 0.1.20).
- While working, `agy` keeps the input box visible with the terminal cursor visible (1.5.18, hc 0.1.20).
- Dialogs and panels (folder trust, `/model`, `/mcp`, `/help`) hide the terminal cursor (`ESC [ ? 25 l`); `/help`, `/model` and `/mcp` keep an empty `>` box visible above the panel while the panel takes keys (1.5.18, hc 0.1.20).

## MCP servers and plugins

- Global MCP servers live in `~/.gemini/config/mcp_config.json` under `mcpServers`, managed by `agy mcp add|remove|list|enable|disable`; `ANTIGRAVITY_APP_DATA_DIR` and `XDG_CONFIG_HOME` do not redirect that path (1.5.18).
- `agy mcp add <name> <command>` writes `{"command": "...", "disabled": false}` without `args` or `type`; `agy mcp list` and `agy` also accept `{"command": "...", "args": []}` (1.5.18).
- Stdio servers use `command`, optional `args`, `env`, and `cwd`; HTTP servers use `url` (or legacy `serverUrl`) for Streamable HTTP (1.5.18).
- `agy` spawns global `~/.gemini/config/mcp_config.json` stdio MCP servers with the CLI's workspace directory as their working directory and inherits the CLI's environment variables, including `HERDR_REVIEWER_MCP_PORT` (1.5.18).
- Workspace plugins live under `.agents/plugins/<name>/` (`plugin.json`, optional `mcp_config.json`, `hooks.json`, `rules/`, `skills/`); `agy` spawns a plugin's stdio MCP server with the plugin folder as its working directory and namespaces its server as `<plugin>_<server>` (1.5.18).
- `agy` connects cleanly to `reviewer-mcp` over stdio and discovers all seven progressive reviewer tools (1.5.18).
