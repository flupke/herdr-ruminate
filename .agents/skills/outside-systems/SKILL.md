---
name: outside-systems
description: Observed behaviour of the systems the reviewer drives (Herdr, herdr-app and hc, herdr-bus, Claude Code, Codex, Antigravity CLI, jj, gh, language servers, cloudflared, browsers, terminals), one line per fact with the version it was seen on. Load before relying on, testing or debugging how one of them behaves.
---

# Outside systems

Each reference file holds what was observed of one system, one fact per line, ending with the
version it was seen on. Read only the file of the system you deal with:

- [herdr.md](references/herdr.md): the Herdr server, its plugins, hooks and timings, and herdr-bus.
- [herdr-app.md](references/herdr-app.md): herdr-app, herdr-core and `hc`: panes, focus, mods, agent events, how hc types a prompt it holds, reading a terminal's screen.
- [claude-code.md](references/claude-code.md): Claude Code's sessions, forks, hooks, plugins, models, MCP servers and terminal screen.
- [codex.md](references/codex.md): Codex's app-server daemon, hooks, plugins, forks, approvals, MCP registration and TUI.
- [antigravity.md](references/antigravity.md): Antigravity CLI (`agy`), its OSC 7501 status, input line, dialogs, MCP servers and plugins.
- [jj-gh.md](references/jj-gh.md): jj, Git and gh.
- [language-servers.md](references/language-servers.md): rust-analyzer and Expert.
- [other.md](references/other.md): cloudflared, browsers, terminals, Ratatui and Linux.

A fact can go stale when the system updates: when what you see differs, trust what you see,
and fix the line with the version you saw it on.
