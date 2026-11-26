# Run-ahead in a vision session

## Turn it on and watch it

- In the Explore tab, `key` `z` twice before the round starts: the pane shows `Run ahead
  (experimental): every choice`. Once shows `the recommended choice`.
- After a question, `forks` with `count` waits until that many forks started. It lists each
  fork's session, choice, prompt (its access and Answer ID are among the turn's files) and how
  far it got, the agent's session, and run-ahead's log (`run_ahead_log` in `start`'s result,
  `state/run-ahead.log` in the session directory).
- `forks` lists forks in the order they reported starting, which varies from run to run. Find
  a fork by its `choice`.
- `forks` gives the agent's session as the fake agent reported it once its `SessionStart` hooks
  ran: read it after the action you wait for.
- `fork_submit` plays a fork: it submits with the fork's own access, and the fork's output
  shows the call and its result.
- Then `key` `1`, `ctrl+enter` answers bare: the pane and the Explore page show `Prepared while
  you were thinking`, and `forks` shows the agent on the fork's session.
- An answer with a comment shows `Not prepared: you added a comment`.
- A bare answer before `fork_submit` waits for the fork: the pane shows `The agent is
  finishing a turn it started while you were thinking`. `fork_submit` then shows the fork's
  turn; `Stop waiting` gives the answer to the agent, whose next `turn` returns it.
- The arguments of `tests/tui/examples/conclusion.json` make a fork's turn a conclusion.
- A fork's question must be on a topic not decided yet: the reviewer refuses one on the topic
  just answered (`Do not ask again on a topic just decided`). A topic's `status` is `open`,
  `accepted` or `needs_follow_up`.

## Script the forks and the switch

Per question, once `forks` listed a fork, choose what it does:

- submits its turn: `fork_submit`;
- still works at the answer: nothing; the bare answer waits for it;
- ends without a turn: `fork_end`; crashes: `fork_end` with `crash`.

Each end and crash counts a failure, as a failed switch does. The third in a row stops
run-ahead for the round, with a toast. Only a prepared turn used resets the count.

Before the answer, choose what the switch to a fork does: `switch_unheard` makes the fake
agent's next `/resume` unheard, so the switch fails after 20 s and the turn waits for Retry
(`r`). With `heard_after_ms` it is heard that late and succeeds. Stop waiting and Cancel
answer play while it waits.

## Reach each `Not prepared` line

The same in the pane and on the Explore page:

| Line ends with | How |
| --- | --- |
| you added a comment / you answered None of the above | the answer itself |
| you sent the agent back to the drawing board / you wrote in the chat | sent from the Explore page, which alone has both |
| the agent was working | `agent_working` true before the answer, false after it |
| the agent's conversation moved on | `agent_prompt` after the forks started |
| the lines left to review changed | a new changed line in the scratch repository |
| nothing was prepared for this question | run-ahead stopped for the round, or the agent working when the question came, or `start` with `agent_hooks: false` |
| only the recommended choice is prepared | setting `the recommended choice`, another choice answered |
| the turn for this choice did not finish | `fork_end`, before the answer or while it waits |
| you stopped waiting for it | Stop waiting while the answer waits for its fork, or during the switch after it |
| the agent could not switch to the prepared turn | `switch_unheard`, then Retry |
| you stopped waiting while it was being used | `switch_unheard` with `heard_after_ms`, Stop waiting during that switch, then Retry |

No session reaches `restart the agent to load … plugin`, `the round changed after it was
prepared` or `the turn for this choice was not ready yet`.
