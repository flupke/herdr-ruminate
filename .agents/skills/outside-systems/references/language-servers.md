# Language servers

## rust-analyzer

- A client that does not offer `workspace/didChangeWatchedFiles` leaves rust-analyzer on its own file watcher (version not recorded).

## Expert (Elixir)

- Expert builds and indexes its project after it answers `initialize`; navigation returns nothing until then, and it shows "Search index is loading" (version not recorded).
- Expert's watchers are `**/*.{ex,exs}` and `**/mix.lock` (version not recorded).
- A filesystem event only removes a deleted file from Expert's index: a file changed on disk while closed stays out of the index until Expert restarts (Expert 0.1.11, and its main branch in October 2026).
