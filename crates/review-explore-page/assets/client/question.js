/** @import { Door, QuestionView, ChoiceView, MarksView, GainView, SectionView } from "./types.ts" */
/** @import { Children } from "./dom.js" */
/** @import { Turn } from "./turn.js" */

import { takeOverComment } from './answer-comment.js';
import { DOORS, doorChip } from './chips.js';
import { choiceCards } from './choices.js';
import { citationsSection } from './citations.js';
import { disclosure } from './disclosure.js';
import { h, keyOf, markdown, Region, setRenderedMarkdown } from './dom.js';
import { keepDraft } from './drafts.js';
import { answerFields, drawingBoard } from './drawing-board.js';
import { SEND_KEYS_LABEL, sendsOnKeys } from './send-keys.js';
import { turnStrip } from './turn.js';

export class QuestionScreen {
  /** @param {QuestionView} question */
  constructor(question) {
    this.element = h('section', {
      class: 'question desk',
      'data-shows': 'cancel-answer',
      'aria-labelledby': `question-${question.number}-label`,
      'data-question': question.id,
      'data-version': question.version,
    });
    this.turn = new Region(this.element, 'turn');
    this.reading = new Region(this.element, 'reading');
    this.panel = new Region(this.element, 'panel');
    this.citations = new Region(this.element, 'citations');
  }

  /**
   * @param {QuestionView} question
   * @param {Turn} turn
   * @param {boolean} past
   */
  update(question, turn, past) {
    this.element.dataset.version = String(question.version);
    this.turn.show(keyOf(turn), () => turnStrip(turn));
    const { text_html, context_html, sections, number, door, answerable, revision } = question;
    const blind = question.recommendation !== 'shown';
    this.reading.show(keyOf({ text_html, context_html, sections, number, door, blind, answerable, revision }), () =>
      reading(question),
    );
    this.panel.show(
      keyOf([
        question.version,
        question.answerable,
        question.recommendation,
        question.choices,
        question.marks,
        question.gain,
        door,
        past,
      ]),
      () => {
        if (question.answerable) return h('div', { class: 'answer-panel panel' }, panel(question), drawingBoard(question));
        return past ? unansweredPanel(question) : null;
      },
    );
    this.citations.show(keyOf(question.citations), () => citationsSection(question.citations, question.number));
  }
}

/** @param {QuestionView} question */
function reading(question) {
  const { revision } = question;
  return [
    ...(revision ? [revisionNotice(revision.reason_html)] : []),
    ...questionReading(
      { ...question, label: `question-${question.number}-label`, name: `Question ${question.number}` },
      [
        question.recommendation !== 'shown' ? h('span', { class: 'chip agent' }, 'Blind pick') : null,
        question.answerable ? h('a', { class: 'choices-link', href: '#answer' }, 'Choices ↓') : null,
      ],
    ),
  ];
}

/** @param {QuestionView} question */
function unansweredPanel(question) {
  return h(
    'div',
    { class: 'answer-panel panel' },
    h('p', { class: 'eyebrow' }, 'The round left this question unanswered'),
    choiceCards(question.choices, { name: `unanswered-${question.number}-choice`, answered: true }),
  );
}

/** @param {string} reasonHtml */
function revisionNotice(reasonHtml) {
  const reason = h('div', { class: 'revision-reason' });
  setRenderedMarkdown(reason, reasonHtml);
  return h(
    'div',
    { class: 'revision', role: 'note' },
    h('strong', {}, 'The agent revised this question after your message in the chat.'),
    reason,
  );
}

/**
 * @param {{ label: string, name: string, door: Door | null, text_html: string,
 *   context_html: string | null, sections: SectionView[] }} question
 * @param {Children} more
 */
export function questionReading(question, more) {
  const headline = h('h2', { class: 'question-text' });
  setRenderedMarkdown(headline, question.text_html);
  return [
    h(
      'header',
      { class: 'question-head' },
      h(
        'p',
        { class: 'eyebrow question-eyebrow' },
        h('span', { id: question.label }, question.name),
        question.door ? doorChip(question.door) : null,
        more,
      ),
      headline,
    ),
    question.context_html !== null ? markdown(question.context_html, 'explanation') : null,
    question.sections.length > 0 ? h('div', { class: 'assessments' }, question.sections.map(assessment)) : null,
  ].filter((node) => node !== null);
}

