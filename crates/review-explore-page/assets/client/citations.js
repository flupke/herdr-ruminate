/** @import { CitationView, RowView } from "./types.ts" */

import { disclosure } from './disclosure.js';
import { h } from './dom.js';

/**
 * @param {CitationView} citation
 * @param {string} id
 */
export function citation(citation, id) {
  const added = citation.rows.length > 0 && citation.rows.every((row) => row.kind === 'added');
  return h(
    'section',
    { class: 'citation', 'aria-labelledby': id },
    h('p', { class: 'notes' }, citation.notes),
    h(
      'div',
      { class: 'code-frame' },
      h('h4', { class: 'code-head', id }, h('code', {}, citation.path), ' ', h('span', {}, citation.span)),
      citation.limitation !== null
        ? h('p', { class: 'limitation' }, citation.limitation)
        : h(
            'div',
            { class: 'code', tabindex: 0, role: 'group', 'aria-label': `Lines of ${citation.location}` },
            h('table', { class: added ? 'all-added' : null }, h('tbody', {}, citation.rows.map(row))),
          ),
    ),
  );
}

const SIGNS = { added: '+', removed: '−', context: ' ' };

/** @param {RowView} row */
function row(row) {
  return h(
    'tr',
    { class: row.kind },
    h('td', { class: 'number old' }, row.old_line ?? ''),
    h('td', { class: 'number new' }, row.new_line ?? ''),
    h('td', { class: 'sign' }, SIGNS[row.kind]),
    h('td', { class: 'line' }, tokens(row)),
  );
}

/** @param {RowView} row */
function tokens(row) {
  /** @type {(HTMLElement | string)[]} */
  const nodes = [];
  let indent = '';
  for (const token of row.tokens) {
    let text = token.text;
    if (nodes.length === 0) {
      const code = text.trimStart();
      indent += text.slice(0, text.length - code.length);
      text = code;
      if (!text) continue;
    }
    const last = nodes.length - 1;
    if (token.role) nodes.push(h('span', { class: `c-${token.role}` }, text));
    else if (typeof nodes[last] === 'string') nodes[last] += text;
    else nodes.push(text);
  }
  if (indent) nodes.unshift(h('span', { class: 'indent' }, indent));
  return nodes;
}

/** @type {Map<string, boolean>} */
const opened = new Map();

/**
 * @param {CitationView[]} citations
 * @param {number} number
 */
export function citationsSection(citations, number) {
  const [first, ...others] = citations;
  if (!first) return null;
  const id = (/** @type {number} */ index) => `question-${number}-citation-${index}`;
  const citationsOf = (/** @type {number} */ many) => `citation${many > 1 ? 's' : ''}`;
  const key = `${number}:${citations.map((each) => each.location).join(' ')}`;
  const more =
    others.length > 0
      ? disclosure(
          `${others.length} more ${citationsOf(others.length)}`,
          others.map((other, index) => citation(other, id(index + 2))),
          { open: opened.get(key) ?? false, onToggle: (open) => opened.set(key, open) },
        )
      : null;
  return h(
    'section',
    { class: 'citations', 'aria-labelledby': `question-${number}-citations` },
    h('h3', { class: 'eyebrow', id: `question-${number}-citations` }, 'Citations'),
    citation(first, id(1)),
    more?.element ?? null,
  );
}
