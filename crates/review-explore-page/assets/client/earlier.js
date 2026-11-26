/** @import { EarlierQuestionView } from "./types.ts" */
/** @import { Current } from "./design.js" */

import { answerCard, markedLine, marksProblemLine } from './answer-card.js';
import { choiceCards } from './choices.js';
import { citationsSection } from './citations.js';
import { goTo } from './design.js';
import { h } from './dom.js';
import { questionReading } from './question.js';
import { cancelAnswer, followUpsLine, recorded } from './turn.js';

/**
 * @param {EarlierQuestionView} question
 * @param {Current | null} current
 */
export function earlierScreen(question, current) {
  const label = `earlier-${question.number}-label`;
  const [head, ...reading] = questionReading(
    { ...question, label, name: `Question ${question.number} · answered` },
    null,
  );
  return h(
    'section',
    { class: 'earlier-question desk', 'aria-labelledby': label },
    head,
    panel(question, current),
    reading,
    citationsSection(question.citations, question.number),
  );
}

/**
 * @param {EarlierQuestionView} question
 * @param {Current | null} current
 */
function panel(question, current) {
  const title = `earlier-${question.number}-answer`;
  return h(
    'section',
    { class: 'earlier-panel panel', 'aria-labelledby': title },
    h('p', { class: 'eyebrow', id: title }, `Your answer to question ${question.number}`),
    question.answer?.drawing_board
      ? null
      : choiceCards(question.choices, { name: `earlier-${question.number}-choice`, answered: true }),
    question.answer
      ? answerCard(question.answer, { withChoice: false })
      : h('p', { class: 'hint' }, 'The round left this question unanswered.'),
    question.marks.map(markedLine),
    question.marks_problems.map(marksProblemLine),
    record(question),
    question.cancel ? cancelAnswer(question.cancel.answer, question.cancel.later) : null,
    current ? goTo(current) : null,
  );
}

/** @param {EarlierQuestionView} question */
function record(question) {
  const response = question.recorded;
  const said = response.interpretations.some((interpretation) => interpretation.recap_html) || response.reply_html;
  const followUps = followUpsLine(response, 'earlier-follow-ups');
  if (!said && !followUps) return null;
  return h(
    'div',
    { class: 'earlier-record' },
    said ? recorded(response, `earlier-${question.number}-record`) : null,
    followUps,
  );
}
