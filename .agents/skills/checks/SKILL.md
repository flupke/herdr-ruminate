---
name: checks
description: Load before running or debugging the checks (make lint, test, e2e, check-changed, mutations), a gate in a second jj workspace, or moving the Herdr pin.
---

# Checks

AGENTS.md lists the check levels (step 2) and says which gate a change runs (step 5). The
`Makefile` says what each target runs. This skill holds what neither says.

## Gotchas

- nextest runs no doc test. `make test` with `CRATES` runs none: only `make test` without it
  runs the doc tests, and `make check-changed` runs them only when it runs every check.
- `make check-changed` cannot tell a comment from code: a comment edited in a crate runs that
  crate's checks. `make check-changed CHECK_CHANGED_ARGS=--dry-run` lists the checks it would run.
- `make lint` fails on a tracked file outside every crate that no table of
  `crates/check-changed/src/plan.rs` names, such as a stray file jj tracked.
- `tests/tui` is a separate Cargo workspace, with its own `Cargo.lock`, that uses the
  workspace's crates. A search or a `cargo` command at the root misses it. Before you remove a
  public item, a derive or a dependency of a crate, search `tests/tui` for its uses too.

## The fake agent's build

The reviewer's tests run the program `reviewer-fake-agent`. Cargo builds it for a run of the
whole workspace (`cargo nextest run --workspace`, `make mutants`). `make test`, with or
without `CRATES`, `make e2e-tui` and `make vision` build it first
(`cargo build -p reviewer-fake-agent`). A bare `cargo nextest run -p reviewer` does not: it runs
the program built last, or fails when none was. Build it first. A build that names programs
with `--bin` must name it too (`--bin reviewer-fake-agent`): `--bin` selects among the
programs of every package given.

The workspace takes `fake-agent` from herdr-common by git, never by path, so that a clone of
this repository alone builds. `Cargo.lock` pins the revision: a change to the fake agent
reaches the tests once it is pushed and `cargo update -p fake-agent` moves the pin.

## The Herdr the tests run

The tests do not use the Herdr installed on the machine. The Nix shell takes the Herdr,
herdr-app and herdr-core (`hc`) releases that herdr-common's flake pins, and points
`TEST_HERDR_BIN_PATH`, `TEST_HERDR_APP_BIN_PATH` and `TEST_HC_BIN_PATH` at them. The private
Herdr servers run that binary. `herdr` on the `PATH` stays the installed Herdr, which
`make install` uses. Outside the Nix shell, the tests fall back to `herdr` on the `PATH`. To
try another binary, set `TEST_HERDR_BIN_PATH` inside the shell:

```sh
scripts/dev-shell env TEST_HERDR_BIN_PATH=/path/to/herdr make check
```

