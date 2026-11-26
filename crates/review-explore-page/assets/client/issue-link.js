import { h } from './dom.js';

/**
 * @param {string} url
 * @param {string} text
 * @param {string} [label]
 */
export function issueLink(url, text, label) {
  return h('a', { class: 'issue', href: url, target: '_blank', rel: 'noopener noreferrer', 'aria-label': label }, text);
}
