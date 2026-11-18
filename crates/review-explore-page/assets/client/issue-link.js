// A link to the page of an issue or a pull request the reviewed change names, in a new tab, in
// the accent (masthead.css): the masthead's Linear issues, and the pull request and tickets of a
// past round (past.js).

import { h } from './dom.js';

/**
 * @param {string} url the address of its page
 * @param {string} text what it shows: its number, `ENG-12`, `#7`, `PR #12`
 * @param {string} [label] what a screen reader says in place of `text`
 */
export function issueLink(url, text, label) {
  return h('a', { class: 'issue', href: url, target: '_blank', rel: 'noopener noreferrer', 'aria-label': label }, text);
}
