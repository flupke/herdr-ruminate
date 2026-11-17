# Publish the review comments on herdr-bus

A review comment reached only the agent the reviewer woke. When the user wrote "@David" in a
round conversation, no other program saw it, and the mention never reached David, the
project's lead. herdr-bus is the machine's message bus, one SQLite file, and herdr-bowtie
routes the @mentions of the `herdr.words` messages published on it.

The reviewer publishes each new review comment on the bus's output `words`, as a
`herdr.words` message whose `to` is the pane of the agent it wakes. It publishes the comment
when the delivery pass that follows the post picks that agent (`State::prepare_notification`),
not when the post is saved, and from a thread of its own, so the bus never holds up the post
nor the wakeup. It takes herdr-bus by git, pinned in `Cargo.lock`.

- The pin moves only with herdr-bowtie's. A herdr-bus revision with a newer schema migrates the
  bus file the first time it opens it, and older builds then refuse the file: every program
  that opens the bus of a machine, on every machine, installs the new revision together.
  a5d7accc is the first pin, schema 2.

Considered:
- Publish when the post is saved, with no pane: herdr-bowtie could not tell which agent the
  comment was written to. Resolving the agent there too would cost a second call to Herdr
  before each wakeup.
- Deliver the comments to the agent through the bus as well: out of the ticket's scope, and
  the wakeup through Herdr already works.
- Pin herdr-bus a0ebd675, the first revision with herdr-words: the reviewer would stop opening
  the bus on the first machine where herdr-bowtie, on a5d7accc, migrated it.
