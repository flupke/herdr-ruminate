/** @import { QuizView, QuizItemView, QuizAnswerView, RailStep } from "./types.ts" */
/** @import { Turn } from "./turn.js" */

import { choiceCards, plainChoices } from './choices.js';
import { citation } from './citations.js';
import { codeSpans, h, keyOf, Region } from './dom.js';
import { STAGE } from './route.js';
import { turnStrip } from './turn.js';

/** @typedef {{ conclusion: string, item: number }} QuizAt */

/**
 * @typedef {object} QuizShowing
 * @property {number} shown
 * @property {boolean} asking
 * @property {(item: number) => void} open
 * @property {() => void} done
 */

export class QuizScreen {
  constructor() {
    this.element = h('section', { class: 'quiz desk', id: 'quiz', 'aria-labelledby': 'quiz-label' });
    this.turn = new Region(this.element, 'turn');
    this.head = new Region(this.element, 'head');
    this.panel = new Region(this.element, 'panel');
    this.proof = new Region(this.element, 'proof');
    /** @type {number | null} */
    this.shown = null;
  }

  /**
   * @param {QuizView} quiz
   * @param {string} conclusion
   * @param {Turn} turn
   * @param {QuizShowing} showing
   */
  update(quiz, conclusion, turn, showing) {
    const { shown, asking } = showing;
    const item = /** @type {QuizItemView} */ (quiz.items[shown]);
    const checked = item.picked_correct !== null;
    this.turn.show(asking ? keyOf(turn) : null, () => turnStrip(turn));
    const reach = quiz.items.map((_, index) => itemOpens(quiz, index, asking));
    const marks = quiz.items.map((each) => each.picked_correct);
    this.head.show(keyOf([shown, item.question, marks, reach, asking, quiz.skipped]), () => head(quiz, showing, reach));
    this.panel.show(keyOf([conclusion, shown, quiz, asking]), () => panel(quiz, item, conclusion, showing, reach));
    const proven = checked || !asking;
    this.proof.show(keyOf([shown, proven ? item.proof : null]), () => (proven ? proof(item) : null));
    if (this.shown !== null && this.shown !== shown) {
      if (this.element.getBoundingClientRect().top < 0) this.element.scrollIntoView({ block: 'start' });
      const headline = this.element.querySelector('.quiz-item');
      if (!this.element.contains(document.activeElement) && headline instanceof HTMLElement) {
        headline.focus({ preventScroll: true });
      }
    }
    this.shown = shown;
  }
}

/**
 * @param {QuizView} quiz
 * @param {number} index
 * @param {boolean} asking
 */
export function itemOpens(quiz, index, asking) {
  const item = quiz.items[index];
  if (!item) return false;
  return !asking || item.picked_correct !== null || index === quiz.next;
}

/** @param {RailStep} step */
export function quizOpens(step) {
  return (
    step.step.kind === 'quiz' &&
    step.state.kind === 'done' &&
    step.step.stage.kind === 'scored' &&
    step.step.stage.answered > 0
  );
}

/**
 * @param {RailStep[]} rail
 * @param {number} shown
 * @param {number} items
 * @returns {RailStep[]}
 */
export function railShowing(rail, shown, items) {
  return rail.map((step) => {
    if (step.step.kind === 'quiz') {
      return {
        step: { kind: 'quiz', stage: { kind: 'running', item: shown + 1, items } },
        state: { kind: 'current', working: false },
      };
    }
    if (step.step.kind === 'conclusion') return { ...step, state: { kind: 'later' } };
    return step;
  });
}

/**
 * @param {QuizView} quiz
 * @param {QuizShowing} showing
 * @param {boolean[]} reach
 */
function head(quiz, showing, reach) {
  const item = /** @type {QuizItemView} */ (quiz.items[showing.shown]);
  const before = showing.asking && item.picked_correct === null;
  return h(
    'header',
    { class: 'quiz-head' },
    h(
      'p',
      { class: 'eyebrow quiz-eyebrow' },
      h('span', { id: 'quiz-label' }, `Quiz · Question ${item.number} of ${quiz.items.length}`),
      dots(quiz, showing, reach),
    ),
    h('h2', { class: 'quiz-item', tabindex: -1 }, codeSpans(item.question)),
    h(
      'p',
      { class: 'hint' },
      'The quiz checks what you took from the design; nothing here marks lines.',
      before ? ' Its proof shows once you check your answer.' : null,
    ),
  );
}

