# Publish every text the user writes to an agent on herdr-bus

Extends [ADR 0019](0019-publish-review-comments-on-herdr-bus.md), which published only the
review comments. The texts the user writes in an Explore round reached the agent but no other
program, so herdr-bowtie did not route the @mentions in them. The reviewer now publishes each
text the user writes to an agent on the bus, as ADR 0019 does a review comment: a `herdr.words`
message on the output `words`, whose `id` is the ID the reviewer saved the text under, `to` the
pane of the agent it goes to, and whose `place` says where the user wrote it:

- `thread`: a review comment, as ADR 0019 says.
- `answer`: the text of an answer in an Explore round, a choice's comment or a comment alone,
  or of a reply to the conclusion. Back to the drawing board sends no text of its own.
- `implementation`: the lines the user wrote or changed in the list they sent with Implement or
  Post review, which the agent's `to_be_implemented` fills in first: the agent's own lines stay
  off the bus, and an unchanged list publishes nothing.
- `start`: the user's name, the one text the user types at the start of a round, for someone
  else's change.

The Explore session publishes a new turn's text once the round saved it, to the agent the round
selected for it, and the list once the round saved the request. It hands them to the thread
service's worker, through its `PromptSender`, which owns the one bus thread: one declaration of
`words`, one toast when the bus fails. A Retry publishes nothing again. A cancelled answer stays
published: the bus is a log.

Not published: the passage of the round a chat message quotes, which the agent wrote; the
earlier rounds' answers that a later round's instructions quote again; the reviewer notes file
and the change's description, written outside the reviewer.

Considered:
- Publish an answer when its prompt starts delivery (`DispatchObserver::before_attempt`), as
  the exact pane: an answer that a run-ahead fork takes sends no prompt, and would go unpublished.
- Publish the whole list of Implement, or the whole list once the user changed it: the agent's
  own @mentions would be routed as the user's.
- Give the Explore session a bus thread of its own: two writers of one file, each declaring the
  output and toasting its failures.
