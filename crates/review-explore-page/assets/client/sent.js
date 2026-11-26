/** @import { SentView, LatestAnswer } from "./types.ts" */

import { answerCard, markedLine, marksProblemLine } from './answer-card.js';
import { choiceCards } from './choices.js';
import { citationsSection } from './citations.js';
import { h, keyOf, Region } from './dom.js';
import { questionReading } from './question.js';
import { panelActions, panelCard } from './status.js';
import { cancelAnswer } from './turn.js';

export class SentScreen {
  constructor() {
    this.element = h('div', { class: 'sent desk' });
    this.card = new Region(this.element, 'card');
    this.panel = new Region(this.element, 'panel');
    this.reading = new Region(this.element, 'reading');
    this.citations = new Region(this.element, 'citations');
  }

  /**
   * @param {SentView} sent
   * @param {LatestAnswer | null} cancellable
   */
  update(sent, cancellable) {
    this.card.show(keyOf(sent.card), () => panelCard(sent.card));
    const question = sent.question;
    this.reading.show(keyOf(question && [question.id, question.version, question.number]), () => {
      if (!question) return null;
      const label = `question-${question.number}-label`;
      return h(
        'section',
        { class: 'answered-question', 'aria-labelledby': label },
        questionReading(
          { ...question, label, name: `Question ${question.number} · answered` },
          question.recommendation !== 'shown' ? h('span', { class: 'chip agent' }, 'Blind pick') : null,
        ),
      );
    });
    this.panel.show(keyOf([sent, cancellable]), () => panel(sent, cancellable));
    this.citations.show(keyOf(question?.citations ?? null), () =>
      question ? citationsSection(question.citations, question.number) : null,
    );
  }
}

/**
 * @param {SentView} sent
 * @param {LatestAnswer | null} cancellable
 */
function panel(sent, cancellable) {
  const title = sent.question
    ? sent.number !== null
      ? `Your answer to question ${sent.number}`
      : 'Your answer'
    : 'Your reply to the conclusion';
  return h(
    'section',
    { class: 'sent-answer panel', 'aria-labelledby': 'sent-answer-title' },
    h('p', { class: 'eyebrow', id: 'sent-answer-title' }, title),
    sent.question && !sent.answer.drawing_board
      ? choiceCards(sent.question.choices, { name: 'sent-choice', answered: true })
      : null,
    answerCard(sent.answer, { withChoice: !sent.question }),
    sent.marked ? markedLine(sent.marked) : null,
    sent.marks_problem ? marksProblemLine(sent.marks_problem) : null,
    panelActions(sent.card),
    cancellable ? cancelAnswer(cancellable.id, 0) : null,
  );
}