/** @param {SectionView} section */
function assessment(section) {
  const lead = h('span', { class: 'assessment-lead' });
  setRenderedMarkdown(lead, section.lead_html);
  const { element } = disclosure(
    [h('span', { class: 'assessment-title' }, section.title), lead],
    section.details_html !== null ? markdown(section.details_html) : [],
    { button: 'disclosure-row assessment-toggle' },
  );
  element.classList.add('assessment');
  return element;
}

/** @param {QuestionView} question */
function panel(question) {
  const picking = question.recommendation === 'hidden_until_pick';
  const revealed = question.recommendation === 'shown_after_pick';
  const door = question.door ? DOORS[question.door].name : 'This question';
  const sendLabel = revealed ? 'Confirm answer' : 'Send answer';
  const { key: draft, from } = takeOverComment(question);
  const box = sendsOnKeys(
    keepDraft(
      h('textarea', { class: 'comment', id: 'comment', name: 'comment', rows: 3, placeholder: `${sendLabel}: ${SEND_KEYS_LABEL}` }),
      draft,
      '',
    ),
  );
  if (from !== null) box.dataset.draftFrom = from;
  return h(
    'form',
    {
      class: 'answer',
      id: 'answer',
      'aria-label': `Your answer to question ${question.number}`,
      'data-method': picking ? 'pick' : 'answer',
      'data-requires': picking ? 'choice' : 'choice-or-comment',
    },
    answerFields(question),
    picking
      ? h(
          'p',
          { class: 'hint' },
          `${door}: the agent's recommendation shows when you send, so your first read of the choices is your own. You can still change your pick after.`,
        )
      : null,
    revealed ? revealLine(question.choices) : null,
    choiceCards(question.choices, { picking, revealed }),
    h(
      'label',
      { class: 'eyebrow comment-label', for: 'comment' },
      'Comment',
      h('span', { class: 'optional' }, ' · optional'),
    ),
    box,
    question.marks ? gainLine(question.marks, question.gain) : null,
    h('button', { class: 'button primary block', type: 'submit' }, sendLabel),
  );
}

/** @param {ChoiceView[]} choices */
function revealLine(choices) {
  const same = choices.some((choice) => choice.checked && choice.recommendation !== null);
  const shows = { tabindex: -1, 'data-shows': 'pick' };
  return same
    ? h('p', { class: 'reveal same', ...shows }, h('strong', {}, 'You and the agent picked the same choice.'))
    : h(
        'p',
        { class: 'reveal other', ...shows },
        h('strong', {}, 'The agent recommends another choice.'),
        ' Read its reason, then keep yours or change it.',
      );
}

/**
 * @param {MarksView} marks
 * @param {GainView | null} gain
 */
function gainLine(marks, gain) {
  const [first, ...rest] = marks.summary.parts;
  const { element } = disclosure(
    [
      h(
        'span',
        { class: 'gain-line' },
        h('span', { class: 'gain-text' }, `${marks.summary.verb} `, h('strong', {}, first), rest.map((part) => ` · ${part}`)),
        gain ? h('span', { class: 'gain-share' }, `${gain.before}% → `, h('strong', {}, `${gain.after}%`)) : null,
      ),
      gain ? gainBar(gain) : null,
    ],
    h(
      'ul',
      { class: 'gain-lines' },
      marks.reviewed.map((line) => h('li', {}, `${line} (reviewed)`)),
      marks.not_relevant.map((line) => h('li', {}, line)),
      marks.reopened.map((line) => h('li', {}, `${line} (reopened)`)),
    ),
    { button: 'disclosure-row gain-toggle' },
  );
  element.classList.add('gain');
  return element;
}

/** @param {GainView} gain */
function gainBar(gain) {
  const done = h('span', { class: 'gain-done' });
  const added = h('span', { class: 'gain-added' });
  done.style.width = `${gain.before}%`;
  added.style.width = `${Math.max(gain.after - gain.before, 0)}%`;
  return h('span', { class: 'gain-bar', 'aria-hidden': 'true' }, done, added);
}
