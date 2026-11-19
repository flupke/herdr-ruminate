# Keep the reviewer off other workspaces' working-copy commits only

Several jj workspaces of one repository often share a stack: the user reviews in one workspace
while an agent works in another, whose working copy sits on top of the user's changes. On 11-17
the reviewer's revision keys and its "Select revision" picker (`]v`, `[v`, `vv`) and the check
before `jj edit` started to leave out every change another workspace's working copy reached and
this workspace's did not (commit 8deeb19b). On 11-19 the user found the picker empty above @: in
their workspace @ sat at the bottom of their own stack of three changes, with another workspace's
working copy on top, and the reviewer no longer let them step up to review those three. They
called it a bug, and the lead, in the user's place, chose this rule.

The reviewer leaves out the other workspaces' working-copy commits themselves, and nothing else
(`working_copies() ~ @`, `other_workspaces_revset!` in
`crates/review-repository/src/repository/jj.rs`). Every other mutable change is offered and
editable, the changes between this workspace's @ and another workspace's working copy among
them. An edit of another workspace's working-copy commit is refused, and the pane names that
workspace.

Considered:
- Leaving out the changes that only other workspaces' working copies reach as well, the rule of
  8deeb19b: it hides the user's own stack whenever an agent builds on it, and which changes it
  hides follows @, so stepping down with `[v` loses the way back up. jj cannot tell whose a
  described change is; only a working-copy commit names its workspace.
- No check at all, as before 8deeb19b: `jj edit` onto another workspace's working-copy commit
  makes both workspaces snapshot their files into the same commit, and the other agent's files
  change under it.

Editing a change below another workspace's working copy rewrites nothing by itself. An edit the
user then makes there rebases the other workspace's working-copy commit, as any jj descendant, and
leaves that workspace stale until its next jj command updates it; the same holds when the user
edits that change without the reviewer.
