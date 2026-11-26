# jj and gh

## jj

- The revset `change_id(<id>)`, like a bare change ID, finds a visible change only (jj 0.45).
- Walking a commit's evolution reads the operation log back to the change's creation: `jj evolog` costs about 7 µs per operation, measured on a repository of 3,000 operations (jj 0.45).
- A hidden commit stays readable until `jj util gc` (version not recorded).
- Two clones of one project share change IDs (version not recorded).
- jj cannot tell whose a described change is: only a working-copy commit names its workspace (version not recorded).
- `jj edit` onto another workspace's working-copy commit makes both workspaces snapshot their files into that one commit (version not recorded).
- Editing a change below another workspace's working copy rebases that working-copy commit, and leaves that workspace stale until its next jj command (version not recorded).

## Git

- A snapshot's tree stays readable until `git gc` prunes it (version not recorded).

## gh

- gh reads the repository from a Git checkout: in a non-colocated jj workspace, which has none, pass `--repo` (version not recorded).
- `gh pr list --head <branch>` matches the branch name alone, forks' branches of that name included, and takes no `owner:branch`; tell them apart with `isCrossRepository` and `headRepositoryOwner` (version not recorded).
- `gh issue view <number>` also answers for a pull request's number (version not recorded).
