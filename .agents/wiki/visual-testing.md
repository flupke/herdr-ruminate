# Visual testing (make vision)

How an agent drives the real reviewer pane through the vision MCP server, in herdr-common's app sandbox, how its tools wait for the reviewer, its gotchas, and turning findings into regression tests.

The vision MCP server is an agent's eyes and hands on the review pane. Its tools and their
schemas say what each one takes and returns; this page holds what they cannot say.

## Setup

Run `make vision` first: it builds the server and the reviewer, and prints how to use them.
`.mcp.json` registers the server as `reviewer-vision` through `scripts/vision-mcp`, which
runs the built server, or builds it first when it is missing or older than the sources
(slower, and it says so on standard error), with the dev shell's pinned herdr-app, hc, Weston
and Vulkan driver, which the sandbox runs, and the dev shell's `PATH`, where it finds
bubblewrap, nsenter and socat. Claude Code asks once to approve a project's
server. A session whose server was missing or stale when Claude Code connected may get its
instructions but no tools: run `make vision`, then drive the server over stdio with a small
MCP client (`initialize`, then `tools/call`), from one process, since a client that leaves
ends the session.

The server lives in [`tests/tui`](../../tests/tui), a Cargo workspace of its own: it takes
herdr-common's app sandbox, vision core and fake agent by path from the checkout beside this
one (`../herdr-common`), which a clone of this repository alone lacks, and patches the
herdr-common git dependencies of the workspace's crates to that checkout. `start` builds the
reviewer and the fake agent of the checkout in the dev shell, so each session runs the
current code. The server itself does not rebuild: an MCP client keeps the process it started,
so after a change under `tests/tui` the client must start the server again to get it.

## The session's sandbox

`start` runs the reviewer in herdr-common's app sandbox: a private herdr-app, its own core and
home, on a headless Weston that never shows on the user's screen nor takes their focus. It
opens a "reviewer vision" space there, the fake agent in a tab, and the reviewer right of
it, as the launcher mod opens it (`--focused-terminal`, `--herdr-app-home`): the reviewer runs
its hc backend. The reviewer's terminal has the focus, so `key` and `type` reach it; `screenshot`
is the whole window, as herdr-app draws it. The window is 1600x1000.

The sandbox runs in bubblewrap: its `/tmp`, network and runtime directory are its own, and the
user's Herdr, herdr-app and hc directories show empty, so its processes reach none of their
sockets, nor those of the user's display. The rest of the host's files show at their own paths,
so another socket, such as Docker's, stays reachable by its path. What the server shares with it
lives elsewhere:

- The reviewer's MCP endpoint (`reply`, `fork_submit`) and the Explore page's network listener
  take ports the server leases free on the host, and `start` forwards both from the sandbox's
  loopback (`AppSandbox::forward`, which runs `socat` from the dev shell inside). The page's
  own loopback address takes any port free in the sandbox: `explore_page` runs the Herdr action
  in the sandbox, then forwards the port of the address it opened. A forward fails when that
  port is taken on the host.
- The scratch repository is in `/var/tmp`, which the sandbox shows: the host's `/tmp` it does
  not, and the session directory sits under the checkout, whose `.envrc` direnv would refuse in
  the sandbox, failing the pane's language servers.
- The fake agent's event socket is in the sandbox's own `/tmp`, which the server reaches
  through `AppSandbox::directory`: a socket's path must be short.

