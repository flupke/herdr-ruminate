import type { Register } from 'claude-code'

const ROUND_TURN =
  /^Explore round \S+: read .+ and follow it\. The turn ends in one call to submit_question or submit_conclusion, tools of the \S+ MCP server\.$/
const IMPLEMENT =
  "The reviewer clicked Implement on the Explore conclusion. Implement the edited task list below in this conversation, following the repository instructions and validating the changes. This action authorizes code edits and tests for these tasks. Summary and future-work items are not additional implementation scope. Do not submit another review question or mark files reviewed as part of this action. Close with a short report for a reader who did not watch you work and has not seen your tool calls: first what changed for each task, then how you checked it and what is left, in plain full sentences, with no term you made up during the work."
const POST_REVIEW =
  "The reviewer clicked Post review on the Explore conclusion of someone else's change. Post the review below on the change's pull request, with the repository's usual tool for it, such as gh: each comment on the file and line it names, the others in the review's body. This action authorizes posting these comments only: do not edit code, push, approve or request changes. If you cannot find the change's pull request, say so and stop. Close with a short report that says what you posted and where."

const isReviewersPrompt = (text: string) => {
  const first = text.split('\n', 1)[0]!
  return ROUND_TURN.test(first) || first === IMPLEMENT || first === POST_REVIEW
}

const REVIEWERS_TEXT = /\n(?:Tasks|Review) from the reviewer:\n/
const ROUND_MODEL = /^Explore runs on (\S+) at (low|medium|high|xhigh|max) effort\.$/m

type Effort = 'low' | 'medium' | 'high' | 'xhigh' | 'max'

// Claude Code's own /model and /effort would save a round's model and effort as the user's
// defaults for new sessions.
let round: { model: string; effort: Effort } | undefined

export const register: Register = on => {
  on('prompt.submit', async ($, e, next) => {
    const found = isReviewersPrompt(e.text) ? ROUND_MODEL.exec(e.text.split(REVIEWERS_TEXT)[0]!) : null

    if (found !== null) {
      round = { model: found[1]!, effort: found[2] as Effort }
    }

    return next(e)
  }).catch(($, e, next) => next(e))

  on('turn.step', async function* ($, e, next) {
    if (e.agentId !== undefined || round === undefined) {
      return yield* next(e)
    }

    return yield* next({ ...e, model: round.model, effort: round.effort })
  })
}
