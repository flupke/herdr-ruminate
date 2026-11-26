import { h } from './dom.js';

const UNMARKED = 'pre, svg, .diagram, .code-frame, textarea, input, button, a, .glossary, .term, .term-tip, .past-banner';

const WORD = '[\\p{L}\\p{M}\\p{N}_]';

/** @param {string} term */
function termPattern(term) {
  return term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace(/\s+/g, '\\s+');
}

export class Glossary {
  /** @param {HTMLElement[]} roots */
  constructor(roots) {
    this.roots = roots;
    /** @type {[string, string][]} */
    this.entries = [];
    /** @type {RegExp | null} */
    this.pattern = null;
    this.key = '';
    /** @type {WeakMap<HTMLElement, string>} */
    this.defined = new WeakMap();
    this.tip = h('div', { class: 'term-tip', id: 'term-tip', role: 'tooltip', hidden: true });
    document.body.append(this.tip);
    /** @type {HTMLElement | null} */
    this.shown = null;
    this.observer = new MutationObserver((records) => {
      for (const record of records) for (const node of record.addedNodes) this.mark(node);
      this.observer.takeRecords();
    });
    for (const root of roots) this.observer.observe(root, { childList: true, subtree: true });
    document.addEventListener('click', (event) => {
      if (this.shown && !(event.target instanceof Node && this.shown.contains(event.target))) this.hide();
    });
    document.addEventListener('keydown', (event) => {
      if (event.key === 'Escape' && this.shown) {
        event.preventDefault();
        this.hide();
      }
    });
    const follow = () => {
      if (this.shown) this.place(this.shown);
    };
    addEventListener('scroll', follow, { capture: true, passive: true });
    addEventListener('resize', follow);
  }

  /** @param {{ [term in string]?: string }} glossary */
  set(glossary) {
    const entries = Object.entries(glossary).filter(
      /** @returns {entry is [string, string]} */ (entry) => typeof entry[1] === 'string',
    );
    const key = JSON.stringify(entries);
    if (key === this.key) return;
    this.key = key;
    this.entries = entries;
    const alternatives = entries
      .map(([term]) => term)
      .sort((a, b) => b.length - a.length)
      .map(termPattern);
    this.pattern =
      alternatives.length > 0 ? new RegExp(`(?<!${WORD})(?:${alternatives.join('|')})(?!${WORD})`, 'giu') : null;
    this.hide();
    for (const root of this.roots) {
      unmark(root);
      this.mark(root);
    }
    this.observer.takeRecords();
  }

  /** @param {Node} node */
  mark(node) {
    const pattern = this.pattern;
    if (!pattern) return;
    const start = node instanceof Element ? node : node.parentElement;
    if (!start || start.closest(UNMARKED)) return;
    /** @type {Text[]} */
    const texts = [];
    if (node instanceof Text) texts.push(node);
    else {
      const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT, {
        acceptNode: (text) =>
          text.parentElement?.closest(UNMARKED) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
      });
      for (let text = walker.nextNode(); text; text = walker.nextNode()) texts.push(/** @type {Text} */ (text));
    }
    for (const text of texts) this.markText(text, pattern);
  }

  /** @param {Text} text @param {RegExp} pattern */
  markText(text, pattern) {
    const value = text.data;
    pattern.lastIndex = 0;
    /** @type {(Node | string)[]} */
    const parts = [];
    let at = 0;
    for (const match of value.matchAll(pattern)) {
      const entry = this.entryOf(match[0]);
      if (!entry) continue;
      const index = match.index ?? 0;
      if (index > at) parts.push(value.slice(at, index));
      parts.push(this.termMark(match[0], entry));
      at = index + match[0].length;
    }
    if (parts.length === 0) return;
    if (at < value.length) parts.push(value.slice(at));
    text.replaceWith(...parts);
  }

  /** @param {string} word */
  entryOf(word) {
    return this.entries.find(([term]) => new RegExp(`^(?:${termPattern(term)})$`, 'iu').test(word));
  }

  /**
   * @param {string} word
   * @param {[string, string]} entry
   */
  termMark(word, entry) {
    const mark = h('span', { class: 'term', tabindex: 0, role: 'button', 'aria-describedby': 'term-tip' }, word);
    this.defined.set(mark, entry[1]);
    const show = () => this.show(mark);
    mark.addEventListener('mouseenter', show);
    mark.addEventListener('focus', show);
    mark.addEventListener('mouseleave', () => this.hide());
    mark.addEventListener('blur', () => this.hide());
    let open = false;
    mark.addEventListener('pointerdown', (event) => {
      open = event.pointerType !== 'mouse' && this.shown === mark;
    });
    mark.addEventListener('click', (event) => {
      event.preventDefault();
      if (open) this.hide();
      else show();
      open = false;
    });
    return mark;
  }

  /** @param {HTMLElement} mark */
  show(mark) {
    const definition = this.defined.get(mark);
    if (definition === undefined) return;
    this.shown = mark;
    this.tip.replaceChildren(definition);
    this.place(mark);
  }

  /** @param {HTMLElement} mark */
  place(mark) {
    this.tip.style.left = '0px';
    this.tip.style.top = '0px';
    this.tip.hidden = false;
    const box = mark.getBoundingClientRect();
    const width = this.tip.offsetWidth;
    const height = this.tip.offsetHeight;
    const left = Math.max(8, Math.min(box.left, window.innerWidth - width - 8));
    const below = box.bottom + 6 + height < window.innerHeight;
    this.tip.style.left = `${left}px`;
    this.tip.style.top = `${below ? box.bottom + 6 : box.top - 6 - height}px`;
  }

  hide() {
    this.shown = null;
    this.tip.hidden = true;
  }
}

/** @param {{ [term in string]?: string }} glossary */
export function glossaryList(glossary) {
  const entries = Object.entries(glossary)
    .filter(/** @returns {entry is [string, string]} */ (entry) => typeof entry[1] === 'string')
    .sort(([a], [b]) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
  if (entries.length === 0) return null;
  return h(
    'section',
    { class: 'glossary', 'aria-labelledby': 'glossary-title' },
    h('h2', { class: 'eyebrow', id: 'glossary-title' }, 'Glossary'),
    h(
      'dl',
      {},
      entries.map(([term, definition]) => [h('dt', {}, term), h('dd', {}, definition)]),
    ),
  );
}

/** @param {HTMLElement} root */
function unmark(root) {
  for (const mark of root.querySelectorAll('.term')) {
    const parent = mark.parentNode;
    mark.replaceWith(mark.textContent ?? '');
    parent?.normalize();
  }
}