/** @typedef {'correct' | 'wrong' | 'open'} Outcome */

const OUTCOMES = {
  correct: { dot: 'good', dotWords: 'correct', tone: 'good', verdict: 'Correct.' },
  wrong: { dot: 'bad', dotWords: 'wrong', tone: 'bad', verdict: 'Not quite.' },
  open: { dot: 'pending', dotWords: 'not answered yet', tone: 'open', verdict: 'Not answered.' },
};

/** @param {QuizItemView} item
 * @returns {Outcome} */
function outcomeOf(item) {
  if (item.picked_correct === null) return 'open';
  return item.picked_correct ? 'correct' : 'wrong';
}

/**
 * @param {QuizView} quiz
 * @param {QuizShowing} showing
 * @param {boolean[]} reach
 */
function dots(quiz, showing, reach) {
  const unanswered = !showing.asking && quiz.skipped ? 'skipped' : OUTCOMES.open.dotWords;
  return h(
    'span',
    { class: 'quiz-dots', role: 'group', 'aria-label': 'Quiz questions' },
    quiz.items.map((item, index) => {
      const outcome = outcomeOf(item);
      const name = `Question ${item.number}, ${outcome === 'open' ? unanswered : OUTCOMES[outcome].dotWords}`;
      return h(
        'button',
        {
          class: `dot ${OUTCOMES[outcome].dot}`,
          type: 'button',
          'aria-label': name,
          title: name,
          'aria-current': index === showing.shown ? 'step' : null,
          disabled: !reach[index],
          'data-focus-key': `quiz-dot-${item.number}`,
          onclick: () => {
            if (index !== showing.shown) showing.open(index);
          },
        },
        h('span', { class: 'disc', 'aria-hidden': 'true' }),
      );
    }),
  );
}

/**
 * @param {QuizView} quiz
 * @param {QuizItemView} item
 * @param {string} conclusion
 * @param {QuizShowing} showing
 * @param {boolean[]} reach
 */
function panel(quiz, item, conclusion, showing, reach) {
  const { asking } = showing;
  const checked = item.picked_correct !== null;
  return h(
    'section',
    { class: 'quiz-panel panel', 'aria-label': `Your answer to quiz question ${item.number}` },
    asking ? (checked ? checkedItem(item) : pick(item, conclusion)) : answeredItem(quiz, item),
    moves(quiz, showing, reach),
    asking && quiz.next !== null ? skipForm(conclusion) : null,
  );
}

/**
 * @param {QuizItemView} item
 * @param {string} conclusion
 */
function pick(item, conclusion) {
  const answers = plainChoices(item.answers.map((answer) => ({ id: String(answer.index), text: answer.text })));
  return h(
    'form',
    { class: 'quiz-pick', 'data-method': 'quiz', 'data-requires': 'answer' },
    h('input', { type: 'hidden', name: 'conclusion', value: conclusion }),
    h('input', { type: 'hidden', name: 'item', value: item.number - 1 }),
    choiceCards(answers, { name: 'answer', legend: 'Answers' }),
    h('button', { class: 'button primary block', type: 'submit' }, 'Check'),
  );
}

/** @param {QuizItemView} item */
function checkedItem(item) {
  return [verdict(item, { role: 'status', tabindex: -1, 'data-shows': 'quiz' }), answerList(item, 'quiz-answers-title')];
}

/**
 * @param {QuizView} quiz
 * @param {QuizItemView} item
 */
function answeredItem(quiz, item) {
  return [
    quizScore(quiz, { href: STAGE, words: 'Back to the conclusion' }),
    verdict(item, {}, quiz.skipped ? 'Skipped.' : undefined),
    answerList(item, 'quiz-answers-title'),
  ];
}

/**
 * @param {QuizView} quiz
 * @param {QuizShowing} showing
 * @param {boolean[]} reach
 */
