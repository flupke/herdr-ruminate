// The comment typed in the answer's box of a question: the box keeps it as a draft, under a key
// that names the question's version (drafts.js), and a revised question takes over the comment
// typed for the version it replaced. Back to the drawing board sends it too, from wherever it is
// offered (drawing-board.js).

/** @import { QuestionView } from "./types.ts" */

import { keep, kept, typed } from './drafts.js';

/** Carries the comment kept for the version a revised question replaced over to its own version,
 * unless one is kept for it already; returns the key the answer's box of `question` keeps its
 * comment under, and the replaced version's key.
 * @param {QuestionView} question
 * @returns {{ key: string, from: string | null }} */
export function takeOverComment(question) {
  const key = commentKey(question, question.version);
  const from = question.revision ? commentKey(question, question.revision.replaced_version) : null;
  const replaced = from === null ? null : kept(from);
  if (replaced !== null && kept(key) === null) keep(key, replaced);
  return { key, from };
}

/** The comment typed for `question`, whether or not the page shows its box, or the one typed for
 * the version a revised question replaced while none is kept for its own; `''` when it holds
 * only white space. It writes no draft.
 * @param {QuestionView} question */
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
