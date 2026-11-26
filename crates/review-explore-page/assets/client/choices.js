/** @import { ChoiceView } from "./types.ts" */

import { codeSpans, h } from './dom.js';

/**
 * @param {{ id: string, text: string }[]} options
 * @returns {ChoiceView[]}
 */
export function plainChoices(options) {
  return options.map(({ id, text }) => ({ id, text, recommendation: null, checked: false }));
}

/**
 * @param {ChoiceView[]} choices
 * @param {{ name?: string, legend?: string, picking?: boolean, revealed?: boolean, answered?: boolean }} [options]
 */
export function choiceCards(
  choices,
  { name = 'choice', legend = 'Choices', picking = false, revealed = false, answered = false } = {},
) {
  return h(
    'fieldset',
    { class: answered ? 'choices answered' : 'choices', disabled: answered },
    h('legend', { class: answered ? 'sr-only' : 'eyebrow' }, legend),
    choices.map((choice, index) => {
      const { recommendation } = choice;
      const reason = recommendation !== null ? `recommendation-${index + 1}` : null;
      const text = `${name}-${index + 1}`;
      const firstPick = revealed && choice.checked;
      return h(
        'label',
        { class: reason ? 'choice recommended' : 'choice' },
        h('input', {
          type: 'radio',
          name,
          value: choice.id,
          checked: choice.checked,
          required: picking,
          'aria-labelledby': text,
          'aria-describedby': reason,
        }),
        h('span', { class: 'choice-text', id: text }, codeSpans(choice.text)),
        firstPick || reason
          ? h(
              'span',
              { class: 'choice-tags' },
              firstPick ? h('span', { class: 'tag accent' }, 'Your first pick') : null,
              reason ? h('span', { class: 'tag agent' }, '◆ Agent recommends') : null,
            )
          : null,
        recommendation !== null ? h('span', { class: 'choice-reason', id: reason }, codeSpans(recommendation)) : null,
      );
    }),
  );
}