function moves(quiz, showing, reach) {
  const { shown, asking } = showing;
  const item = /** @type {QuizItemView} */ (quiz.items[shown]);
  const checked = item.picked_correct !== null;
  const conclude = asking && checked && quiz.next === null && shown === quiz.items.length - 1;
  const following = reach[shown + 1] === true;
  return h(
    'nav',
    { class: 'quiz-moves', 'aria-label': 'Move between quiz questions' },
    h(
      'button',
      {
        class: 'button secondary',
        type: 'button',
        disabled: shown === 0,
        'data-focus-key': 'quiz-previous',
        onclick: () => showing.open(shown - 1),
      },
      h('span', { class: 'arrow back', 'aria-hidden': 'true' }, '←'),
      'Previous',
    ),
    h(
      'button',
      {
        class: `button ${asking && checked ? 'primary' : 'secondary'}`,
        type: 'button',
        disabled: !conclude && !following,
        'data-focus-key': 'quiz-next',
        onclick: () => (conclude ? showing.done() : showing.open(shown + 1)),
      },
      conclude ? 'Show the conclusion' : 'Next question',
      h('span', { class: 'arrow', 'aria-hidden': 'true' }, '→'),
    ),
  );
}

/** @param {string} conclusion */
function skipForm(conclusion) {
  return h(
    'form',
    { class: 'quiz-skip', 'data-method': 'quiz-skip' },
    h('input', { type: 'hidden', name: 'conclusion', value: conclusion }),
    h('button', { class: 'button quiet', type: 'submit' }, 'Skip the quiz'),
  );
}

/**
 * @param {QuizItemView} item
 * @param {Record<string, string | number>} [props]
 * @param {string} [unanswered]
 */
function verdict(item, props = {}, unanswered = OUTCOMES.open.verdict) {
  const kind = outcomeOf(item);
  const outcome = OUTCOMES[kind];
  const words = kind === 'open' ? unanswered : outcome.verdict;
  return h('p', { class: `verdict ${outcome.tone}`, ...props }, h('span', {}, h('strong', {}, words), ' ', codeSpans(item.why)));
}

/**
 * @param {QuizItemView} item
 * @param {string} id
 */
function answerList(item, id) {
  return [
    h('p', { class: 'eyebrow', id }, 'Answers'),
    h('ol', { class: 'quiz-answers', 'aria-labelledby': id }, item.answers.map(answerCard)),
  ];
}

/** @param {QuizAnswerView} answer */
function answerCard(answer) {
  const tone = answer.correct ? 'correct' : answer.picked ? 'wrong' : 'other';
  return h(
    'li',
    { class: `choice quiz-answer ${tone}` },
    h('span', { class: 'mark', 'aria-hidden': 'true' }),
    h('span', { class: 'choice-text' }, codeSpans(answer.text)),
    answer.picked || answer.correct
      ? h(
          'span',
          { class: 'choice-tags' },
          answer.picked ? h('span', { class: 'tag accent' }, 'Your pick') : null,
          answer.correct ? h('span', { class: 'tag good' }, 'Correct') : null,
        )
      : null,
  );
}

/** @param {QuizItemView} item */
function proof(item) {
  const id = `quiz-${item.number}-proof`;
  return h(
    'section',
    { class: 'proof', 'aria-labelledby': id },
    h('h3', { class: 'eyebrow', id }, 'Proof'),
    item.proof.map((each, index) => citation(each, `${id}-${index + 1}`)),
  );
}

/**
 * @param {QuizView} quiz
 * @param {{ href: string, words: string }} link
 */
export function quizScore(quiz, link) {
  const items = quiz.items.length;
  const missed = quiz.picked - quiz.correct_picks;
  const unanswered = items - quiz.picked;
  const questions = (/** @type {number} */ count) => `${count} ${count === 1 ? 'question' : 'questions'}`;
  const words = [
    missed > 0 ? `You missed ${questions(missed)}.` : null,
    unanswered > 0 ? `You ${quiz.skipped ? 'skipped' : 'did not answer'} ${questions(unanswered)}.` : null,
  ].filter((part) => part !== null);
  const tone = quiz.correct_picks * 3 >= items * 2 ? 'good' : 'warn';
  return h(
    'p',
    { class: `quiz-banner ${tone}` },
    h('strong', {}, `Quiz ${quiz.correct_picks} of ${items}`),
    ' ',
    words.length > 0 ? words.join(' ') : 'Every answer was correct.',
    ' ',
    h('a', { href: link.href }, link.words),
  );
}
