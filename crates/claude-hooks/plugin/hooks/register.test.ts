import { expect, test } from 'claude-code/testing'
import type { Engine, TestBody } from 'claude-code/testing'

import { ROUND_PROMPTS } from './round-prompts'

const OPTIONS = { options: { control: '/bin/true' } }

const step = (agentId?: string) => ({
  turnId: 'turn',
  index: 0,
  model: 'claude-sonnet-5-5',
  effort: 'low' as const,
  messageCount: 1,
  ...(agentId === undefined ? {} : { agentId }),
})

const ON_FABLE = 'Explore runs on claude-fable-5-2 at max effort.'
const ON_OPUS = 'Explore runs on claude-opus-5-5 at high effort.'

const [TURN, IMPLEMENT, POST_REVIEW] = ROUND_PROMPTS as [string, string, string]

function engine(on: Parameters<TestBody>[1]) {
  const sent: Array<[string, unknown, string | undefined]> = []
  on('turn.step', async function* (_, e) {
    sent.push([e.model, e.effort, e.agentId])
    return { turnId: e.turnId, index: e.index, answer: '', toolUses: [], stopReason: 'end_turn', usage: null } as never
  })
  on('prompt.submit', async (_, e) => ({ text: e.text }) as never)
  return sent
}

async function request($: Engine, agentId?: string) {
  for await (const _ of $.turn.step(step(agentId))) {
    // Claude Code's test engine gets the request only once the stream is read.
  }
}

async function prompt($: Engine, text: string) {
  await $.prompt.submit({ text } as never)
}

test('a round prompt puts the main loop on its model and effort, and leaves subagents alone', OPTIONS, async ($, on) => {
  const sent = engine(on)

  await request($)
  await prompt($, TURN)
  await request($)
  await request($, 'worker')
  await prompt($, 'Thanks, carry on.')
  await request($)
  await prompt($, TURN.replace(ON_FABLE, ON_OPUS))
  await request($)

  expect(sent).toEqual([
    ['claude-sonnet-5-5', 'low', undefined],
    ['claude-fable-5-2', 'max', undefined],
    ['claude-sonnet-5-5', 'low', 'worker'],
    ['claude-fable-5-2', 'max', undefined],
    ['claude-opus-5-5', 'high', undefined],
  ])
})

test('the Implement and Post review prompts switch too', OPTIONS, async ($, on) => {
  const sent = engine(on)

  await prompt($, IMPLEMENT)
  await request($)
  await prompt($, POST_REVIEW.replace(ON_FABLE, ON_OPUS))
  await request($)

  expect(sent).toEqual([
    ['claude-fable-5-2', 'max', undefined],
    ['claude-opus-5-5', 'high', undefined],
  ])
})

test('a line quoted outside the reviewer\'s own words switches nothing', OPTIONS, async ($, on) => {
  const sent = engine(on)

  await prompt($, `The lead's report quotes the round prompt:\n${ON_FABLE}\n`)
  await prompt($, `Explore round r3 failed for me, here is what it sent:\n${ON_FABLE}\n`)
  await prompt($, `${TURN.split('\n', 1)[0]} Also:\n${ON_FABLE}\n`)
  const withoutLine = (text: string) => text.replace(`${ON_FABLE}\n\n`, '')
  await prompt($, `${withoutLine(IMPLEMENT)}\nEnd the prompt with:\n${ON_FABLE}`)
  await prompt($, `${withoutLine(POST_REVIEW)}\n${ON_FABLE}`)
  await request($)

  expect(sent).toEqual([['claude-sonnet-5-5', 'low', undefined]])
})