Herdr's documentation leaves much of its behaviour out, such as what it does with a pane's
OSC 52 clipboard write. Read its source at the pinned release instead (`herdr --version` in the
dev shell, or `herdrVersion` in herdr-common's `flake.nix`):
`git clone --depth 1 --branch v<version> https://github.com/herdrdev/herdr`. `src/pane.rs`
handles what a pane's program writes, and `src/selection.rs` the clipboard.

Herdr draws a run of blank cells that ends a row in the style of the cell before it, once that
cell is redrawn, though the reviewer never writes them. So a text selection that ends before
trailing spaces looks highlighted to the end of the line; the selected cells are right. A row
that ends in a background, such as a chip of the revision selector, needs a cell without
background after it. ratatui writes that cell only when it differs from the blank cell under
it: give it a foreground of its own.

Each private Herdr server detects agents with the rules in
`crates/review-test-support/agent-detection`, copies of Herdr's published Codex and Claude
rules, installed as local overrides. So the tests' fake agents reach the same states whatever
release runs them. The test `agents_are_detected_with_the_rules_the_repository_keeps` fails
when the pinned release does not use these files, for example when a manifest needs a newer
rule engine than the release has.

### Moving the pin

1. herdr-common pins the release. Once its `main` has the new release,
   `cd nix && nix flake update herdr-common`. To try a release before herdr-common has it, pass
   `--override-input herdr-common path:<a checkout of it>` to `nix develop`.
2. If the release changes the agent detection, copy the current Codex and Claude manifests from
   `distribution/agent-detection` in the Herdr repository into
   `crates/review-test-support/agent-detection`.
3. Run `scripts/dev-shell make check-with-e2e`.

## A gate in a second jj workspace

`/tmp` may be a RAM disk, and a full build needs about 16 GB. Put a second jj workspace or
checkout on disk, or point `CARGO_TARGET_DIR` at the main checkout's `target/`, before you run
a gate in it.

The flake lives in `nix/`, which `scripts/dev-shell` and `.envrc` load as a `path:` URL: Nix
copies that directory alone into the store. A bare `nix develop` at the root finds no flake.
`nix develop .` or a flake at the root would copy the whole jj workspace, `target/` included,
at each run (tens of GB each). Enter the dev shell through `scripts/dev-shell`:

```sh
scripts/dev-shell make check-changed
```

Keep work in another workspace visible to the user, who follows it from the main checkout with
`jj log` and `jj diff -r <bookmark>`:

- The supervisor names each agent's workspace and bookmark when it starts it.
- Describe the change at the start, with a provisional subject.
- Run `jj st` after each batch of edits: jj records a workspace's files only when a jj command
  runs there.
- Close each slice that works with `jj new`, so progress shows as described commits.

Each private Herdr server keeps its sockets, state and the link to the fake agent's program in
a private directory under `TMPDIR`, removed once the server stops. When several gates run at
once, a test that talks to the Herdr socket can fail with `WouldBlock` and pass alone.

## On a herdr machine

A workspace may live on a herdr machine, such as rayon-scout (Ubuntu 26.04, no window), which
lacks what the user's computer has (seen 2026-11-15):

- The tests of Codex fail with "Codex is not installed" (11 of `make test`): they run the
  `codex` of the `PATH`, installed with npm, which the dev shell does not provide.
- `make e2e-explore` stops before its first test: Chromium finds "No usable sandbox", as Ubuntu
  23.10 and later forbid unprivileged user namespaces to programs AppArmor does not name.
- `make e2e-tui`'s `a_prompt_typed_to_the_agent_moves_its_conversation_past_the_forks` waits
  30 s for a turn and fails when the suite runs in parallel there, on main too. It passes alone,
  and with `-j 1`, most of the time (on key-drum it also failed alone, once in three runs).
- Other agents' work loads the machine (a load average over 20 on 16 CPUs): `make e2e-tui`'s
  vision sessions may then fail at their start with `herdr-app ls failed ... the window did not
  answer`. Run them again with `-j 1`.
- rust-analyzer comes from the dev shell, so `review-lsp`'s tests pass there.

## Tests that call no model

Explore's tests in `make check` use deterministic responses and call no model API. So do the
Explore page's e2e tests (see the `explore-page-tools` skill). A check of Explore with a real
agent is manual, in a disposable repository and a private Herdr server. The checks that run
real Claude Code are in the `real-agent-checks` skill.

## Opt-in tests

None of these runs during `make check`.

### Real language servers

They run in temporary projects:

```sh
cargo test -p review-lsp --test language_servers -- --ignored
cargo test -p review-ui real_rust_lsp -- --ignored --nocapture
```

The server commands are in `crates/review-lsp/src/language.rs` and `typescript_server.sh`.

Run as written, they fail when Expert is not on `PATH` ("command 'expert' not found"), and the
TypeScript test with them. Put a Nix store build of Expert on `PATH` inside `scripts/dev-shell`,
whose shell resets `PATH`, and unset direnv's variables first, or `direnv exec` unloads that
environment, `PATH` with it:

```sh
env -u DIRENV_DIFF -u DIRENV_DIR -u DIRENV_FILE -u DIRENV_WATCHES scripts/dev-shell sh -c \
  'PATH=$(ls -d /nix/store/*-expert-*/bin | tail -1):$PATH cargo test -p review-lsp --test language_servers -- --ignored'
```

`expert_finds_aliased_callers_and_reports_the_changes_it_has_not_indexed` takes about a minute.

### herdr-core

The hc backend's tests against a real herdr-core start a private core each, with the dev
shell's pinned `hc` (`TEST_HC_BIN_PATH`), or else `hc` on the `PATH`:

```sh
scripts/dev-shell cargo nextest run -p hc-client --run-ignored only
scripts/dev-shell cargo nextest run -p review-thread-service --run-ignored only
```

### Jev evaluations

The `reviewer` feature `jev-evals` enables the Jev evaluator and its labelled corpus. Live
calls need both the feature and `--ignored`. Results go under `target/`: do not commit them.
The past studies and their results live in `docs/studies/`.

Validate the dataset, splitting and scoring without API calls:

```sh
cargo test -p reviewer --features jev-evals jev::evals
```

Export the requests without calling Jev (writes `planned.jsonl`):

```sh
JEV_EVAL_BUDGETS=14000 JEV_EVAL_REPEATS=1 \
  JEV_EVAL_OUTPUT="$(pwd)/target/jev-evals/offline-plan" \
  cargo test -p reviewer --features jev-evals export_recursive_requests -- --ignored
```

Run the comparison against Jev. These are paid requests to TypeSafe, with `TYPESAFE_API_KEY`
from the environment: ask the user first.

```sh
cargo test -p reviewer --features jev-evals jev::evals::live_compare_strategies -- --ignored --nocapture
```

## Trying the state merge on a copy

To see what the merge of per-workspace review states does to a real state without touching it,
`cp -a` the state root (`~/.local/state/herdr-progressive-reviewer`) to a scratch directory,
then run in the repository:

```sh
HERDR_PLUGIN_STATE_DIR=<copy> target/debug/reviewer-control stats
```

To try the move from Herdr's folder, copy `~/.local/state/herdr/plugins/herdr.progressive-reviewer`
to `<scratch>/herdr/plugins/` and run the reviewer with `XDG_STATE_HOME=<scratch>`.

## Mutation testing

There are three run levels:

- One file: the `cover-missed-mutations` skill.
- Changed code: `cargo mutants --workspace --test-workspace=true --test-tool=nextest --in-diff <diff>`
  tests only the mutants that overlap the diff. It misses coverage lost outside the diff, and a
  test-only change selects no mutants.
- The whole workspace: `make mutants`. Run it on a schedule or before a high-risk release, and
  read `mutants.out/` for its results.

A score of 100% is not the goal: equivalent and low-value mutants stay missed. The mutation
runs use nextest, which runs no doc test: the gate's doc tests stay the only check of those.
