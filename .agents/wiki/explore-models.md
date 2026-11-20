# Explore models

Which model and effort an Explore round and its Implement run on, how the reviewer sets them in a
running Claude Code, what Claude Code 2.1.329 did when measured, and how to check a change to
them. The decision is [ADR 0025](../adr/0025-run-a-round-on-the-reviewer-s-model-through-the-plugin-s-hooks-module.md);
the code is `crates/review-explore-model` and the plugin's `crates/claude-hooks/plugin/hooks/register.ts`.

## What runs on what

- herdr-bowtie starts a worker with its role's `--model` and `--effort`, and `--add-dir` a folder
  whose `.claude/agents/bt-self-review.md` names its reviewer's `model` and `effort` in its front
  matter. The reviewer reads both through the agent's command line (`pane.process_info`'s argv, or
  `/proc/<pid>/cmdline`; a repeated option counts by its last value, as Claude Code takes it).
  An agent without that file, or not Claude Code, gets none of what follows: its prompts name no
  model, and it implements a conclusion itself.
- Each round prompt, the forks' too, ends with `Explore runs on <model> at <effort> effort.` on a
  line of its own. The plugin's hooks module reads it at `prompt.submit`, only in a prompt whose
  whole first line is a round turn's, an Implement's or a Post review's as the reviewer writes
  them, and only before the reviewer's own text (`Tasks from the reviewer:`, `Review from the
  reviewer:`): leads and workers load the plugin too, and words a person types, a report or a
  pasted log that quotes the line must switch nothing. From then on it rewrites the `model` and
  `effort` of every request of the main loop (`turn.step`, subagents left alone).
- The switch ends only with the process, or a later round that names other settings, which the
  session then runs on. There is no switch back: Implement stays on the round's model, and so does
  every later prompt of the session, a round's or not, the user's own included. The Implement and
  Post review prompts carry the line too, since a plugin reload (`/reload-plugins`, or a hot
  reload of a `--plugin-dir` folder) empties the module's memory; a round conversation's wakeup
  does not, and after such a reload its reply runs on the agent's own model.
- The Implement prompt, when the worker's model has an alias, adds a paragraph: one fresh
  subagent, started with the Agent tool's `model` (the alias) and `effort` (the launch
  `--effort`), does the work from a whole brief, and the pane's agent reads its diff and checks
  its validation before its report.
- An agent started before the plugin had its module runs every round on its own model: restart it.

## How Claude Code behaves (2.1.329)

- `/model <id>` and `/effort <level>` typed in a running session save their value as the user's
  default for new sessions, in `~/.claude/settings.json` (`model`, `effortLevel`): no typed form is
  session-only. `/model` in a conversation whose cache may be warm opens a "Switch model?" dialog
  that takes the next typed text; Enter or `1` confirms it. Never type them in a test run on the
  user's configuration.
- A hooks module's `turn.step` rewrite of `model` and `effort` writes no setting and opens no
  dialog (settings file checksum unchanged). A `claude -p --resume <s> --fork-session` prompt that
  carries the line runs at its effort too; the fork's model comes from the transcript's last
  answer, already the round's.
- The Agent tool's `model` takes only `sonnet`, `opus`, `haiku` or `fable`: a full ID is refused
  (`Invalid option`). An alias runs the family's current model. Its `effort` takes the five
  levels. A module's `$.agent.register` type takes a full ID, and is dispatched in the turn of the
  prompt that registered it, but the Agent tool's own parameters were enough.
- The prompt cache is per model. Measured on a fork of a 170k-token conversation: on the same
  model a turn read 169.5k from the cache; after a switch to another model it read none and
  wrote 159.2k to the 1-hour cache, about twenty times the input cost of a cached turn; back on
  the first model within the hour it read 169.5k again. An effort change alone (Opus 5.5, high
  to xhigh) kept the cache: 170.3k read.
- The transcript records each request's model and effort: the `api-request` entries' `params.model`
  and `params.output_config.effort`, a subagent's under `<session>/subagents/agent-*.jsonl` with
  its `querySource` (`agent:builtin:general-purpose`).

## Checking a change to it

The module's own tests (`plugin/hooks/register.test.ts`) run in Claude Code's test kit,
`claude plugin test`, which `crates/claude-hooks/tests/hooks_module.rs` runs in `make test`: it needs
the user's `claude` on the PATH. They switch on the prompts in `plugin/hooks/round-prompts.ts`,
which a runner test (`the_hooks_module_s_tests_switch_on_the_prompts_the_reviewer_sends`) holds to
the prompts the reviewer writes: after a change to a prompt, run it with `BLESS=1` to write the
file again, and the module's tests then say whether the module still knows the prompt's first
line. `tsc -p crates/claude-hooks/plugin` type-checks the module only
once Claude Code loaded the plugin from this folder, which writes its types under
`.claude-plugin/types/` (ignored), so `make lint` does not run it.

The unit and behaviour tests prove the prompts: `review-explore-model`'s command lines and agent
files, the runner's prompt lines, the session's on the in-memory host
(`set_command`), and the fake agent's in a private Herdr (`FakeAgent::HerdrBowtie` in
`run_ahead.tests.rs`). They cannot prove what Claude Code does with them. For that, run one real
round in a private herdr-app ([herdr-app](herdr-app.md#a-private-herdr-app)), with `--plain`:

1. A scratch jj repository with one small change, and a folder holding a `bt-self-review.md` on
   another model than the agent's (Opus 5.5 at high, the agent on Sonnet 5.5 at low).
2. In the space's terminal, `claude` through `env` with the scratch XDG folders unset,
   `HOME` and the `CLAUDE_CONFIG_DIR` of the account the session runs under, `--model` and
   `--effort`, `--add-dir <that folder>`, `--plugin-dir <checkout>/crates/claude-hooks/plugin`,
   and `--settings` holding `enabledPlugins` with the installed `progressive-reviewer@...` off and
   `pluginConfigs."progressive-reviewer@inline"` with this checkout's `bin/reviewer-control`.
3. `mod.open`, `e`, `p`; answer each question with its number and `ctrl+enter`; `ctrl+enter` in the
   conclusion's editor sends Implement.
4. Read the transcript: every `api-request` of the round on the reviewer's model and effort, the
   subagent's on the worker's, its Write calls, then the pane's agent's `jj diff` and tests. Check
   `~/.claude/settings.json` is unchanged (checksum before and after).

About thirty Opus requests and a few Sonnet ones on the account's subscription.
