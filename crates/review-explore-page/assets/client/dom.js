/**
 * @typedef {Record<string, string | number | boolean | null | undefined | EventListener>} Props
 */

/**
 * @typedef {Node | string | number | null | undefined | false} Leaf
 * @typedef {Leaf | Children[]} Children
 */

/**
 * @template {keyof HTMLElementTagNameMap} K
 * @param {K} tag
 * @param {Props} [props]
 * @param {...Children} children
 * @returns {HTMLElementTagNameMap[K]}
 */
export function h(tag, props = {}, ...children) {
  const element = document.createElement(tag);
  for (const [name, value] of Object.entries(props)) {
    if (value === false || value === null || value === undefined) continue;
    if (typeof value === 'function') {
      element.addEventListener(name.slice(2), value);
    } else {
      element.setAttribute(name, value === true ? '' : String(value));
    }
  }
  append(element, children);
  return element;
}

/**
 * @param {Element} element
 * @param {Children} children
 */
function append(element, children) {
  if (Array.isArray(children)) {
    for (const child of children) append(element, child);
  } else if (children !== null && children !== undefined && children !== false) {
    element.append(typeof children === 'number' ? String(children) : children);
  }
}

/**
 * @param {string} text
 * @returns {(string | HTMLElement)[]}
 */
export function codeSpans(text) {
  /** @type {(string | HTMLElement)[]} */
  const nodes = [];
  let written = 0;
  let at = text.indexOf('`');
  while (at >= 0) {
    const width = runLength(text, at);
    const close = closingRun(text, at + width, width);
    if (close < 0) {
      at = text.indexOf('`', at + width);
      continue;
    }
    if (at > written) nodes.push(text.slice(written, at));
    nodes.push(h('code', { class: 'inline-code' }, spanText(text.slice(at + width, close))));
    written = close + width;
    at = text.indexOf('`', written);
  }
  if (written < text.length) nodes.push(text.slice(written));
  return nodes;
}

/**
 * @param {string} text
 * @param {number} at
 */
function runLength(text, at) {
  let end = at;
  while (text[end] === '`') end += 1;
  return end - at;
}

/**
 * @param {string} text
 * @param {number} from
 * @param {number} width
 */
function closingRun(text, from, width) {
  for (let at = text.indexOf('`', from); at >= 0; ) {
    const run = runLength(text, at);
    if (run === width) return at;
    at = text.indexOf('`', at + run);
  }
  return -1;
}

/** @param {string} inside */
function spanText(inside) {
  const line = inside.replace(/\r\n|\r|\n/g, ' ');
  return line.startsWith(' ') && line.endsWith(' ') && /[^ ]/.test(line) ? line.slice(1, -1) : line;
}

/**
 * @param {Element} element
 * @param {string} html
 */
export function setRenderedMarkdown(element, html) {
  element.innerHTML = html;
  for (const table of element.querySelectorAll('table')) {
    const frame = document.createElement('div');
    frame.className = 'table-frame';
    table.replaceWith(frame);
    frame.append(table);
  }
}

/**
 * @param {Element} figure
 * @param {string} svg
 */
export function setDiagramDrawing(figure, svg) {
  figure.innerHTML = svg;
}

/**
 * @param {string} html
 * @param {string} [extra]
 */
export function markdown(html, extra) {
  const element = h('div', { class: extra ? `markdown ${extra}` : 'markdown' });
  setRenderedMarkdown(element, html);
  return element;
}

export class Region {
  /**
   * @param {Element} parent
   * @param {string} name
   */
  constructor(parent, name) {
    this.anchor = document.createComment(name);
    parent.append(this.anchor);
    /** @type {Node[]} */
    this.nodes = [];
    /** @type {string | undefined} */
    this.key = undefined;
    /** @type {unknown} */
    this.current = undefined;
  }

  /**
   * @param {string | null} key
   * @param {() => Node | Node[] | null} build
   * @returns {boolean}
   */
  show(key, build) {
    if (key === null) {
      this.clear();
      return false;
    }
    if (key === this.key) return false;
    keepingFocus(() => {
      this.clear();
      const built = build();
      this.nodes = built === null ? [] : Array.isArray(built) ? built : [built];
      for (const node of this.nodes) this.anchor.before(node);
    });
    this.key = key;
    return true;
  }

  /**
   * @template {{ element: Node }} C
   * @param {string} key
   * @param {() => C} create
   * @returns {C}
   */
  component(key, create) {
    this.show(key, () => {
      const component = create();
      this.current = component;
      return component.element;
    });
    return /** @type {C} */ (this.current);
  }

  clear() {
    for (const node of this.nodes) node.parentNode?.removeChild(node);
    this.nodes = [];
    this.key = undefined;
    this.current = undefined;
  }
}

/** @param {unknown} data */
export function keyOf(data) {
  return JSON.stringify(data);
}

/** @param {() => void} change */
function keepingFocus(change) {
  const focused = document.activeElement;
  const key = focused instanceof HTMLElement ? focused.dataset.focusKey : undefined;
  if (key) {
    change();
    if (focused?.isConnected) return;
    const again = document.querySelector(`[data-focus-key="${CSS.escape(key)}"]`);
    if (again instanceof HTMLElement && !again.matches(':disabled')) again.focus({ preventScroll: true });
    return;
  }
  const typed = focused instanceof HTMLTextAreaElement || focused instanceof HTMLInputElement;
  const draft = typed ? focused.dataset.draft : undefined;
  if (!draft || !typed) {
    change();
    return;
  }
  const { selectionStart, selectionEnd, selectionDirection, scrollTop } = focused;
  change();
  if (focused.isConnected) return;
  const escaped = CSS.escape(draft);
  const again =
    document.querySelector(`[data-draft="${escaped}"]`) ??
    document.querySelector(`[data-draft-from="${escaped}"]`);
  if (!(again instanceof HTMLTextAreaElement || again instanceof HTMLInputElement)) return;
  again.focus({ preventScroll: true });
  again.setSelectionRange(selectionStart, selectionEnd, selectionDirection ?? undefined);
  again.scrollTop = scrollTop;
}
