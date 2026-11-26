# Codex

What Codex and its app-server daemon were seen to do, with the version seen on.

## The daemon and its socket

- By default every Codex TUI of a Codex home runs its sessions ("threads") on one shared daemon (0.162.1).
- The daemon's control socket is `$CODEX_HOME/app-server-control/app-server-control.sock`, a link to `/tmp/codex-daemon-$UID/<digest>` (0.162.1).
- The socket carries JSON-RPC in WebSocket text frames over the Unix socket, with no authentication beyond the file's mode; `codex app-server proxy` only relays bytes (0.162.1).
- The handshake is `initialize`, then `initialized`; on stdio, `codex app-server` takes one JSON-RPC message per line, without `jsonrpc` (0.162.1).
- `codex app-server generate-json-schema --out <dir> --experimental` writes the protocol's types (0.162.1).
- A TUI on `--remote unix://` uses the default socket, `--remote unix://<path>` another; such a TUI never starts a daemon (0.162.1).
- A TUI on `--no-daemon` runs its own app server: no socket, and no other client can see, join or fork its sessions (0.162.1).
- A managed daemon (the one a plain TUI starts) runs a `daemon-updater` that downloads about 850 MB into `$CODEX_HOME/packages` and restarts the daemon on a new release (0.162.1, updated itself to 0.164.0).
- Stop a managed daemon with `codex app-server daemon stop`, then kill its `pid-update-loop` process (0.162.1).
- An unmanaged `codex app-server --listen unix://` binds the same default socket, which a plain TUI takes for its daemon, and never updates itself (0.162.1).
- After a daemon restart, a TUI shows "Reconnecting to server…" and rejoins its session within a few seconds; any other client must connect and join again itself (0.162.1).
- The sessions of a TUI on `--remote` run in the server's working directory, not the pane's (0.162.1).
- MCP servers run in the daemon, with its environment: a variable an MCP server needs has to be in the daemon's (0.162.1).

## What a client hears

