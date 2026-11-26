# Claude Code

What Claude Code was seen to do, with the version seen on.

## Sessions, forks and `/resume`

- Under herdr-core, Claude Code reports `idle` before it writes the end of its turn to the transcript, which then lacks the turn's last tool calls and answer; under Herdr its `Stop` hook reported idle after that write (2.1.318).
- An answer that ends a turn has a `stop_reason` of `end_turn`, `stop_sequence`, `max_tokens` or `refusal`; an interrupted turn has none of them (2.1.318).
- Claude Code takes what is typed while it loads a session after the load, in order: a later `/resume` cannot be overtaken by an earlier one (2.1.318).
- A blocked prompt runs no turn: Claude Code shows the reason and the original prompt in its transcript, and puts the prompt back in the input box (2.1.309; 2.1.292 left the box empty).
- A fork (`-p --resume --fork-session`) runs its shell commands in the user's shell, Bash or Zsh, with the user's options (version not recorded).
- A fork inherits the agent's configuration, so the hooks of the user's, the project's and the plugins' settings run in it too; only the hooks of a replaced `--settings` do not (version not recorded).
- `disableAllHooks` false in a `--settings` outranks a `disableAllHooks` of the user's or the project's settings (version not recorded).
- Of a repeated command-line option, Claude Code takes the last value (2.1.329).

## Hooks

- `SessionStart` names the session started or resumed (with its `source`, such as `resume`); `UserPromptSubmit` sees the submitted text and can block it (version not recorded).
- `UserPromptSubmit` runs within milliseconds of the submission, also for a prompt Claude Code queues while it works (2.1.309).
- A hook sees a prompt as it is submitted, not as a turn starts, and each event runs a process of its own (version not recorded).
- The prompt a hook reports has its end trimmed (version not recorded).
- Claude Code reads its hooks when the session starts: a session started before a plugin's install, or with it disabled, runs without them (version not recorded).
- Without a login, `claude` runs no hook: a scratch `CLAUDE_CONFIG_DIR` can check an install but not the hooks (2.1.292).

## Plugins

- `claude plugin marketplace add` and `claude plugin install` succeed when they ran before; run from another checkout, they move the marketplace and the plugin's options to it (2.1.292).
- A plugin of a directory marketplace loads in place: an edit of its hooks takes effect at the next session start, without a reinstall; `claude plugin install` of the installed plugin says so (2.1.318).
- A hook command may hold `${user_config.<option>}` only in exec form (`command` and `args`); in a shell string Claude Code refuses it (2.1.292).
- A hook gets `CLAUDE_PLUGIN_ROOT` and each option as `CLAUDE_PLUGIN_OPTION_<NAME>` (2.1.292).
- `claude --plugin-dir <dir> --settings '{"pluginConfigs":{"<plugin>@inline":{"options":{...}}}}'` runs a plugin without installing it, and creates `~/.claude/plugins/data/<plugin>-inline` (2.1.292).
- A hooks module loads when the session starts: a session started before the plugin had its module runs without it (2.1.329).
- A hooks module's `turn.step` rewrite of `model` and `effort` writes no setting and opens no dialog; a `-p --resume --fork-session` prompt it switches runs at that effort too (2.1.329).
- A module's `$.agent.register` type takes a full model ID, and is dispatched in the turn of the prompt that registered it (2.1.329).
- `claude plugin test` runs a hooks module's tests (2.1.329).
- Claude Code writes a plugin module's types under `.claude-plugin/types/` of the plugin folder once it loaded it from there, so `tsc` type-checks the module only after that (2.1.329).
- A skill installs apart from the program that ships it, so it can lag the binary (version not recorded).

## Models and effort

