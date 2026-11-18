# Save a round's change references beside it

The Explore page names a past round by the pull request or the ticket of the change it
reviewed. The reviewer finds them with jj and `gh` once the round starts, on a thread of its
own, because `gh` can be slow, missing, logged out or offline, and the start must not wait for
it. The thread ends at any time after the round's record was created, while the session goes
on saving that record.

The reviewer saves what the thread finds in a file of its own beside the round,
`<round>.references.json` in the review's Explore folder, and never in the round's record.
The past-rounds list reads it with the round's heading; a round without the file, saved before
or still being looked up, keeps its description's first line as its name.

Considered:
- A new optional field of the saved round, as the ticket suggested: the thread would write
  the round through the session's locked update, which raises the round's revision under the
  session's feet, writes a historical round that the session refuses to change, and moves the
  file's time, which the list shows as when the round was saved.
- Hand the result to the session's inbox, which then saves it in the round: the same revision
  and history problems, and the session would carry state that only the past-rounds list reads.