The fake agent runs herdr-common's `fake_agent` from `crates/reviewer-fake-agent`
([the fake agent's program](private-herdr.md#the-fake-agents-program)), named `claude`: it reports its state to herdr-core in
OSC 7501, runs the hooks of the reviewer's Claude Code plugin through the checkout's
`reviewer-control`, and plays a fork when run-ahead starts it as one. The agent and its forks
keep their transcripts in `claude-config` in the session directory, and the reviewer's state is
`state` there: a session touches neither the user's reviewers nor their Claude Code
configuration. Under herdr-core the reviewer takes on its agent's environment, so the agent's
command drops `TYPESAFE_API_KEY` and `NO_COLOR` too.

`stop`, or a client that leaves or signals the server, stops the sandbox and every process in
it; after a `SIGKILL` of the server the kernel still ends them and frees the sandbox's `/tmp`,
but the scratch repository stays in `/var/tmp`, as `review-repository-*`. `reopen` closes the
reviewer's and the agent's panes and waits for herdr-core to report both ended before it starts
the new ones, which bind the same MCP port.

## How the tools wait

herdr-common's vision core gives `screen`, `screenshot`, `key`, `type`, `click`, `drag` and
`wait_for`; the server adds the reviewer's own. In a vision session the reviewer paints each
frame as one synchronized update and ends it with a terminal title, its frame marker
(herdr-common's `vision-signal`, which the reviewer takes from git): `vision frame=N ack=K
size=CxR`. herdr-core reports the title as an event after the frame. After its input, an action
pastes an acknowledgement request that the reviewer never hands to the application; the
reviewer names it in the next frame's marker, so an action returns the screen of the frame that
followed its input, with no delay of its own. The reviewer's own tools that act through
something else (`reply`, `fork_submit`, `fork_end`, `jev`, `agent_prompt`) send a request with no
input (the core's `Vision::settle`), and return the frame that names it, with `changed_rows`
as the core's tools give it. Asynchronous work (a refresh, a diff loading,
Jev, an agent's turn) shows later: `wait_for` it, which checks the screen at once, then after
each frame.

## What herdr-common lacks here

These are gaps of herdr-common, for its owner to fill (its issues in brackets); until then:

- `click` and `drag` take the window's pixels only: no click on a terminal cell or on a text
  (#17). Read the pixels off a `screenshot`, or use the reviewer's keys (`e` opens Explore, `p`
  starts a round in the pane).
- `screen` gives text only: no styled screen (colors as ANSI, #17); each action returns the
  whole screen, without its size.
- `actions.jsonl` records the server's own tools only: the core's go through its router
  untouched.

## Gotchas

- `type` types each character as its key, a newline as Enter; `paste: true` pastes the text
  in one bracketed paste, as a terminal pastes the clipboard, which the reviewer's editors take
  whole.
- `screenshot` returns the image alone: nothing is saved in the session directory.
- The fake agent comes from tests/tui's build, with the `fake-agent` crate of herdr-common's checkout;
  the reviewer comes from the root workspace's, with the `vision-signal` that `Cargo.lock` pins.
  After pulling herdr-common, `cargo update -p fake-agent` keeps the two at one revision: a
  changed frame marker would otherwise go unread.
- Open (#22): under herdr-core, an answer while `agent_working` shows the agent working still
  continues as the fork, though hc reports the agent `working` (the ignored session test
  `an_agent_shown_working_at_the_answer_takes_it_itself`).

- The first screen `start` returns can come before the repository has loaded: `wait_for`
  a file name first.
- `start`'s `change` only writes files: for a change that deletes one, put it in `base`, then
  remove it from the repository `start` returns; `wait_for` its name shows the reviewer saw it.
- While the reviewer has handed the terminal to `$EDITOR`, the editor reads the input and
  the acknowledgement request, and no frame names the request: an action then fails after
  its guard. Quit the editor through its own keys before acting on the reviewer again.
- On the Explore tab, `Start` runs the round on the Explore page and `Start in the pane`
  (`p`) in the pane.
- A `drag` copies a text selection to the sandbox's own clipboard, never the user's.
- `agent_prompt` types through `hc send`, without moving the focus.

A whole Explore round in the pane, with the arguments of
[`tests/tui/examples`](../../tests/tui/examples) (the reviewer's tools reject unknown and
missing fields, so start from these files): `key` `e`, then `p` (Start in the pane);
`turn` returns the kickoff; `reply` with `question.json` shows "Question 1"; `key` `1`, then
`ctrl+enter`, sends the answer; `turn` returns the wakeup; `reply` with `conclusion.json`
shows "To be implemented".

A review comment answered by the agent: `key` `tab`, then `j` to a changed line, then `a`;
`type` the comment, then `ctrl+enter` posts it; `turn` returns its `comments` wakeup;
`reply` with `get_new_messages` and `{}` returns the threads, then `reply` with `reply` and
the fetched `thread_id` and `in_reply_to`, a new `message_id` and the `text`; `key` `t`
shows the answer in the Threads tab. The thread tools take only the access value, which
`reply` fills in from the latest turn: when that is an Explore turn, pass the `comments`
turn's `access` as `review`.

## Run-ahead in a session

In the Explore tab, `key` `z` twice before the round starts: the pane shows `Run ahead
(experimental): every choice` (once, `the recommended choice`). After a question, `forks` with
`count` waits until that many forks started, and lists each fork's session, choice, prompt (its
access and Answer ID among the turn's files) and how far it got, the agent's session, and
run-ahead's log (`run_ahead_log` in `start`'s result, `state/run-ahead.log` of the session
directory). `fork_submit` plays a fork: it submits with the fork's own access, then the fork's
output shows the call and its result, as Claude Code's would. Then `key` `1`, then
`ctrl+enter`, answers bare: the pane and the Explore page show `Prepared while you were
thinking`, and `forks` shows the agent on the fork's session, which the next question's forks
copy and the next answer reaches. An answer with a comment runs the plain chain and shows `Not
prepared: you added a comment`. A bare answer before `fork_submit` waits for the fork: the pane
shows `The agent is finishing a turn it started while you were thinking` with a running time,
and `fork_submit` then shows the fork's turn; `Stop waiting` gives the answer to the agent, whose
next `turn` returns it.

- The arguments of [`conclusion.json`](../../tests/tui/examples/conclusion.json) make a fork's
  turn a conclusion. A fork's question must ask about a topic not decided yet: the reviewer
  refuses one on the topic just answered (`Do not ask again on a topic just decided`), and a
  topic's `status` is `open`, `accepted` or `needs_follow_up`.
- `forks` gives the agent's session it started last, which the fake agent reports once its
  `SessionStart` hooks, the reviewer's among them, ran: read it there after the action you
  wait for.
- `forks` lists the forks in the order they reported starting, which varies from run to run:
  run-ahead starts them at once. Find a fork by its `choice`.
- The Explore page is a client that draws itself from a socket: `curl` shows no round. Open its
  address (`explore_page`) in the dev shell's headless Chromium (`E2E_CHROMIUM`), with Playwright
  from `tests/explore-page` once `make e2e-explore-deps` installed it, or read its text with
  `"$E2E_CHROMIUM" --headless --virtual-time-budget=8000 --dump-dom <address>`.

### Scripting the forks and the switch

Per question, the agent driving the session chooses what each fork does, once `forks` listed it:

- submits its turn: `fork_submit`;
- is still working at the answer: nothing; the bare answer waits for it;
- ends without a turn: `fork_end`; crashes: `fork_end` with `crash` (exit status 1, no result).
  Both count a failure of run-ahead, as a switch that fails does: the third in a row stops
  run-ahead for the round, with a toast, and the round's later answers say `nothing was
  prepared`. Only a prepared turn used starts the count again;

and before the answer, what the switch to a fork does: `switch_unheard` makes the fake agent's next
`/resume` unheard, so the switch fails after its 20 s, the turn waits for Retry (`r`) and the fake agent
resumes its own session; with `heard_after_ms` it is heard that late and succeeds. Stop waiting
and Cancel answer play while it waits.

How to reach each `Not prepared` line, the same in the pane and on the Explore page:

| Line ends with | How |
| --- | --- |
| you added a comment / you answered None of the above | the answer itself |
| you sent the agent back to the drawing board / you wrote in the chat | sent from the Explore page, which alone has both; the pane shows the line too |
| the agent was working | `agent_working` true before the answer, false after it |
| the agent's conversation moved on | `agent_prompt` after the forks started |
| the lines left to review changed | a new changed line in the scratch repository |
| nothing was prepared for this question | run-ahead stopped for the round, or the agent working when the question came; or `agent_hooks: false`, since under herdr-core the plugin's hook is what names the agent's session |
| only the recommended choice is prepared | setting `the recommended choice`, another choice answered |
| the turn for this choice did not finish | `fork_end`, before the answer or while it waits |
| you stopped waiting for it | Stop waiting while the answer waits for its fork, or during the switch after it |
| the agent could not switch to the prepared turn | `switch_unheard`, then Retry |
| you stopped waiting while it was being used | `switch_unheard` with `heard_after_ms`, Stop waiting during that switch, then Retry |

`restart the agent to load … plugin` has no way there: under herdr-core it needs a session the
plugin did not name. Two more lines have none: `the round changed after it was prepared`, since every input of a
turn but the unreviewed lines is fixed when the round starts, and `the turn for this choice was not
ready yet`, which only an earlier version recorded.

## Regression tests from vision sessions

Populate [`tests/tui/tests`](../../tests/tui/tests/README.md) with deterministic E2E
cases derived from recorded vision sessions. Keep each case focused on a reproduced
finding, with the expected behavior and relevant interaction sequence. Preserve the
evidence needed to understand the case alongside the test; session artifacts under
`target/` are ignored by version control. A test drives the reviewer in an app sandbox, as
the server does, or in a private Herdr (`PrivateHerdrServer`), never the user's.

`make e2e-tui` runs them with the server's own tests: its logic, and sessions in an app
sandbox (`src/vision/session.tests.rs`) that wait on herdr-core's events and the fake agent's or its forks'.
