/** @import { QuestionView } from "./types.ts" */

import { keep, kept, typed } from './drafts.js';

/**
 * @param {QuestionView} question
 * @returns {{ key: string, from: string | null }}
 */
export function takeOverComment(question) {
  const key = commentKey(question, question.version);
  const from = question.revision ? commentKey(question, question.revision.replaced_version) : null;
  const replaced = from === null ? null : kept(from);
  if (replaced !== null && kept(key) === null) keep(key, replaced);
  return { key, from };
}

/** @param {QuestionView} question */
export function typedComment(question) {
  const replaced = question.revision ? kept(commentKey(question, question.revision.replaced_version)) : null;
  const text = typed(commentKey(question, question.version)) ?? replaced ?? '';
  return text.trim() === '' ? '' : text;
}

/** @param {QuestionView} question
 * @param {number} version */
function commentKey(question, version) {
  return `comment:${question.round ?? ''}:${question.id}:${version}`;
}