- `/model <id>` and `/effort <level>` typed in a session save the value as the user's default for new sessions in `~/.claude/settings.json` (`model`, `effortLevel`): no typed form is session-only (2.1.329).
- `/model` in a conversation whose cache may be warm opens a "Switch model?" dialog that takes the next typed text; Enter or `1` confirms it (2.1.329).
- The Agent tool's `model` takes only `sonnet`, `opus`, `haiku` or `fable`, a full ID is refused (`Invalid option`); an alias runs the family's current model; its `effort` takes the five levels (2.1.329).
- A fork's model is that of the transcript's last answer (2.1.329).
- The prompt cache is per model: a switch to another model re-reads the whole conversation uncached; an effort change alone keeps the cache (2.1.329; numbers in `docs/studies/explore-models-cache.md`).
- The transcript records each request's model and effort in its `api-request` entries (`params.model`, `params.output_config.effort`); a subagent's are under `<session>/subagents/agent-*.jsonl`, with its `querySource` (`agent:builtin:general-purpose`) (2.1.329).


## A fresh configuration directory

- These `.claude.json` keys skip the first-run screens: `hasCompletedOnboarding`, a theme, the project's `hasTrustDialogAccepted` and `hasCompletedProjectOnboarding`, `hasSeenAutoDefaultNudge` and `hasSeenAutoModeEntryWarning` (2.1.310).
- Without `hasSeenAutoDefaultNudge`, the first prompt typed with "don't ask" on opens a dialog offering auto mode, which takes the prompt and its Enter (2.1.310).
- `claude setup-token` prints a token that logs in a scratch `CLAUDE_CONFIG_DIR` through `CLAUDE_CODE_OAUTH_TOKEN` (version not recorded).
- Under a parent Claude Code session, the inherited `CLAUDE*` and `ANTHROPIC*` variables make a child `claude` load the parent's plugins from `~/.claude` and reach that session (version not recorded).
- Claude Code keeps its MCP servers' logs under `XDG_CACHE_HOME`: `~/.cache/claude-cli-nodejs/<project>/mcp-logs-<server>/` (version not recorded).
- Claude Code writes `~/.local/share/applications/claude-code-url-handler.desktop` when it is missing (version not recorded).
- With a `HOME` of its own, Claude Code shows its first-run screens (2.1.318).
- An account at its weekly limit answers no turn (2.1.318).
- In the insert mode of the vim editor mode (`~/.claude.json`), Ctrl+C does not quit; `/exit` does (2.1.318).

## MCP servers

- A session not started with `enableAllProjectMcpServers` asks once to approve a project's server, and waits 30 s for it to connect; `MCP_TIMEOUT` sets that wait (version not recorded).
- Claude Code opens a connection with a `server/discover` probe and, when no answer comes within 3 s, falls back to `initialize` (version not recorded).
- An rmcp server older than 3.5.1 serves the protocol of the first request it reads, so after the fallback it refuses `tools/list`: the session gets the server's instructions but no tools, and the server's log says `Failed to fetch tools: request _meta is missing` (version not recorded).
- A client keeps the server process it started: a rebuilt server reaches it only once `/mcp` reconnects it (version not recorded).
- The user-level registration lives in `$CLAUDE_CONFIG_DIR/.claude.json` (default `~/.claude.json`); a `--mcp-config` file can give a server its `"args"` (2.1.220).
- After a server changes a tool's schema, a running session must refresh its MCP tool definitions (2.1.220).

## Its terminal screen

- Claude Code empties its input line with Esc Esc, Ctrl-W, Delete or its vim mode's keys, none of which ends hc's count of unsent typing (Claude Code version not recorded; hc 0.1.20).
- An idle Claude Code prints nothing, so a watch on its screen costs nothing while it waits (version not recorded).
- Claude Code reports `blocked` to herdr-app while a question of its `AskUserQuestion` tool waits (2.1.318).
- Claude Code can move a conversation into a background `claude daemon run --origin transient` session (a new session ID, the old transcript ending with a `continued-in` record) on a pty of its own, so the terminal shows no status while it runs; what triggers it is not known (2.1.318).
- Never give Claude Code a scratch `XDG_DATA_HOME`: it copies itself into `$XDG_DATA_HOME/claude/versions` and points the user's `~/.local/bin/claude` there, even with `DISABLE_AUTOUPDATER` (2.1.318).
