# Progressive review

A review tracks the examination of changing code and the discussions about it.
Review progress and those discussions remain meaningful across code changes.

## Language

- **Review**: one logical unit of code being examined, whose identity continues across changes to that code. _Avoid_: review session.
- **Review checkpoint**: a particular version of the code within a review. _Avoid_: review.
- **Review thread**: a discussion about a code selection within one review, or the round conversation of one Explore round, which stays part of the review when its code changes or disappears. _Avoid_: agent thread, conversation without qualification.
- **Text selection**: the text the reviewer drags over in the pane with Alt held, copied to the clipboard without gutters and borders and with wrapped lines joined back. _Avoid_: selection without qualification (the code a review thread is about), copy mode.
- **Thread message**: a posted contribution to a review thread, written by the reviewer or an agent.
- **Review comment**: a thread message containing the reviewer's question or feedback. _Avoid_: agent prompt.
- **Agent reply**: a thread message from an agent responding to the conversation, possibly to several review comments.
- **Draft**: review-comment text that the author is still composing, not yet part of the shared review thread. _Avoid_: unposted text, queued comment, saved-but-unsent comment.
- **Posting**: the reviewer's publication of a comment to a review thread. _Avoid_: saving a draft.
- **Agent session**: one ongoing conversation with a coding agent, which may address several review threads. _Avoid_: review thread, review session.
- **Review mark**: the record that some changed lines of a file are reviewed, and who marked them: the reviewer, Jev or the Explore agent. _Avoid_: coverage, credit.
- **Reviewed version**: a file as the reviewer has read it: the base with the changed lines accepted as reviewed. _Avoid_: baseline (the commit a mark was made at).
- **Unreviewed lines**: the changed lines of a review checkpoint that no review mark covers. _Avoid_: gaps, uncovered regions, remaining coverage.
- **Jev**: the classifier that marks changed lines too insignificant to need a reviewer's attention.
- **Open hunk**: a change from the reviewed version to the current file, still waiting for review ("changed since review" when it rewrites reviewed lines).
- **Reviewed hunk**: a change from the base that the reviewed version holds and the current file still matches, folded into one row of the diff.
- **Linear issue**: an issue of the Linear tracker that the reviewed change names by its identifier, in its description, bookmarks or jj workspace name; one kind of ticket. _Avoid_: task.
- **Ticket**: the issue of a tracker that a reviewed change is for: a GitHub issue, herdr-bowtie's `t<N>-<slug>` bookmark or workspace, or a Linear issue. _Avoid_: task, issue (alone).
- **Change references**: the pull request and tickets of the change an Explore round reviewed, found with jj and `gh` once the round starts and saved beside the round. _Avoid_: links, metadata.

## Explore

