# Herdr, herdr-common and herdr-bus

Herdr here is `herdr`, the older terminal multiplexer and server whose plugin the reviewer is; herdr-app and its core `hc` are in `herdr-app.md`. A version in parentheses is the one the fact was seen on.

## Herdr's server and agents

- `herdr server` prints `herdr server running` on its stderr once its API socket is bound (version not recorded).
- An `events.subscribe` subscription holds once the call returns: subscribe before the action whose event you wait for (version not recorded).
- Herdr detects an agent within about half a second, but reports a newly detected agent's first `idle` about 4.4 s later (version not recorded).
- Herdr detects an agent by its program's name (`claude`, `codex`) and reads its turns from the terminal's title (version not recorded).
- Herdr ignores a reported `working` state of an agent it detects by name, so a prompt to it stalls for Herdr's 5 s (Herdr 0.9.3).
- Herdr sends `AgentDetected` with `released: true` only when it did not detect the agent by name yet, which a loaded machine makes rare (version not recorded).
- Herdr releases an agent whose process ended, then reports no agent in the pane until it detects one again; an agent that starts where it released one is detected by name, whatever it reports (version not recorded).
- Herdr sends no event when a pane's screen changes (version not recorded).
- Herdr judges an agent by the state it knows: an integration that reports the agent's state in place of Herdr's screen reading must report the turns it starts, or every prompt counts as not started (version not recorded).
- `pane.process_info` lists a pane's processes (version not recorded).
- `pane.report_agent_session` raises no event (version not recorded).

## Prompts

- `agent.prompt` types also while the agent is working (version not recorded).
- Herdr types a prompt into the agent's input box without emptying it: text left there joins the prompt into one (version not recorded).
- Herdr rejects a prompt to a `blocked` agent (version not recorded).
- Herdr refuses a prompt to an agent it has not detected yet (0.9.5).
- Herdr takes 0.3 to 0.6 s to type a long prompt (version not recorded).
- `herdr pane send-text` leaves text in an agent's input box; `herdr pane send-keys <pane> ctrl+u` clears it (version not recorded).

## Tabs, plugins and state

- Herdr names a tab `<workspace>:t<number>` and keeps both in its session snapshot, so a tab keeps its ID across a Herdr restart (Herdr 0.9.3; read in its source, not tried).
- `HERDR_TAB_ID` does not follow a pane moved to another tab (version not recorded).
- Herdr sends the focused tab's `tab_id` in a plugin action's context (Herdr 0.9.5).
- Herdr gives a plugin `HERDR_PLUGIN_STATE_DIR` (`~/.local/state/herdr/plugins/<plugin id>`), `HERDR_PLUGIN_CONFIG_DIR`, `HERDR_PLUGIN_ID`, `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID` and `HERDR_PLUGIN_CONTEXT_JSON` (focused pane, its folder, tab) (version not recorded).
- Herdr creates only `plugins/<plugin id>` in its state folder and lists nothing beside it, so a folder of your own there is safe (Herdr 0.9.3).

## Agent hooks and sessions

- Herdr learns an agent's session from its own hooks, `~/.claude/hooks/herdr-agent-state.sh` and `~/.codex/herdr-agent-state.sh`, which exit at once without `HERDR_PANE_ID` and `HERDR_SOCKET_PATH` (version not recorded).
- Herdr writes its Claude Code hook into `~/.claude/settings.json`, among the user's own entries (version not recorded).
- Herdr hears a Codex agent's `/resume` only from its Codex hook's `SessionStart` with the source `resume`, at the agent's next turn; until then it reports the old session (version not recorded).
- Herdr's session for a Codex pane can be another pane's: Codex's shared app-server daemon runs Herdr's hook with the environment of the TUI that started the daemon (version not recorded).
- `pane.report_agent_session` takes a `source`, a `session_start_source` and a `seq`, and orders the reports of one source by `seq` (version not recorded).

## herdr-bus

- herdr-bus is the machine's message bus, one SQLite file: `$HERDR_BUS_DB`, else `$XDG_DATA_HOME/herdr-bus/bus.db` (herdr-bus a5d7accc).
- herdr-bowtie routes the @mentions of the `herdr.words` messages published on the bus (herdr-bus a5d7accc).
- The bus keeps the first message of an `(output, id)`: a retry publishes once (herdr-bus a5d7accc).
- herdr-bus waits up to 5 s on a locked bus file (herdr-bus a5d7accc).
- `herdr-bus messages --output words --db <file>` lists what was published, newest first (herdr-bus a5d7accc).
- A revision with a newer schema migrates the bus file the first time it opens it, and older builds then refuse the file: move a program's pin with herdr-bowtie's, and install every program that opens the bus together, on every machine (herdr-bus a5d7accc).
- a5d7accc (schema 2) is the first revision the reviewer can pin; a0ebd675, the first with herdr-words, cannot open a bus that herdr-bowtie on a5d7accc migrated (herdr-bus a5d7accc).