- Every connection gets `thread/started` (a fork's carries `forkedFromId`), `thread/status/changed` and `thread/closed`, for every session (0.162.1).
- Turn and item events go only to the session's clients: `turn/started`, `item/completed` (a `userMessage` with the exact prompt, `mcpToolCall`, `agentMessage`) and `turn/completed` (0.162.1).
- `thread/resume {threadId}` on a loaded session makes the caller one of its clients without disturbing the TUI; on a session not loaded, it loads it from its rollout first, starting its MCP servers (0.162.1).
- A fresh TUI's session cannot be joined before its first prompt ("no rollout found"); no event names the client that resumed a session (0.162.1).
- On every join, the session's clients get its goal snapshot: `thread/goal/cleared`, or `thread/goal/updated` with no `turnId`; it follows the answer to the join at once, and comes as soon as the TUI took a `/resume` (0.162.1).
- A turn that changes the goal sends `thread/goal/updated` with its `turnId`; a TUI's rejoin after a daemon restart sends a snapshot too; a connection that joined nothing got the snapshots as well (0.162.1).
- `thread/loaded/list` lists loaded sessions, not who is on them, and keeps idle sessions for minutes after their last client left (0.162.1).
- A session names itself in each MCP call: `_meta.threadId`, `_meta.sessionId` and `x-codex-turn-metadata` (0.162.1).
- A `turn/start` from another client shows live in the TUI; `thread/inject_items` adds history the model sees and the TUI does not show (0.162.1).

## Hooks

- `SessionStart` with source `resume` (after a `SessionEnd`) fires only when the next turn starts, so Herdr's session for the pane changes only then (0.162.1, Herdr 0.9.5).
- Under `--no-daemon`, hooks run in the TUI with the pane's environment, from the first prompt (0.162.1).
- A scratch home's hooks run once trusted: write each `currentHash` of `hooks/list` as `[hooks.state."<key>"] trusted_hash` in its `config.toml`; auto mode refuses `--dangerously-bypass-hook-trust` (0.162.1).
- A session keeps the hooks it was loaded with: a hook trusted, untrusted or installed later changes nothing for it until the daemon loads it again; `hooks/list` says what a new session would run (0.162.1).
- `UserPromptSubmit` carries the exact text, with no paste wrapper; a bare `/resume <id>` fires no hook (0.162.1).
- Text left in the input box and a typed `/resume <id>` run as one prompt, `<text>/resume <id>`, on the session the TUI was on; blocked, the agent stays there (0.162.1).
- A blocked prompt runs no turn: clients get `turn/started`, `hook/completed` with `status: "blocked"`, then `turn/completed`, with no `userMessage`; the TUI shows "Blocked by hook" with the reason, keeps the prompt in its history and empties the input box (0.162.1).
- `PreToolUse` fires before every tool a session calls, nested `exec` tools and MCP tools included; `exec` itself fires none (0.162.1).
- `PreToolUse` tool names: `Bash` (with `tool_input.command`), `apply_patch`, `view_image`, `get_goal`, `create_goal`, `read_mcp_resource`, `mcp__<server>__<tool>`, and namespaced tools joined to their namespace (`clockcurr_time`, `clocksleep`, `collaborationspawn_agent`) (0.162.1).
- Exit 2 with the reason on standard error, or a `permissionDecision` of `deny`, stops the call; `create_goal` starts goal continuation turns (0.162.1).
- A fork's `SessionStart` (source `fork`) runs Herdr's Codex hook for the starting pane, and Herdr's session for the pane stays the agent's (0.162.1, Herdr 0.9.5).
- Herdr hears of a resumed Codex session only from its own hook at the next turn, so until then it reports the agent on the session it left (0.162.1, Herdr 0.9.5).
- Herdr's Codex hook reports a session only with `HERDR_ENV=1`, `HERDR_SOCKET_PATH` and `HERDR_PANE_ID` in the daemon's environment, as a daemon started from a pane has them (0.162.1, Herdr 0.9.5).
- Since the daemon runs every hook, no hook can tell which terminal a session runs in: nothing names a Codex agent's session under herdr-core (version not recorded).

## Plugins

- `codex plugin marketplace add <dir>` records a local marketplace by its path, and refuses one of the same name from another path until `codex plugin marketplace remove` (0.162.1).
- `codex plugin add` copies the plugin into `$CODEX_HOME/plugins/cache/<marketplace>/<plugin>/<version>`, again on each run, and enables it in `config.toml` (0.162.1).
- `codex plugin add` leaves the hooks untrusted; `codex plugin remove` deletes the copy but leaves `hooks.state`; a hook runs with `PLUGIN_ROOT` and `PLUGIN_DATA` (0.162.1).
- A trust key such as `progressive-reviewer@herdr-progressive-reviewer:hooks/hooks.json:user_prompt_submit:0:0` holds no version; its hash changes with the command (0.162.1).
- `config/batchWrite` with `upsert` on `hooks.state` trusts a hook; no edit removes a key (`null` is refused), so removing one means writing the user layer's `hooks.state` again without it (`replace`, with the layer's version from `config/read` and `includeLayers`) (0.162.1).

## Forks

