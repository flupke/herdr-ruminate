/** @import { DecisionTag, Door } from "./types.ts" */

import { h } from './dom.js';

export const DOORS = {
  one_way: { name: 'One-way door', kind: 'warn' },
  two_way: { name: 'Two-way door', kind: 'good' },
  mixed: { name: 'Mixed door', kind: 'neutral' },
  unknown: { name: 'Door unknown', kind: 'neutral' },
};

/** @param {Door} door */
export function doorChip(door) {
  const { name, kind } = DOORS[door];
  return h('span', { class: `chip ${kind}` }, name);
}

const DECISION_TAGS = {
  changed_after_first_pick: { words: 'changed after your first pick', kind: 'accent' },
  as_recommended: { words: 'as recommended', kind: 'agent' },
};

/** @param {DecisionTag} tag */
export function decisionTag(tag) {
  const { words, kind } = DECISION_TAGS[tag];
  return h('span', { class: `tag ${kind}` }, words);
}
