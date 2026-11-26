---
name: visual-testing
description: See and act on the real review pane, in a private herdr-app off the user's screen. Use when asked for a screenshot of the review pane, to see or drive the pane, or to run make vision or herdr-common's app-sandbox.
---

# Visual testing

The vision MCP server, `reviewer-vision`, is your eyes and hands on the review pane. Its
tools' schemas say what each one takes and returns. This skill holds what they cannot say.

For a private herdr-app driven from the shell (`app-sandbox`), for mods, several panes or
another machine, read [references/app-sandbox.md](references/app-sandbox.md). It is also the
route when your session has no vision tools: `make vision` builds them but takes no screenshot.
There, the reviewer needs a scratch repository (`jj git init` in a new folder under
`/var/tmp`), and a plain shell can stand in for the agent's terminal. `app-sandbox shot` is
1400x900 by default.

## Setup

- `.mcp.json` registers the server through `scripts/vision-mcp`. It builds the server first
  when it is missing or older than the sources (slow; it says so on standard error).
- An agent that herdr-bowtie starts needs no setup step.
- In any other session, a build in a new workspace can outlast Claude Code's wait for the
  server. Run `make vision` before you start the session: it builds the server and the
  reviewer and prints how to use them. If the session already lost the server, run it, then
  ask the user to reconnect the server with `/mcp`.
- The session gets the server's instructions but no tools: read Claude Code's log of the
  server in `~/.cache/claude-cli-nodejs/<project>/mcp-logs-reviewer-vision/`. `Failed to fetch
  tools: request _meta is missing` means the server's rmcp is older than 3.5.1.
- The server lives in `tests/tui`, a Cargo workspace of its own. It takes herdr-common by
  path from the checkout beside this one (`../herdr-common`): a clone of this repository
  alone cannot build it.
- `start` builds the reviewer and the fake agent of the checkout, so each session runs the
  current code. A change in herdr-common does not trigger a rebuild: run `make vision` after
  you pull it, and `cargo update -p fake-agent` so the fake agent and the reviewer's
  `vision-signal` stay at one revision (a changed frame marker goes unread otherwise).
- A running server never rebuilds: the MCP client must start it again to get a change.

## The session's sandbox

- `start` opens a private herdr-app, with its own herdr-core and home, on a headless Weston:
  it never shows on the user's screen nor takes their focus. It holds a "reviewer vision"
  space with the fake agent (named `claude`) in a tab and the reviewer right of it, as the
  launcher mod opens it.
- The reviewer's terminal has the focus, so `key` and `type` reach it. `screenshot` is the
  whole window, 1600x1000, as herdr-app draws it. It returns the image only: nothing is saved.
- The sandbox's `/tmp`, network and runtime directory are its own. The user's Herdr,
  herdr-app and hc directories show empty. Other host files show at their own paths.
- The scratch repository is in `/var/tmp`. After a `SIGKILL` of the server it stays there as
  `review-repository-*`: delete it.
- The session directory holds `claude-config` (the agent's and forks' transcripts) and
  `state` (the reviewer's state). A session touches neither the user's reviewers nor their
  Claude Code configuration.
- The reviewer's MCP port and the Explore page's port are forwarded to the host. A forward
  fails when that port is taken on the host.
- `reopen` closes the reviewer's and the agent's panes and starts new ones.
- `stop`, or a client that leaves, stops the sandbox and every process in it.

## How the tools wait

- An action (`key`, `type`, `click`, `drag`) returns the screen of the frame that followed
  its input, with no delay of its own. So do the reviewer's tools that act otherwise
  (`reply`, `fork_submit`, `fork_end`, `jev`, `agent_prompt`).
- Asynchronous work (a refresh, a diff loading, Jev, an agent's turn) shows later: `wait_for`
  it. It checks the screen at once, then after each frame.

## Gotchas

- `click` and `drag` take window pixels only, not terminal cells or text. Read the pixels off
  a `screenshot`, or use the reviewer's keys (`e` opens Explore, `p` starts a round in the
  pane).
- `screen` gives text only, without colors or its size.
- `actions.jsonl` records the server's own tools only, not `screen`, `key` and the like.
- `type` types each character as its key, a newline as Enter. `paste: true` pastes the text
  in one bracketed paste, which the reviewer's editors take whole.
- The first screen `start` returns can come before the repository loaded: `wait_for` a file
  name first.
- `start`'s `change` only writes files. To delete a file, put it in `base`, then remove it from
  the repository `start` returns, and `wait_for` its name to see the reviewer noticed.
- To replay a jj history (`jj duplicate`, `split`, `squash`, `abandon` of a reviewed change),
  run it from your shell in the repository `start` returns, `wait_for` the new change ID, and
  `key` `v` `v` to show each revision's share. Reproduce it first in a unit test over
  `JjFixture`, as `crates/review-state/src/carried_marks.tests.rs` does.
- While the reviewer has handed the terminal to `$EDITOR`, actions fail after their guard.
  Quit the editor through its own keys first.
- On the Explore tab, `Start` runs the round on the Explore page, `Start in the pane` (`p`) in
  the pane.
- A `drag` copies the selection to the sandbox's clipboard, never the user's.
- `agent_prompt` types through `hc send`, without moving the focus.
- Open issue (#22): an answer while `agent_working` shows the agent working still continues as
  the fork (the ignored test `an_agent_shown_working_at_the_answer_takes_it_itself`).
- The Explore page draws itself from a socket: `curl` shows no round. Open its address
  (`explore_page`) with Playwright from `tests/explore-page` (after
  `scripts/dev-shell make e2e-explore-deps`) in the dev shell's headless Chromium, or read its
  text with `"$E2E_CHROMIUM" --headless --virtual-time-budget=8000 --dump-dom <address>`.

## Recipes

The reviewer's tools reject unknown and missing fields: start from the arguments in
`tests/tui/examples`.

A whole Explore round in the pane: `key` `e`, then `p`. `turn` returns the kickoff. `reply`
with `question.json` shows "Question 1". `key` `1`, then `ctrl+enter`, sends the answer. `turn`
returns the wakeup. `reply` with `conclusion.json` shows "To be implemented".

A review comment answered by the agent: `key` `tab`, `j` to a changed line, then `a`. `type`
the comment, then `ctrl+enter` posts it. `turn` returns its `comments` wakeup. `reply` with
`get_new_messages` and `{}` returns the threads. Then `reply` with `reply`, the fetched
`thread_id` and `in_reply_to`, a new `message_id` and the `text`. `key` `t` shows the answer in
the Threads tab. `reply` fills in the access value from the latest turn: when that is an
Explore turn, pass the `comments` turn's `access` as `review`.

Run-ahead: read [references/run-ahead.md](references/run-ahead.md).

## Regression tests from a session

- Turn a reproduced finding into a deterministic case in `tests/tui/tests` (see its
  `README.md`): the expected behavior, the interaction sequence, and the evidence needed to
  understand it, kept beside the test (session artifacts under `target/` are not tracked).
- A test drives the reviewer in an app sandbox, as the server does, or in a private Herdr
  (`PrivateHerdrServer`), never the user's.
- `make e2e-tui` runs them with the server's own tests (`tests/tui/src/vision/session.tests.rs`
  holds the sandbox sessions).
