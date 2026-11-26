/** @import { QuestionView } from "./types.ts" */

import { typedComment } from './answer-comment.js';
import { h } from './dom.js';

const HINT =
  "The agent stops asking and lists the changes the design needs, for you to edit on the conclusion. Your comment, if you typed one, goes with it as your reason. Acting on the conclusion reopens the lines this round marked reviewed, for a new round on the new code; until then, Cancel answer takes it back.";

const CONFIRM = 'Confirm: back to the drawing board';

/** @param {QuestionView} question */
export function answerFields(question) {
  return [
    question.round !== null ? h('input', { type: 'hidden', name: 'round', value: question.round }) : null,
    h('input', { type: 'hidden', name: 'question', value: question.id }),
    h('input', { type: 'hidden', name: 'version', value: question.version }),
    question.number > 0 ? h('input', { type: 'hidden', name: 'number', value: question.number }) : null,
  ];
}

/** @param {QuestionView} question */
export function drawingBoard(question) {
  /** @type {HTMLElement[]} */
  let shown = [];
  const ask = h(
    'button',
    { class: 'button secondary block', type: 'button', onclick: () => arm() },
    'Back to the drawing board…',
  );
  const form = drawingBoardForm(question, ask);
  const arm = () => {
    const confirm = h('button', { class: 'button danger block', type: 'submit' }, CONFIRM);
    shown = [confirm, h('p', { class: 'hint' }, HINT)];
    ask.replaceWith(...shown);
    confirm.focus();
  };
  form.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape' || shown.length === 0) return;
    for (const node of shown.slice(1)) node.remove();
    shown[0].replaceWith(ask);
    shown = [];
    ask.focus();
  });
  return form;
}

/** @param {QuestionView} question */
export function drawingBoardConfirmation(question) {
  return drawingBoardForm(
    question,
    h('p', { class: 'hint' }, HINT),
    h('button', { class: 'button danger', type: 'submit' }, CONFIRM),
  );
}

/**
 * @param {QuestionView} question
 * @param {...HTMLElement} children
 */
function drawingBoardForm(question, ...children) {
  const form = h(
    'form',
    { class: 'drawing-board', 'data-method': 'answer' },
    answerFields(question),
    h('input', { type: 'hidden', name: 'drawing_board', value: 'true' }),
    children,
  );
  form.addEventListener('formdata', (event) => event.formData.set('comment', typedComment(question)));
  return form;
}
