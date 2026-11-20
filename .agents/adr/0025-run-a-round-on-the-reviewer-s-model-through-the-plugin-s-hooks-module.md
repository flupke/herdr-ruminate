# Run a round on the reviewer's model through the plugin's hooks module

The user wants the best agent for Explore rounds: an agent herdr-bowtie started runs its round
on herdr-bowtie's reviewer's model and effort, and the implementation the round concludes is
done by a fresh subagent on the worker's, which the pane's agent supervises. An Explore round
runs in the agent's own conversation, in its pane, and Implement in the same conversation after
it.

The reviewer reads both settings through the agent's command line: the reviewer's from
herdr-bowtie's `bt-self-review.md` in a folder of its `--add-dir`, the worker's from its own
`--model` and `--effort`. Each round prompt names the round's model and effort on a line of its
own, and the hooks module of the reviewer's Claude Code plugin sends every request of the main
loop on them from then on (`turn.step`), until the process ends or a later round names others.
It reads the line only in a prompt whose whole first line is one the reviewer writes, and before
the reviewer's own text in it: every agent that loads the plugin, leads and workers too, may be
sent a text that quotes it. The agent never switches back: Implement runs on the round's
model too, and its prompt asks for one subagent through the Agent tool, with the worker's model
alias and effort, briefed and then reviewed by the pane's agent.

Considered:
- Typing Claude Code's `/model` and `/effort` into the pane before the round's first prompt. Claude
  Code 2.1.329 saves each as the user's default for new sessions in `~/.claude/settings.json`, and
  `/model` opens a confirmation dialog that takes the next typed text whenever the cache may be
  warm. Restoring the file after each switch would race when two rounds start at once.
- Compacting the conversation and running the round in a fork on another model: dropped by the
  user for this simpler switch.
- A subagent type the module registers on the worker's exact model ID (`$.agent.register`),
  measured to work: the user found a plain sentence naming the alias enough, and the Agent tool's
  own `model` takes only an alias.

A switch to another model re-reads the whole conversation uncached on the round's first turn
(measured: 159k tokens written instead of 170k read), once per round when the agent is not
already on the round's model. An alias runs its family's current model, which may be newer than
the worker's ID. The module loads at the agent's start: an agent started before the plugin had
it runs its rounds on its own model.
