---
name: real-agent-checks
description: Load before running or planning a check with real Claude Code or Codex (the run-ahead and herdr-core contract checks, a real Explore round), after an upgrade of Claude Code, Herdr or herdr-core, or a change to forks, prompt delivery or the Explore models.
---

# Checks with real agents

The Claude Code checks below are paid: they run real turns on a subscription. Ask the user for
their go before each run, and say what it costs. Run each one once per upgrade or change, not
once per ticket. The rehearsals and unit tests guard the rest of the time.

## The run-ahead contract check

The run-ahead tests play Claude Code with the fake agent. The bodies of
`crates/reviewer/src/runtime/run_ahead/contract.tests.rs` check what the fake agent assumes of
Claude Code, each step naming the assumption it rests on (`Assumption`). Each body runs twice:

- its rehearsal (`rehearse_*`) runs the fake agent, in `make check`, at no cost. It finds the
  faults of the test itself, but not a timing fault that only real Claude Code's pace brings out.
- its contract check (`real_claude_code_*`), ignored by default, runs real Claude Code in a
  private Herdr's pane, at `claude-sonnet-5-5`: about ten turns on the subscription of the
  token's account. A failure names the assumption that broke, and prints the agent's pane.

The contract check has not had a real run yet: its first needs a token from
`claude setup-token`.

Run it after an upgrade of Claude Code or of Herdr, or a change to the fork's command line or
its settings (`claude-fork`'s `arguments.rs`, `pane.rs`, `host.rs`, `settings.rs`,
`stream.rs`), or to the switch and the settling (`claude-fork`'s `switch.rs`,
`review-explore-session`'s `run_ahead/switch.rs` and `run_ahead/settle.rs`).

```sh
claude setup-token   # once: a token of the account whose subscription pays
export CLAUDE_CODE_OAUTH_TOKEN=<the token>
cargo build -p reviewer --bin reviewer-control -p review-mcp-config --bin reviewer-mcp -p reviewer-fake-agent
cargo nextest run -p reviewer real_claude_code --run-ignored only
```

The agent and its forks log in with that token and run in a configuration of their own under
the test's Herdr directory (`CLAUDE_CONFIG_DIR`, and `XDG_CACHE_HOME`). The check never reads
nor writes `~/.claude` nor `~/.cache`. Never copy the user's credentials there. They take none
of the test's `CLAUDE*` and `ANTHROPIC*` variables but the token. What the check still touches:
the user's `claude` program, run as it is with its updater off, the token's account, and
`~/.local/share/applications/claude-code-url-handler.desktop`, which Claude Code writes when it
is missing.

## The contract check under herdr-core

`crates/reviewer/tests/claude_code_under_hc.rs` checks that real Claude Code takes the
reviewer's prompts through herdr-core as the reviewer expects. It runs the user's `claude` on
the user's login and configuration, with the reviewer's plugin set to this build's
`reviewer-control` by `--settings`: install the plugin first (`make install`). It needs no
token. It leaves in the user's configuration the session of each run, and the trust of its
folder, `~/.cache/herdr-reviewer/hc-contract-check`. It runs their other plugins, hooks and MCP
servers, on a private core. Six Sonnet turns.

Run it after an upgrade of Claude Code or herdr-core, or a change to the prompts' delivery
(`review-thread-service`'s `delivery.rs`, `hc-client`'s `send`, `agent-hooks`'s expectations):

```sh
scripts/dev-shell cargo nextest run -p reviewer --test claude_code_under_hc --run-ignored only
```

It passed with Claude Code 2.1.318 and herdr-core 0.1.18, and with Claude Code 2.1.327 and
herdr-core 0.1.20.

## Run-ahead with real Claude Code under herdr-app

Checked once with herdr-app 0.1.13, herdr-core 0.1.18 and Claude Code 2.1.318. Run it again
after a change to the forks or the switch. About ten Sonnet turns on the user's subscription.
The private herdr-app is in the `visual-testing` skill.

1. `make build`, then herdr-common's `app-sandbox start --plain --size 2400x4000 <name>` (plain:
   Claude Code needs the network; tall, so the pane shows a question's choices), and
   `HERDR_APP_HOME=<its app_home> scripts/herdr-app-mod install`. Run the `herdr-app` and `hc`
   commands below as `app-sandbox herdr-app <name>` and `app-sandbox hc <name>`.
2. Write `<its directory>/state/herdr-progressive-reviewer/settings.json` with
   `"explore_round":{"run_ahead":"recommended"}` and `"explore_page":{"pane_starts":"in_pane"}`.
3. Make a scratch jj repository with one change, then `herdr-app do space.new '{"folder": ...}'`.
4. Type the agent into its terminal with `hc send`: `env` with the `CLAUDE*` variables unset and
   `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` unset, then
   `claude --settings <file>`, the file holding `pluginConfigs` with this checkout's
   `bin/reviewer-control` as the plugin's `control`. Accept the folder's trust with
   `hc keys <t> down enter`. The sandbox's terminals have a `HOME` of their own: set `HOME` to
   yours, and `CLAUDE_CONFIG_DIR` to the configuration whose account pays, when it is not
   `~/.claude` (an account at its weekly limit answers no turn). For an agent in Code Mode, add
   `CLAUDE_CODE_EXPERIMENTAL_CODE_MODE=1`: no fork starts, and the pane says why. Quit the agent
   with `/exit`.
5. `herdr-app do mod.open '{"name":"herdr-progressive-reviewer"}'`; in the pane, `e`, then `p`
   (Start in the pane). Once `run-ahead.log` in the private state says the fork submitted,
   `end`, `1`, `ctrl+enter`: the pane shows the next turn with `Prepared while you were
   thinking`. Reset, to start a round again, takes two clicks within 5 s on the pane's Reset
   corner (`app-sandbox run <name> xdotool mousemove <x> <y> click 1`, at the pixels of `shot`),
   the second on Confirm reset. A new build reaches the round once the pane is closed
   (`pane.close`) and opened again: the round goes on.

## A real round after a change to the Explore models

The free tests come first. The hooks module's own tests (`plugin/hooks/register.test.ts`) run in
Claude Code's test kit, `claude plugin test`, which `crates/claude-hooks/tests/hooks_module.rs`
runs in `make test`: it needs the user's `claude` on the PATH. They switch on the prompts in
`plugin/hooks/round-prompts.ts`, which the runner test
`the_hooks_module_s_tests_switch_on_the_prompts_the_reviewer_sends` holds to the prompts the
reviewer writes. After a change to a prompt, run that test with `BLESS=1` to write the file
again; the module's tests then say whether the module still knows the prompt's first line.
`tsc -p crates/claude-hooks/plugin` type-checks the module only once Claude Code loaded the
plugin from this folder, which writes its types under `.claude-plugin/types/` (ignored), so
`make lint` does not run it.

These tests cannot prove what Claude Code does with the prompts. For that, run one real round
in a private herdr-app (`visual-testing` skill), with `--plain`. About thirty Opus requests and
a few Sonnet ones on the account's subscription.

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

## A Codex TUI in a test

These run a real Codex TUI but call no model API, so they are free and run in `make test`:
`crates/review-mcp-config/tests/codex_daemon.rs`, `crates/reviewer/tests/codex_hooks.rs` and the
run-ahead tests of `crates/reviewer/src/runtime/run_ahead/codex.tests.rs`. Each runs the TUI in a
private Herdr server, on an unmanaged app server and a scratch home, with a local model fixture
(`review_test_support::codex`). They need `codex` on the `PATH`. Codex needs no contract check:
these tests run the real program.