- `thread/fork {threadId}` answers in tens of milliseconds, and `turn/start` on the fork runs inside the daemon, with no new process (0.162.1).
- A fork inherits the model, the sandbox and the approval policy, starts its own MCP servers, and sees the same tools (0.162.1).
- A fork's hooks fire in the daemon: `SessionStart` with source `fork`, `UserPromptSubmit`, `Stop` (0.162.1).
- A fork's first request repeats the parent's input except the IDs of its first two items (the tool list and the base instructions) and `prompt_cache_key`, which is the thread ID; whether the API then serves it from the parent's cache is not measured (0.162.1).
- Overriding `approvalPolicy` or `sandbox` on a fork appends a permissions message after the history (0.162.1).
- A `config` override of `thread/fork` that removes MCP servers changes the tool list, the first input item, so the fork loses the parent's prompt cache (0.162.1).
- A `config` override that names an MCP server the configuration lacks, such as a plugin's, fails `thread/fork` (`invalid transport`) (0.162.1).
- Forks live in the daemon, not in the forking client's process group: a client that dies leaves them running their turn (0.162.1).
- `turn/interrupt` stops a turn; `thread/delete` removes a thread and interrupts its running turn; an interrupted turn leaves its prompt in the history, then a `<turn_aborted>` developer message (0.162.1).
- `thread/fork` without overrides answers with the settings the fork inherits (`approvalPolicy`, `sandbox`, `approvalsReviewer`); `thread/settings/update` changes them for the next turns (`sandboxPolicy` in the shape `sandbox` came in), and the next turn appends a permissions message after the history (0.162.1).
- A session keeps its settings and its `config` overrides once a TUI resumes it: neither the TUI's `/resume` of a loaded session nor a `thread/resume` with `config` changes them (0.162.1).
- `thread/settings/update` has no `config`, so a `config` override such as `default_tools_approval_mode` stays on the session after the agent resumes it (0.162.1).
- Codex asks the model for a title of a session's first prompt, in a request of its own that quotes the prompt (0.162.1).

## Approvals

- Approvals go to the session's clients; the client that forked is the fork's only client (0.162.1).
- Under the read-only sandbox, `on-request` and `untrusted` both ask (`mcpServer/elicitation/request`, `_meta.codex_approval_kind: "mcp_tool_call"`) before an MCP tool with the default mode (0.162.1).
- They run without asking a tool set to `approval_mode = "approve"` and a tool its server marks `readOnlyHint`: the sandbox and the approval policy alone do not stop these (0.162.1).
- A tool's own `approval_mode` wins over `default_tools_approval_mode`; the request names the tool only in its message (`Allow the <server> MCP server to run tool "<tool>"?`) (0.162.1).
- `{"action": "accept", "content": {}}` runs the tool; a decline answers the model "user rejected MCP tool call" (0.162.1).
- `on-request` runs commands inside the sandbox without asking; `untrusted` asks for every one, `ls` included (0.162.1).
- The automatic approval review can reject an MCP tool that sends data as sending to an unverified destination; `approval_mode = "approve"` under `[mcp_servers.<server>.tools.<tool>]` preapproves it, and `/approve` authorizes one retry, still reviewed (0.154.0).

## MCP registration

- The user-level registration lives in `$CODEX_HOME/config.toml` (default `~/.codex/config.toml`); `mcp_servers.<server>.args` passes the server its arguments (0.154.0).
- Project entries (`.codex/config.toml`) can override the user settings (0.154.0).
- There is no `/mcp reload`: a running session loads a new registration only after a restart or a resume, or an app-server client's `config/mcpServer/reload`; meanwhile `/mcp` can list the tools with an `unknown` status (0.154.0).
- A session starts its MCP servers when it starts: a server that listens later gives it none of its tools (0.162.1).

## The TUI

- A model migration notice at start (`gpt-5.6-terra` to `gpt-6-sol`) drops what is typed meanwhile (0.162.1).
- In a Git repository, `/new` asks "Where should the new conversation run?" (current worktree or a new one) in place of the input box, and takes the next Enter as the answer; no setting turns it off (the `worktrees` feature is `removed`) (0.166.0; not 0.162.1).
- The start screen the TUI opens on takes a prompt typed at once (0.166.0).
- A daemon restart right after a `/resume`, before the TUI took a turn, left it with "This conversation is unavailable … input is paused" (0.162.1).
- The server prints `app-server control socket listening` under `RUST_LOG=codex_app_server_transport=info` once its socket listens (0.162.1).
- Herdr lists both the Node wrapper and the native binary among the pane's processes (0.162.1, Herdr 0.9.5).
- `thread/read` of a deleted thread fails (0.162.1).
- Codex runs a plugin hook's command as `$SHELL -lc <command>`, with `PLUGIN_ROOT` and `PLUGIN_DATA` set (0.162.1).
- Under `TMPDIR`, Codex warns that it refuses to create its helper binaries there, and goes on (0.162.1).
- `codex exec fork <id> <prompt>` runs one process per fork (0.162.1).
