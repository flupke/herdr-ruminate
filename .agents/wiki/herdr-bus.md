# herdr-bus

The reviewer publishes each text the user writes to an agent on herdr-bus (flupke/herdr-bus),
the machine's message bus, so that herdr-bowtie routes the @mentions in it: a `herdr.words`
message on the output `words`, which the reviewer declares when it starts
([ADR 0019](../adr/0019-publish-review-comments-on-herdr-bus.md),
[ADR 0020](../adr/0020-publish-every-text-the-user-writes-to-an-agent-on-herdr-bus.md)).

What goes on it, by the `place` of its `data`, with the review (`review`) and the round
(`round`, an Explore round's instance) beside the text:

| `place` | The text | Its `id` | Its `to` |
|---|---|---|---|
| `thread` | a review comment, in a review thread or a round conversation (`thread`) | the thread message's ID | the pane the next delivery pass picks for the wakeup (`State::prepare_notification`), or `null` when none takes it |
| `answer` | an Explore answer's comment, with its `question` and `choice`, or a reply to the conclusion (no `question`); `drawing_board` when it replies at the drawing board | the answer's ID | the pane of the agent the round selected for the turn |
| `implementation` | the lines the user wrote or changed in the list of Implement or Post review; none when they changed nothing | the request's delivery ID | the same |
| `start` | the user's name, given at Start for someone else's change | the kickoff's request | the same |

- A retry publishes once: herdr-bus keeps the first message of an `(output, id)`. An ID that is
  no UUID, as in a thread message saved by an older reviewer, goes as a `UUIDv5` of it.
- The Explore session publishes when the round saves a new turn (`ExploreSession::deliver_turn`)
  or an implementation request (`ExploreSession::implement`); a Retry publishes nothing again.
  Back to the drawing board has no text, and a choice without a comment publishes nothing.
- One thread, the thread service worker's, writes the bus file; the Explore session hands it its
  words through `PromptSender::publish_words`. A bus that fails never holds up the post or the
  prompt: the failure shows as a toast, and the next text opens the bus again. A text sent while
  the bus failed stays off it. Quitting waits at most a second for the texts not yet published
  (`EXIT_WAIT`), since herdr-bus waits up to 5 seconds on a locked file.
- Agent replies, the reviewer's own texts to the agent, and the reviewer notes file do not go on
  the bus.
- The bus file is `$HERDR_BUS_DB`, else `$XDG_DATA_HOME/herdr-bus/bus.db`
  (`~/.local/share` by default). Every test harness sets `HERDR_BUS_DB` to a file of its own
  (the private Herdr, the private herdr-core, the vision session's `bus.db`), and the unit tests
  give the worker a bus file in a temporary folder, or none: no test writes the user's.
  `PromptSender::flush` returns once the worker's bus published what it was handed, for a test
  that reads the file.
- `herdr-bus messages --output words --db <file>` lists what a session published, with a
  `herdr-bus` built at the pinned revision.
- `Cargo.lock` pins herdr-bus. A revision that changes its schema migrates the bus file the first
  time it opens it, and older builds then refuse the file: move the pin with herdr-bowtie's, and
  install both together on every machine.
