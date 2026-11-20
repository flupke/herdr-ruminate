# Type a prompt only while the agent's input line reads empty

Supersedes [ADR 0022](0022-end-unsent-typing-that-holds-a-wakeup-with-ctrl-u.md).

On 11-20 the user turned off herdr-core's hold behind unsent typing for every sender
(`draft_wait = 0`): the hold had stalled a card's answer for 92 s while the agent's line was
empty (herdr-bowtie#466). Without it, herdr-core types a `terminal.send` once nobody pressed a
key for its quiet pause of about 3 s, whatever the line holds: a person who pauses mid-sentence
gets the prompt typed after their words (measured on hc 0.1.21: 3.1 s after their last key). The
user's rule: "if prompt empty post, if not, wait". herdr-bowtie builds the same behaviour (its
task 466-instant-delivery); both must behave the same.

So the reviewer's herdr-core client reads the agent's input line right before each send, and
offers the send only while the line reads empty (`crates/hc-client/src/line_wait.rs`). Text in
the line, a screen with no input line (a dialog, a menu), or an agent whose line the client cannot
read (neither Claude Code nor Codex) holds the prompt, with no time limit; nothing is typed over
it. The client tells why it waits from its first moment, and the review pane shows it under the
comments the wakeup covers. It reads the line again each time the program prints, which
herdr-core's watch-only attach (`terminal.attach` with `observe`) tells within a millisecond or
so (median 0.5 ms; an idle Claude Code prints nothing). The line is read from the screen
herdr-core paints to that watcher, styles included, which tell a placeholder or a suggested prompt
(drawn dim) from typed text (`crates/agent-input-line`): `terminal.read` gives no styles, and
reads Claude Code's first placeholder as typed text, which would hold every prompt to a new agent.

A send offered on an empty line may still wait inside herdr-core for the quiet pause after a key
that showed nothing. The client offers it with a press of no key (`terminal.keys` with no key),
which herdr-core answers when that same pause ends: while it is unanswered, herdr-core has typed
nothing, and the watch goes on. Text in the line that is not the prompt herdr-core pasted, or no
input line, still there 150 ms later with the press still unanswered, means someone pressed a key
within the pause, and the client hangs up. herdr-core holds that press while it types, so the
paste itself tells it: the whole prompt, or its first 16 characters at least, blanks aside, or
the agent's stand-in for a long paste (`[Pasted`). A few characters that start the prompt are
taken for a person's, as every wakeup starts with the same words. herdr-core 0.1.20 and later cancel a waiting send whose client hung up (checked on
0.1.21: nothing typed); the client reads the line again and offers the send again once it is
empty. Once the press of no key is answered, the line shows the pasted prompt, or the agent went
working, the client hangs up no more: herdr-core answers a send only once the agent went working, or a few
seconds after it typed, and a hang-up then would lose the answer of a prompt typed already, which
the client would offer again. A send herdr-core answered was typed, once. Ctrl+U, its checks and
the read of the window's keyboard focus are gone.

A Codex agent is read too: its composer is the rows from its last `›`, its placeholder drawn dim
(Codex 0.166). An agent the reader knows nothing of gets nothing: the reviewer cannot tell typed
text there.

This reads the agent's screen, which ADR 0005 moved run-ahead off because a new design of Claude
Code's screen breaks such reads. Here a screen the reader does not recognise reads as no input
line, so a new design holds prompts, with the reason shown; it never types over text.

Considered:
- Keeping Ctrl+U under its checks (ADR 0022): with the hold off there is no count left to end,
  and the quiet pause alone types into a half-written line.
- Reading the line on a timer: the watch tells each print as it happens, and an idle agent costs
  nothing; the user asked for events.
- A time limit on the wait: it would type over text left in the line for longer, which the
  user's rule forbids.
- Sending to agents whose line cannot be read with the core's pause alone: it would type into
  what their person writes.
- Hanging up every send herdr-core does not answer at once: its answer comes only once the agent
  went working, a second or so after it typed, so a send typed just before the hang-up would be
  typed again. The client hangs up only on what someone did while herdr-core's pause after their
  key still runs, which the unanswered press of no key tells.

This needs herdr-core's hold behind unsent typing off (`draft_wait = 0`): with it on, a line
emptied with keys that hold no count (Esc Esc, Ctrl-W) reads empty while herdr-core still holds
the send, and the press of no key, which waits only for the quiet pause, is answered before
herdr-core types it; the send then waits up to 60 s, with no reason shown. One courier still
sends every prompt in turn, so text left in one agent's line holds prompts to the others too,
and a prompt withdrawn while it waits there is still typed once the line empties
(herdr-ruminate#24).
