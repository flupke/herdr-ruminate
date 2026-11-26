import { DESK } from './desk.js';
import { h } from './dom.js';

const OPENED = 'explore:left-column';

const DRAG = 60;

/** @param {string} name */
function token(name) {
  return parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name)) || 0;
}

const RISES = CSS.supports('animation-timeline: scroll()');

export class LeftColumn {
  /**
   * @param {object} parts
   * @param {'aside' | 'nav'} parts.tag
   * @param {string} parts.kind
   * @param {string} parts.label
   * @param {string} parts.title
   * @param {string} parts.closeLabel
   * @param {(refocus: boolean) => void} parts.closed
   */
  constructor({ tag, kind, label, title, closeLabel, closed }) {
    this.closed = closed;
    this.open = false;
    this.element = h(tag, { class: `left-column ${kind}`, id: kind, 'aria-label': label, hidden: true });
    this.meta = h('span', { class: 'left-meta' });
    const grabber = h('div', { class: 'left-grabber', 'aria-hidden': 'true' });
    this.dragSheet(grabber);
    this.element.append(
      grabber,
      h(
        'header',
        { class: 'left-head' },
        h('h2', { class: 'left-title' }, title),
        this.meta,
        h('button', { class: 'left-close', type: 'button', 'aria-label': closeLabel, onclick: () => this.close(true) }, '×'),
      ),
    );
    this.scrim = h('div', { class: 'left-scrim', hidden: true, onclick: () => this.close(false) });
    document.body.append(this.scrim, this.element);
    document.addEventListener(OPENED, (event) => {
      if (/** @type {CustomEvent<LeftColumn>} */ (event).detail !== this) this.close(false);
    });
    // In the browser, the window hears a key after every listener of the document.
    addEventListener('keydown', (event) => {
      if (event.key !== 'Escape' || !this.open || event.defaultPrevented) return;
      if (event.target instanceof Element && event.target.closest('dialog')) return;
      this.close(true);
    });
    if (!RISES) {
      addEventListener('scroll', () => this.rise(), { passive: true });
      addEventListener('resize', () => this.rise());
    }
  }

  rise() {
    if (this.element.hidden) return;
    this.element.style.top = '';
    if (!DESK.matches) return;
    const top = parseFloat(getComputedStyle(this.element).top);
    const held = token('--sticky-top');
    this.element.style.top = `${Math.max(held, top - scrollY)}px`;
  }

  /** @returns {boolean} */
  show() {
    const opened = !this.open;
    this.open = true;
    this.element.hidden = false;
    if (!RISES) this.rise();
    this.scrim.hidden = false;
    requestAnimationFrame(() => {
      if (this.open) this.element.classList.add('open');
    });
    if (opened) document.dispatchEvent(new CustomEvent(OPENED, { detail: this }));
    return opened;
  }

  /** @param {boolean} refocus */
  close(refocus) {
    if (!this.open) return;
    this.open = false;
    this.element.classList.remove('open', 'full');
    setTimeout(() => {
      if (!this.open) this.element.hidden = true;
    }, token('--drawer-out'));
    this.scrim.hidden = true;
    this.closed(refocus);
  }

  /** @param {HTMLElement} grabber */
  dragSheet(grabber) {
    /** @type {number | null} */
    let from = null;
    grabber.addEventListener('pointerdown', (event) => {
      from = event.clientY;
      grabber.setPointerCapture(event.pointerId);
    });
    grabber.addEventListener('pointerup', (event) => {
      if (from === null) return;
      const moved = event.clientY - from;
      from = null;
      const full = this.element.classList.contains('full');
      if (moved < -DRAG) this.element.classList.add('full');
      else if (moved > DRAG && full) this.element.classList.remove('full');
      else if (moved > DRAG) this.close(false);
    });
  }
}
