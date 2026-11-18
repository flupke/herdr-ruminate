# End the unsent typing that holds a wakeup with Ctrl+U

herdr-core 0.1.20 and later hold a `terminal.send` while they count a person's unsent typing in
the terminal: the characters typed less the Backspaces, until Enter, Ctrl-C or Ctrl-U, up to 60
s after the last key. Claude Code empties its input line with keys that leave that count, as Esc
Esc, Ctrl-W, Delete, or its vim mode's keys. On 11-18 a review comment's wakeup waited so for
134 s, its agent idle and its input line empty, and the user had to type in the agent's pane to
get an answer. herdr-bowtie met the same hold that evening, and chose to end it with Ctrl+U
under checks that nobody may be typing (its ADR 0402). The lead, in the user's place, had the
reviewer reuse those checks.

While a send to a Claude Code agent waits, a wakeup or any other prompt the reviewer types, its
herdr-core client looks every 5 s whether nobody may be typing in the agent's terminal, and then
presses Ctrl+U there through herdr-core, which ends the count and erases nothing from an empty
line; herdr-core then types the waiting send at once (`crates/hc-client/src/unsent_typing.rs`).
The checks, each read right before the press, in this order: the agent shows idle; the keyboard
of the herdr-app window the reviewer follows is not in its terminal, a view or a layout of
herdr-app's that the reviewer cannot read counting as the keyboard there; its input line, read
from the screen (`crates/claude-input-line`), shows nothing; and, last, nobody pressed a key in
the terminal for herdr-core's quiet pause, which a press of no key tells. When one fails, the
send waits on, and the next look checks again. The send itself is never given up nor sent again,
so a prompt is never typed twice. A Codex agent, whose composer the reviewer does not read, gets
no key.

This reads the agent's screen, which ADR 0005 moved run-ahead off because a new design of Claude
Code's screen breaks such reads. Here a screen the reader does not recognise reads as not empty,
so a new design only brings the old hold back; it never sends a key over text.

Considered:
- Leaving the hold: the wakeup waits up to 60 s past the user's last key in the agent's pane,
  and longer while they keep typing there, and the user takes it for lost.
- herdr-bowtie's way, a send with a 6 s timeout, then Ctrl+U and a second send: it relies on
  herdr-core cancelling a send whose client hung up (0.1.20 and later), and a send that times out
  after it typed would type twice. A Ctrl+U beside the waiting send ends the same hold with one
  send (measured on hc 0.1.21: the held send typed 0.2 s after the key).
- A flag of herdr-core's `terminal.send` that skips the count for a caller that read the line
  empty: the right fix, but herdr-core is upstream, and nothing goes there without the user's
  word.
- The window's keyboard focus as the only check: it does not see `hc attach`, another window, or
  a key typed just before the keyboard left.

Only a key typed in the moment between the last check and the press could make herdr-core hold
the Ctrl+U until its person pauses, and then erase what they typed.
