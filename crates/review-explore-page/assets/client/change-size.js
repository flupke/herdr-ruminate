/** @import { MarkTally } from "./types.ts" */

import { h } from './dom.js';

/** @param {MarkTally} tally */
export function changeSize(tally) {
  const changed = tally.change.changed;
  return h(
    'span',
    { class: 'change-size' },
    h('span', { class: 'added' }, `+${changed.lines_added}`),
    ' ',
    h('span', { class: 'removed' }, `−${changed.lines_removed}`),
  );
}

/** @param {number} count */
export function fileCount(count) {
  return count === 1 ? '1 file' : `${count} files`;
}
