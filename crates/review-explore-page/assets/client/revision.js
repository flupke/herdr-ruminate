/** @import { ShortRevision } from "./types.ts" */

import { h } from './dom.js';

/** @param {ShortRevision} revision */
export function revisionCode(revision) {
  return h(
    'code',
    { class: 'revision' },
    revision.prefix ? h('span', { class: 'revision-prefix' }, revision.prefix) : null,
    revision.rest ? h('span', { class: 'revision-rest' }, revision.rest) : null,
  );
}

/** @param {ShortRevision} revision */
export function revisionText(revision) {
  return revision.prefix + revision.rest;
}
