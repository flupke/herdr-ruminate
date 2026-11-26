import { DESK } from './desk.js';
import { DIAGRAM_SOURCE, roomOf, wholeWidth } from './diagrams.js';
import { h } from './dom.js';

const KEY = 'explore-chat-width';
const STEP = 16;
const BIG_STEP = 64;

export class ChatWidth {
  /**
   * @param {HTMLElement} chat
   * @param {HTMLElement} log
   */
  constructor(chat, log) {
    this.chat = chat;
    this.log = log;
    /** @type {number | null} */
    this.fitted = null;
    /** @type {number | null} */
    this.chosen = load();
    this.edge = h('div', {
      class: 'chat-edge',
      role: 'separator',
      tabindex: 0,
      'aria-orientation': 'vertical',
      'aria-label': 'Width of the chat',
      'aria-controls': 'chat',
      title: 'Drag to set the width; double-click to fit the messages',
    });
    chat.append(this.edge);
    this.edge.addEventListener('pointerdown', (event) => this.drag(event));
    this.edge.addEventListener('dblclick', () => this.fitAgain());
    this.edge.addEventListener('keydown', (event) => this.key(event));
    this.edge.addEventListener('focus', () => this.describe());
    this.apply();
  }

  /** @returns {boolean} */
  fit() {
    if (this.chat.hidden || !DESK.matches) return false;
    const wide = [...this.log.querySelectorAll(`.chat-message :is(.table-frame, pre:not(${DIAGRAM_SOURCE}, .diagram-error), .diagram.failed > pre)`)];
    const figures = /** @type {HTMLElement[]} */ ([...this.log.querySelectorAll('.chat-message figure.diagram:not(.failed)')]);
    const width = this.chat.getBoundingClientRect().width;
    /** @param {Element} element */
    const around = (element) => width - (element.parentElement ? roomOf(element.parentElement) : width);
    const drawn = figures.map((figure) => wholeWidth(figure) + around(figure));
    const arounds = wide.map(around);
    this.chat.classList.add('measuring');
    const needs = wide.map((element, index) => element.getBoundingClientRect().width + (arounds[index] ?? 0));
    this.chat.classList.remove('measuring');
    needs.push(...drawn);
    this.fitted = Math.ceil(Math.max(0, ...needs));
    this.apply();
    return true;
  }

  apply() {
    const want = this.chosen ?? this.fitted;
    const root = document.documentElement.style;
    if (want === null) root.removeProperty('--chat-want');
    else root.setProperty('--chat-want', `${Math.round(want)}px`);
  }

  bounds() {
    const root = document.documentElement.style;
    const want = root.getPropertyValue('--chat-want');
    const resizing = this.chat.classList.contains('resizing');
    this.chat.classList.add('resizing');
    root.setProperty('--chat-want', '0px');
    const min = this.chat.getBoundingClientRect().width;
    root.setProperty('--chat-want', '100000px');
    const max = this.chat.getBoundingClientRect().width;
    if (want) root.setProperty('--chat-want', want);
    else root.removeProperty('--chat-want');
    // Reading the layout makes the browser apply the width before the transitions return.
    this.chat.getBoundingClientRect();
    if (!resizing) this.chat.classList.remove('resizing');
    return { min, max };
  }

  describe() {
    if (this.chat.hidden || document.activeElement !== this.edge) return;
    const { min, max } = this.bounds();
    const now = this.chat.getBoundingClientRect().width;
    this.edge.setAttribute('aria-valuemin', String(Math.round(min)));
    this.edge.setAttribute('aria-valuemax', String(Math.round(max)));
    this.edge.setAttribute('aria-valuenow', String(Math.round(now)));
    this.edge.setAttribute('aria-valuetext', `${Math.round(now)} pixels${this.chosen === null ? ', fitted to the messages' : ''}`);
  }

  /**
   * @param {number} width
   * @param {{ min: number, max: number }} bounds
   * @param {boolean} keep
   */
  choose(width, bounds, keep) {
    this.chosen = Math.round(Math.min(bounds.max, Math.max(bounds.min, width)));
    this.apply();
    if (keep) save(this.chosen);
  }

  fitAgain() {
    this.chosen = null;
    save(null);
    if (!this.fit()) this.apply();
    this.describe();
  }

  /** @param {PointerEvent} event */
  drag(event) {
    if (event.button !== 0) return;
    event.preventDefault();
    const edge = this.edge;
    edge.setPointerCapture(event.pointerId);
    const from = event.clientX;
    const start = this.chat.getBoundingClientRect().width;
    const bounds = this.bounds();
    let moved = false;
    this.chat.classList.add('resizing');
    /** @param {PointerEvent} move */
    const follow = (move) => {
      if (move.clientX === from && !moved) return;
      moved = true;
      this.choose(start + move.clientX - from, bounds, false);
    };
    const end = () => {
      edge.removeEventListener('pointermove', follow);
      edge.removeEventListener('pointerup', end);
      edge.removeEventListener('pointercancel', end);
      this.chat.classList.remove('resizing');
      if (moved && this.chosen !== null) save(this.chosen);
      this.describe();
    };
    edge.addEventListener('pointermove', follow);
    edge.addEventListener('pointerup', end);
    edge.addEventListener('pointercancel', end);
  }

  /** @param {KeyboardEvent} event */
  key(event) {
    const step = event.shiftKey ? BIG_STEP : STEP;
    const width = this.chat.getBoundingClientRect().width;
    const bounds = this.bounds();
    /** @type {Record<string, () => void>} */
    const keys = {
      ArrowLeft: () => this.choose(width - step, bounds, true),
      ArrowRight: () => this.choose(width + step, bounds, true),
      Home: () => this.choose(bounds.min, bounds, true),
      End: () => this.choose(bounds.max, bounds, true),
      Enter: () => this.fitAgain(),
    };
    const act = keys[event.key];
    if (!act) return;
    event.preventDefault();
    act();
    if (event.key !== 'Enter') this.describe();
  }
}

function load() {
  try {
    const kept = Number(localStorage.getItem(KEY));
    return kept > 0 ? kept : null;
  } catch {
    return null;
  }
}

/** @param {number | null} width */
function save(width) {
  try {
    if (width === null) localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, String(width));
  } catch {
    // The browser may refuse this page its storage.
  }
}
