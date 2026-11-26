/** @import { KeptAnswer, MarkPhrase } from "./types.ts" */

import { decisionTag } from './chips.js';
import { codeSpans, h } from './dom.js';

export const NOTHING_ANSWERED = 'No choice, no comment';

/**
 * @param {KeptAnswer} answer
 * @param {{ withChoice?: boolean }} [options]
 */
export function answerCard(answer, { withChoice = true } = {}) {
  withChoice ||= answer.drawing_board;
  if (!withChoice && answer.choice !== null && !answer.comment && answer.tags.length === 0) return null;
  return h(
    'div',
    { class: 'answer-card' },
    withChoice && answer.choice !== null ? h('p', { class: 'answer-choice' }, codeSpans(answer.choice)) : null,
    answer.comment ? h('p', { class: 'answer-comment' }, `“${answer.comment}”`) : null,
    answer.choice === null && !answer.comment ? h('p', { class: 'answer-comment' }, NOTHING_ANSWERED) : null,
    answer.tags.length > 0 ? h('p', { class: 'answer-tags' }, answer.tags.map(decisionTag)) : null,
  );
}

/** @param {string} problem */
export function marksProblemLine(problem) {
  return h(
    'p',
    { class: 'answer-marks-problem' },
    h('span', { class: 'glyph', 'aria-hidden': 'true' }, '!'),
    ` Review marks not applied: ${problem}`,
  );
}

/** @param {MarkPhrase} marks */
export function markedLine(marks) {
  return h(
    'p',
    { class: 'answer-marked' },
    h('span', { class: 'check', 'aria-hidden': 'true' }, '✓'),
    ` ${marks.verb} ${marks.parts.join(' · ')}`,
  );
}
