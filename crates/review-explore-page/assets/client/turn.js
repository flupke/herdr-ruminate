/** @import { KeptAnswer, LatestAnswer, ResponseView } from "./types.ts" */

import { NOTHING_ANSWERED, marksProblemLine } from './answer-card.js';
import { disclosure } from './disclosure.js';
import { codeSpans, h, markdown } from './dom.js';

/**
 * @typedef {object} Turn
 * @property {LatestAnswer | null} answer
 * @property {KeptAnswer | null} [kept]
 * @property {ResponseView | null} response
 * @property {number | null} number
 */

/** @param {Turn} turn */
export function turnStrip({ answer, kept = null, response, number }) {
  if (!answer && !response) return null;
  const followUps = response ? followUpsLine(response, 'turn-follow-ups') : null;
  const path = response?.path_line ? h('p', { class: 'turn-path' }, response.path_line) : null;
  const shown = answer ?? kept;
  return h(
    'div',
    { class: 'turn' },
    shown ? answered(shown, number) : null,
    response ? recorded(response) : null,
    response?.marks_problem ? marksProblemLine(response.marks_problem) : null,
    path || followUps || answer
      ? h(
          'div',
          { class: 'turn-foot' },
          path,
          followUps,
          answer ? cancelAnswer(answer.id, 0) : null,
        )
      : null,
  );
}

/**
 * @param {LatestAnswer | KeptAnswer} answer
 * @param {number | null} number
 */
function answered(answer, number) {
  const title = number === null ? 'You answered' : `You answered Q${number}`;
  return h(
    'section',
    { class: 'turn-answer', 'aria-labelledby': 'turn-answer-title' },
    h('p', { class: 'eyebrow', id: 'turn-answer-title' }, title),
    h(
      'div',
      { class: 'turn-text' },
      answer.choice ? h('p', { class: 'turn-choice' }, codeSpans(answer.choice)) : null,
      answer.comment ? h('p', { class: 'turn-comment' }, `“${answer.comment}”`) : null,
      !answer.choice && !answer.comment ? h('p', { class: 'turn-comment' }, NOTHING_ANSWERED) : null,
    ),
  );
}

/**
 * @param {ResponseView} response
 * @param {string} className
 */
export function followUpsLine(response, className) {
  const followUps = response.interpretations.flatMap((interpretation) => interpretation.follow_ups);
  return followUps.length > 0
    ? h('p', { class: className }, 'Follow-ups', followUps.map((followUp) => [' · ', codeSpans(followUp)]))
    : null;
}

/**
 * @param {ResponseView} response
 * @param {string} [id]
 */
export function recorded(response, id = 'turn-record-title') {
  const recaps = response.interpretations.flatMap((interpretation) =>
    interpretation.recap_html ? [interpretation.recap_html] : [],
  );
  const title = recaps.length > 0 ? 'The agent recorded' : 'The agent replied';
  return h(
    'section',
    { class: 'turn-record', 'aria-labelledby': id },
    h('p', { class: 'eyebrow', id }, title),
    h(
      'div',
      { class: 'turn-text' },
      recaps.map((recap) => markdown(recap, 'turn-recap')),
      response.reply_html ? markdown(response.reply_html, 'turn-reply') : null,
    ),
  );
}

/**
 * @param {string} answer
 * @param {number} later
 */
export function cancelAnswer(answer, later) {
  const after =
    later === 0
      ? "with the agent's turn after it and the review marks it led to"
      : `with the ${later === 1 ? 'answer' : `${later} answers`} you sent after it, the agent's turns after them and the review marks they led to`;
  return disclosure(
    'Cancel this answer…',
    h(
      'form',
      { class: 'cancel-answer', 'data-method': 'cancel-answer' },
      h('input', { type: 'hidden', name: 'answer', value: answer }),
      h('p', { class: 'hint' }, `Cancel answer takes this answer back, ${after}: its question waits for your answer again.`),
      h(
        'button',
        { class: 'button secondary', type: 'submit' },
        later === 0 ? 'Confirm: cancel my answer' : 'Confirm: cancel from this answer',
      ),
    ),
    { button: 'button quiet' },
  ).element;
}