- **Explore round**: one interview in which an agent questions the reviewer about a review checkpoint, and marks or reopens changed lines according to the answers. _Avoid_: Explore pass, Explore session.
- **Design explanation**: what the first turn of an Explore round explains before its first question: a thesis for the change, then four parts (what and where, types and data flow, algorithm and cost, rejected alternatives). _Avoid_: map.
- **Thesis**: the one sentence you would say at a whiteboard, for the whole change and for each part of its design explanation. _Avoid_: summary, lead paragraph.
- **Writing style**: how the Explore agent writes every text the reviewer reads in a round: plain, Simplified Technical English or concise, fixed when the round starts. _Avoid_: tone, language.
- **Challenger**: a subagent with fresh context that reviews the same change without the implementer's knowledge, taking turns with it to propose the round's questions. _Avoid_: adversary, second reviewer, critic.
- **Challenger proposal**: a question the Challenger proposes for a turn, and what became of it: asked, merged, retired or kept for later. _Avoid_: suggestion, Challenger question.
- **Citation**: a path, side and line range that a question puts before the reviewer as evidence, most decisive first; it marks nothing reviewed. _Avoid_: source reference, inspection, supporting reference.
- **Quiz**: the whiteboard-level questions, one per key aspect of the change, that the Explore agent submits with its conclusion, each with its correct answer and proof. _Avoid_: test, exam, check.
- **Glossary**: the terms of an Explore round's change, each with a one-sentence definition, which the Explore page marks in the text and lists after the design. _Avoid_: dictionary, definitions, terms list.
- **Callout**: a quote in the agent's Markdown that opens with a marker such as `[!TIP]`, shown as a block titled with its kind. _Avoid_: alert, admonition.
- **Diagram**: a fenced `mermaid` block in the agent's Markdown, which the Explore page draws with Mermaid. _Avoid_: chart, figure.
- **Status mark**: a good, bad or warning mark at the start of a table cell in the agent's Markdown, written as a marker such as `[!good]`. _Avoid_: badge, emoji.
- **Not relevant**: changed lines the Explore agent read that hold no decision for the reviewer, marked reviewed without a question, with a reason. _Avoid_: skipped, ignored, insignificant (Jev's judgement).
- **Round conversation**: the review thread of one Explore round, where the reviewer asks, challenges or adds context beside the round's questions without answering them; the Explore page calls it the chat. _Avoid_: chat thread, agent chat (in the domain model), Explore turn.
- **Revised question**: a higher version of the open question that the agent posts when a message of the round conversation made the question wrong. _Avoid_: replaced question, edited question, re-ask.
- **Cancel answer**: withdrawing one of the reviewer's answers with every answer after it, which returns the round, review marks included, to its state before that answer. _Avoid_: cancel (which stops a pending turn), correction, undo, defer.
- **Back to the drawing board**: the reviewer's answer when they disagree with the change's design itself, which makes the agent conclude at once with the changes the design needs. _Avoid_: redesign, reject, start over (which is Reset).
- **Explore page**: a browser page, served by the open review pane behind a token, that shows the current Explore round and lets the reviewer run it as in the pane. _Avoid_: web UI, Explore web, browser view.
- **Round on the page**: an Explore round started with Start or Start with Challenger, or on the Explore page, whose interview the pane does not show until the reviewer chooses Continue. _Avoid_: web session, web round.
- **Round rail**: the steps of an Explore round as the Explore page lists them: the design explanation, each question (Q1, Q2 …), the quiz and the conclusion, each done, current or later. _Avoid_: stepper, breadcrumb, progress bar.
- **Earlier question**: a question of the current Explore round that the reviewer already answered, shown read only as a done step of the round rail. _Avoid_: previous question, history, earlier round, past round.
- **Past round**: a saved Explore round of the repository other than the one the Explore page follows, which the page on this machine lists and opens read only. _Avoid_: archive, old round, earlier round.
- **Meter**: the line under the Explore page's masthead that shows the share of the changed lines review marks cover, split by who marked them. _Avoid_: coverage, progress bar.
- **First pick**: the choice the reviewer picks on the Explore page before it shows the agent's recommendation, on a question whose Door is one-way, mixed or unknown; also called the blind first pick. _Avoid_: initial answer, draft choice.
- **Someone else's change**: an Explore round on a change the reviewer did not write, reviewed for its author, whose conclusion is a review to post on its pull request. _Avoid_: external change, foreign change, PR mode.
- **Reviewer notes**: what the reviewer asks of every Explore round, written in a file beside the reviewer's settings and quoted by each kickoff. _Avoid_: custom instructions, preferences.
- **Decision**: a question the reviewer answered in an Explore round, with the answer kept, which the conclusion lists. _Avoid_: verdict, outcome.
- **Reset**: closing the current Explore round and returning to the start screen; the round stays readable as a past round, and its decisions no longer stand. _Avoid_: new round (which is what Start begins), clear, restart.
- **Run-ahead**: preparing the agent's next turn in forks while a question of an Explore round waits for the reviewer. _Avoid_: prefetch, speculation, preparation.
- **Fork**: a copy of the agent's session that run-ahead starts in the background for one choice of the question that waits. _Avoid_: branch, clone, subagent.
- **Prepared turn**: the turn a fork submitted for its choice, which becomes the round's when the reviewer answers with that choice and nothing changed. _Avoid_: cached turn, speculative turn.
- **Plain chain**: the path of a turn without run-ahead: the reviewer's answer goes to the agent in the pane, which takes the turn. _Avoid_: fallback, normal path.

## Testing

The reviewer's tests and visual testing use herdr-common's terms, as its
[CONTEXT.md](https://github.com/flupke/herdr-common/blob/main/CONTEXT.md) defines them: private
Herdr, fake agent, visual testing, vision server, vision session, vision core and frame signal.
_Avoid_: stand-in agent (the fake agent), Herdr test server (a private Herdr server), driver
(the vision core, or the agent driving a vision session), UI exploration (visual testing, since
Explore is the reviewer's interview)
