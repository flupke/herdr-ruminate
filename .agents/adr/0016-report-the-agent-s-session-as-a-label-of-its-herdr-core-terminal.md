# Report the agent's session as a label of its herdr-core terminal

Supersedes [ADR 0012](0012-report-a-codex-agent-s-resumed-session-to-herdr.md): the Codex
resume reports through the host, to Herdr under Herdr as 0012 said. Supersedes, for herdr-core,
the part of [ADR 0005](0005-hear-the-agent-through-a-claude-code-plugin.md) that limits the
hooks to a Herdr pane.

The review access values, the pinned agent of an Explore round and run-ahead bind the agent's
session, which Herdr learned from its own hooks. Under herdr-app those hooks do nothing, and
herdr-core keeps no agent session: an agent there had none, so a grant bound its process group
alone, and a round could not tell that the agent changed its session.

Under herdr-core, the agent's session is a label of its terminal, `herdr-reviewer.session`,
whose value is `<agent>:<session>`; the reviewer reads it as the agent's session when it names
the agent the terminal's status names. The reviewer's Claude Code hook sets it at
`SessionStart`, and removes it at `SessionEnd` when it still names the session that ended, so
that a Claude Code started later in the terminal never runs on its predecessor's session
before its own hook ran; a session cleared or resumed is left to its successor, which names
itself. The hook sets it only for a Claude Code that leads the foreground process group of its
terminal: a `claude` that a tool or an MCP server of the agent runs inherits the terminal's
variables, but has no terminal or leads no group, and would otherwise name its session as the
agent's. A Claude Code started through a wrapper that does not `exec` it leads no group either,
and names no session. A crash leaves the label until the next session of the terminal replaces
it: until then, a Claude Code started there without the reviewer's plugin runs on the crashed
one's session, as the reviewer sees it.

The Codex resume of ADR 0012 reports through the host: under Herdr, `pane.report_agent_session`
as before; under herdr-core, the label. It still reports only when the host reports a session
for the pane, and under herdr-core nothing names a Codex agent's session, since Codex's hook
runs in its daemon, which cannot tell the terminal: there it reports none.

herdr-core tells each change of a terminal's labels: the reviewer follows them, and a prompt
that waits for its agent's session waits for that event, where under Herdr it asks Herdr again
every 100 ms.

ADR 0005's hooks reach the reviewers of herdr-core as those of a Herdr server: a directory per
core, named after a digest of `HC_TERMINAL_SOCKET`, and the terminal's address,
`HC_TERMINAL_ID`, as the pane. Herdr's variables win when both are set, as the review pane
picks its host ([ADR 0014](0014-talk-to-herdr-core-behind-the-host-traits-beside-herdr.md)). A
fork inherits neither host's variables.

Considered:
- A file per terminal under the hook directory, which the reviewer would watch: a second channel
  beside herdr-core's events, which already tell label changes to every subscriber, and a file
  the reviewer of another core would never read.
- Asking herdr-core upstream for an agent session field: the reviewer could not run on
  herdr-app until it lands, and a label carries the same value.
- Naming the agent's process group in the label, and taking the session only while that group
  holds the terminal's foreground: it would also catch a crash, but the session would go each
  time the agent hands the foreground to a program, such as an editor, and the reviewer would
  follow the terminal's foreground events too.
- Reaching the reviewers of herdr-core as Codex's hook reaches every reviewer, through the
  directory of every host, by the agent's session: Claude Code's hook knows its terminal, which
  keeps its events to the reviewers of its own core, and run-ahead expects them by pane.
- Keeping the 100 ms recheck under herdr-core too: it asks the core ten times a second while a
  prompt waits, for what the core's events tell.
