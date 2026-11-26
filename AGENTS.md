# Ownership

You are the owner and long-term maintainer of this codebase, and its design is up to you:
agents write all of its code. Keep how existing
features behave, unless the task asks for a new behaviour or it fixes a bug.

# Eyes and hands

Give yourself eyes and hands on what you build: run it, look at it, act on it as its user
would, and fix what you see. Tests prove what you thought of; eyes show what you did not.
The `visual-testing` skill runs the review pane in a private herdr-app that never shows on the
user's screen, and shows it; `explore-page-tools` shows the Explore page and acts on it;
`install-check` runs `make install` in a scratch home; `real-agent-checks` runs a real round
with real Claude Code. When a part of the product has no such tool, build one before you rely
on tests alone.

# Standards

- Name things with the terms of `CONTEXT.md`, in code, docs and UI text, and use
  none of the synonyms it lists to avoid. Add a `CONTEXT.md` entry only for a
  term the user introduces or corrects, in one line.
- Keep crates small, don't hesitate to create tiny ones or split a big one.
- Reuse existing types between workspace crates. Do not create a local copy of
  a type only to adapt it for another crate.
- Reuse shared UI components for rendering, editing, focus and keyboard behavior.
  Do not duplicate their internals or intercept component-owned keys in
  feature-specific wrappers. Extend the shared component when behavior is missing,
  then reuse it across the affected views.
- Design types first, then implement their methods.
- Use `#[must_use]` only when ignoring a return value is likely to cause a bug.
  Do not add it to routine getters or to functions that return types that
  already have this attribute.
- Always use the smallest visibility level that permits the required use. Do
  not make an item `pub` when private or `pub(crate)` visibility is sufficient.
- Make every unit test go red on a plausible bug in the code it calls; a test
  that cannot is tautological. Before writing or code-reviewing a unit test,
  load the `unit-tests` skill: the kinds of tautological test, and what to
  assert in a prompt test.
- Refresh filesystem-driven views through filesystem events (inotify on Linux),
  not periodic polling. Reuse the repository watcher and event pipeline so idle
  views do not spend CPU checking for file changes.
- With the current `eyre` version, use `Err(eyre::eyre!(...))` in
  expression-position match arms, or put `eyre::bail!(...);` in a statement
  block, to avoid the trailing-semicolon macro warning.
- Before writing an Explore page e2e test, load the `explore-page-tools` and
  `e2e` skills.
- Test a change that only moves or removes docs or comments with `make lint` and
  `make check-changed`, check that a comment removal changes comment lines only
  (doc tests, `#[doc]` text a test or a prompt reads, and Markdown compiled into a
  crate are code), and try each skill it changes once by running the tool it describes.

# Change workflow

Follow these steps for every change: a feature, a fix, or a change to docs.

1. Create a fresh jj change before implementation. Use the previous change as
   the fixed point for this change.
2. Implement the change, and check it as you go with the levels that fit it,
   cheapest first:
   1. `make lint`: clippy on every target, the Explore page client's types, and
      the complexity gate.
   2. `make test CRATES="crate-a crate-b"`: the unit tests of the crates the
      change touches.
   3. `make test`: every unit test of the workspace.
   4. The end-to-end tests of what the change touches: `make e2e-tui` for the
      pane, `make e2e-explore` for the Explore page.
   Choose the levels at your discretion: run the Explore page tests only for a
   change that reaches the page, which
   `make check-changed CHECK_CHANGED_ARGS=--dry-run` lists with the other checks
   a change reaches. Once the change is complete, run
   `make fmt`.
3. For a change the user can see, explore the affected paths with the tools of
   Eyes and hands, in the pane or on the Explore page, and fix what you find.
4. Invoke the `code-review` skill against the fixed point; the subagents it
   starts are authorized. Fix its findings, by your judgement for a judgement
   call, and review again until a round finds nothing that you fix, for at most
   two rounds. After the second round, fix what it found, and report any
   finding you leave unfixed, with the reason.
5. Once step 4 is over, run the gate, with the change as the working copy:
   `make check-changed`. It runs only the checks that the files changed since
   the change's parent reach, and says why; for a stack of changes, set
   `CHECK_BASE` to the revision below the first. Fix whatever fails, even outside the change, run
   `make fmt`, review those fixes with one round of step 4, and run the gate
   again until it passes.
6. Describe the change with `jj describe`: a plain imperative
   subject, then what changed for the user and why. The description and your
   closing report name each thing the change does, however small: check both
   against `jj diff`. Write both for a reader who did not follow the session (see
   Reports).
7. Run `make install` once the gate passes, unless the user deferred
   installing; say so when you skip it. It builds and installs only; it does
   not run the checks.
8. Keep later user-feedback fixes in the same change, and run the gate again
   once each set of them is done. Create another change only when the user
   asks for something new.

For an investigation, which answers a question and builds nothing, check the answer by its
sources in place of step 2's levels and step 3: give the `file:line` of each claim about this
code, and the docs, API schema or `--help` of each claim about an outside tool, and report
what you tried live as what you saw, with the command. Give the plan to whoever asked.

Run the make targets through `scripts/dev-shell` so the pinned Rust
toolchain, `cccc`, and `cargo-nextest` are available, and `make install` through
it too so the installed build uses the same toolchain. Keep the warning gate of
the checks (`-Dwarnings`) enabled; fix warnings in the current change instead
of overriding `-Dwarnings`.

Build what the user asks for yourself, in the current workspace, so the
user sees each change live. Supervise subagents in separate jj workspaces only
for a large feature planned as a graph of tickets, or for many independent
changes at once. When you work in a jj workspace, supervise agents there, or run
gates in parallel, load the `checks` skill: how to enter
the dev shell, keep work visible, and run several gates at once.

# Reports

The user reads your reports, change descriptions and messages after the work, often coming
back from other work. They have not seen your tool calls, your reasoning or the files you
opened. So make each message stand on its own, and put the key points in its first few
sentences. Do not refer to something that only a later line explains. Use plain words in full
sentences, and use no term or abbreviation that you made up during the work: say what the
thing is. Use a paragraph of prose where ideas connect, and bullets for separate items. Leave
out details the reader does not need, but keep the context they need to act.

# Sandbox E2E Tests

Herdr E2E tests start an isolated background server with private socket,
config, state, workspace, and agent paths. Never use, restart, or modify the
user's live Herdr server or panes during tests.

The vision MCP server and herdr-common's `app-sandbox` run herdr-app in a sandbox of their own,
on a headless display, with their own core and environment, and act on nothing of the user's
Herdr or herdr-app. The vision server's sandbox runs in bubblewrap, where its processes reach
none of the user's Herdr, herdr-app or hc sockets, nor those of their runtime directory
(the `visual-testing` skill); the rest of the host's files show at their own paths.

The Codex Linux sandbox blocks Unix-domain socket bind and connect operations.
Run tests in the normal sandbox first. If an E2E test fails with `EPERM` while
it creates or connects to its private socket, retry the same exact command
with sandbox escalation.

Do not enable project-wide network access. Do not replace the real Herdr
server with a mock only to avoid the sandbox restriction.

# Skill settings

## Agent skills

### Issue tracker

Issues live in GitHub Issues for flupke/herdr-progressive-reviewer (via `gh`). See `.agents/settings/issue-tracker.md`.

### Triage labels

Default five-role vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `.agents/settings/triage-labels.md`.

### Domain docs

Single-context: root `CONTEXT.md`. See `.agents/settings/domain.md`.
