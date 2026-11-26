import { requestChat } from './chat.js';
import { h } from './dom.js';

const LONGEST = 1000;

const NOT_READING = '.panel, form, textarea, input, button, .masthead, .left-column';

export class QuotePicker {
  /** @param {HTMLElement} main */
  constructor(main) {
    this.main = main;
    this.writable = false;
    this.button = h(
      'button',
      {
        class: 'chat-pick',
        type: 'button',
        hidden: true,
        // A press in the browser would clear the selection.
        onpointerdown: (/** @type {Event} */ event) => event.preventDefault(),
        onclick: () => this.pick(),
      },
      'Add to chat',
    );
    document.body.append(this.button);
    this.passage = '';
    document.addEventListener('selectionchange', () => this.follow());
  }

  /** @param {boolean} writable */
  update(writable) {
    this.writable = writable;
    if (!writable) this.hide();
  }

  follow() {
    const selection = document.getSelection();
    const passage = selection ? quotable(selection, this.main) : null;
    if (!this.writable || !selection || !passage) {
      this.hide();
      return;
    }
    this.passage = passage;
    const box = selection.getRangeAt(0).getBoundingClientRect();
    this.button.hidden = false;
    const width = this.button.offsetWidth;
    const left = Math.min(Math.max(8, box.left + box.width / 2 - width / 2), window.innerWidth - width - 8);
    const above = box.top - this.button.offsetHeight - 8;
    const top = above >= 8 ? above : box.bottom + 8;
    this.button.style.left = `${left + window.scrollX}px`;
    this.button.style.top = `${top + window.scrollY}px`;
  }

  pick() {
    const passage = this.passage;
    this.hide();
    document.getSelection()?.removeAllRanges();
    requestChat(passage);
  }

  hide() {
    this.button.hidden = true;
    this.passage = '';
  }
}

/**
 * @param {Selection} selection
 * @param {HTMLElement} main
 */
function quotable(selection, main) {
  if (selection.isCollapsed || selection.rangeCount === 0) return null;
  const inReading = (/** @type {Node | null} */ node) => {
    const element = node instanceof Element ? node : node?.parentElement;
    return element !== null && element !== undefined && main.contains(element) && !element.closest(NOT_READING);
  };
  if (!inReading(selection.anchorNode) || !inReading(selection.focusNode)) return null;
  const text = selection.toString().replace(/\s+/g, ' ').trim();
  if (!text) return null;
  return text.length > LONGEST ? `${text.slice(0, LONGEST - 1)}…` : text;
}
