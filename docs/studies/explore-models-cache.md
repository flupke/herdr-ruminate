# Claude Code's prompt cache across a model switch

Measured on Claude Code 2.1.329, on a fork of a 170k-token conversation, by reading the cache counts of each request in the transcript.

| Turn | Cache read | Cache written |
| --- | --- | --- |
| Same model as the conversation | 169.5k | - |
| After a switch to another model | 0 | 159.2k, to the 1-hour cache |
| Back on the first model, within the hour | 169.5k | - |
| Same model, effort changed only (Opus 5.5, high to xhigh) | 170.3k | - |

The prompt cache is per model. The turn after a model switch costs about twenty times the input of a cached turn, once per round when the agent is not already on the round's model. An effort change alone keeps the cache.
