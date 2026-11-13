# Carry review marks along jj's record of predecessors

A review's marks live under its change ID (`changes/<change id>/`), and each record names that
change. jj gives the code a new change ID when it duplicates a change, for the second half of a
split, and for the code a squash moves into another change. The reviewer then showed the new
change at 0%, though the user had reviewed its code. In one case, the user reviewed kpurqwpk,
an agent made lzurnpru with `jj duplicate`, renamed a function in it and abandoned kpurqwpk.
All 40 of kpurqwpk's marks stayed in the state, and lzurnpru showed 0%.

A change that never had a review mark, with no `marks-stamp` and no record, takes the marks of
its origins when the reviewer reads its states (`ReviewTracker::statuses`), at each read until
it has marks. Its
origins are, on each line of its evolution, the nearest change with marks. The evolution is
jj's record of the predecessors of each commit, which `jj evolog` shows and jj-lib's
`walk_predecessors` walks.

- The marks are copied, under the new change's marks lock. Each copy keeps its baseline commit
  or its partial base, so lines rewritten since show as changed since review. After the copy,
  the change owns its marks like any other.
- A path that several origins marked takes none, and stays unreviewed.
- A change with marks of its own, or whose marks were cleared, takes none. A line of the
  evolution goes past a change that never had marks, and ends at one whose marks were
  cleared, so marks a reviewer reopened never come back from further down.
- The evolution of each commit read is kept, since it never changes. Which changes have marks
  is read again each time.

Considered:
- Guess the origin from file similarity: a heuristic, which can give marks to unrelated code.
  jj records the predecessors exactly, and jj 0.45 and 0.47 record a duplicate's source.
- Read the origins' marks through a link on each read, instead of copying them: every read of
  a change would walk the operation log, and a mark made on the new change would have to
  shadow its origin's.
- Leave it to the user to copy the records: the user has no tool for it, and each record names
  its change, so a plain copy reads as unreviewed.
