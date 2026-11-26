/** @import { MarkTally, PageView, RailStep, Tally } from "./types.ts" */

import { changeSize, fileCount } from './change-size.js';
import { h, keyOf, Region } from './dom.js';

/** @param {PageView} view */
export function measured(view) {
  const tally = view.tally;
  return view.rail.length > 0 && tally !== null && tally.files.length > 0 ? tally : null;
}

/** @typedef {'hover' | 'pinned' | null} Open */

const SEGMENTS = /** @type {const} */ (['answers', 'others', 'jev', 'pending']);

const LEAVE_GRACE = 180;
const PANEL_ROOM = 16;
const LEAST_HEIGHT = 240;

export class Meter {
  /** @param {HTMLElement} line */
  constructor(line) {
    this.segments = Object.fromEntries(SEGMENTS.map((name) => [name, h('span', { class: `meter-segment ${name}` })]));
    this.glow = h('span', { class: 'meter-glow', 'aria-hidden': 'true' });
    this.strip = h('button', {
      class: 'meter-strip',
      type: 'button',
      'aria-expanded': 'false',
      'aria-controls': 'meter-window',
      onclick: () => this.toggle(),
      onmouseenter: (/** @type {Event} */ event) => this.enter(/** @type {MouseEvent} */ (event)),
      onmouseleave: () => {
        this.pointed = false;
        this.leave();
      },
      onfocus: () => {
        this.focused = this.strip.matches(':focus-visible');
        this.refresh();
      },
      onblur: () => {
        this.focused = false;
        this.refresh();
      },
    });
    this.window = h('div', {
      class: 'meter-window',
      id: 'meter-window',
      role: 'group',
      'aria-label': 'Review marks of the change',
      onmouseenter: () => this.enter(null),
      onmouseleave: () => this.leave(),
    });
    this.root = h(
      'div',
      { class: 'meter', hidden: true },
      h('div', { class: 'meter-bar', 'aria-hidden': 'true' }, SEGMENTS.map((name) => this.segments[name])),
      this.glow,
      this.strip,
      this.window,
    );
    line.append(this.root);
    this.content = new Region(this.window, 'meter-window');
    /** @type {Open} */
    this.open = null;
    this.pointed = false;
    this.focused = false;
    /** @type {ReturnType<typeof setTimeout> | undefined} */
    this.closing = undefined;
    /** @type {{ marked: number, pending: number } | null} */
    this.previous = null;
    document.addEventListener('keydown', (event) => {
      if (event.key !== 'Escape' || !this.open) return;
      if (this.open === 'pinned') event.preventDefault();
      this.close();
    });
    document.addEventListener('click', (event) => {
      if (this.open === 'pinned' && !event.composedPath().includes(this.root)) this.close();
    });
    window.addEventListener('resize', () => {
      if (!this.open) return;
      const line = this.root.getBoundingClientRect();
      this.place(line.left + this.window.offsetLeft + this.window.offsetWidth / 2, true);
    });
  }

  /** @param {PageView} view */
  update(view) {
    const tally = measured(view);
    this.root.hidden = !tally;
    if (!tally) {
      this.previous = null;
      if (this.open) this.close();
      return;
    }
    const change = tally.change;
    this.strip.setAttribute(
      'aria-label',
      `Lines reviewed: ${change.share.percent}%, ${lineShare(change.share.marked, change.share.changed)}`,
    );
    this.draw(change);
    this.content.show(keyOf([tally, view.rail]), () => windowContent(tally, answeredSteps(view.rail)));
  }

  /** @param {Tally} change */
  draw(change) {
    const total = change.share.changed;
    const widths = segmentLines(change);
    let left = 0;
    for (const name of SEGMENTS) {
      const segment = this.segments[name];
      const share = total > 0 ? (widths[name] / total) * 100 : 0;
      if (name === 'pending' && share === 0 && segment.dataset.lines) {
        segment.style.opacity = '0';
        delete segment.dataset.lines;
        continue;
      }
      segment.style.left = `${left}%`;
      segment.style.width = `${share}%`;
      segment.style.opacity = '';
      if (name === 'pending') {
        if (share > 0) segment.dataset.lines = String(widths[name]);
        else delete segment.dataset.lines;
      }
      left += share;
    }
    const done = total > 0 ? (change.share.marked / total) * 100 : 0;
    this.glow.style.width = `${done}%`;
    const now = { marked: change.share.marked, pending: widths.pending };
    if (this.previous && this.previous.pending > 0 && now.pending === 0 && now.marked > this.previous.marked) {
      this.glow.classList.remove('glowing');
      void this.glow.offsetWidth;
      this.glow.classList.add('glowing');
    }
    this.previous = now;
  }

  /** @param {MouseEvent | null} pointer */
  enter(pointer) {
    clearTimeout(this.closing);
    if (pointer) {
      this.pointed = true;
      if (this.open !== 'pinned') this.place(pointer.clientX);
    }
    if (!this.open) this.show('hover');
    this.refresh();
  }

  leave() {
    this.refresh();
    if (this.open !== 'hover') return;
    clearTimeout(this.closing);
    this.closing = setTimeout(() => {
      if (this.open === 'hover') this.close();
    }, LEAVE_GRACE);
  }

  toggle() {
    if (this.open === 'pinned') {
      this.close();
      return;
    }
    if (!this.open) this.place(null);
    this.show('pinned');
  }

  /** @param {Open} open */
  show(open) {
    this.open = open;
    this.refresh();
  }

  close() {
    clearTimeout(this.closing);
    this.open = null;
    this.refresh();
  }

  refresh() {
    this.root.classList.toggle('open', this.open !== null);
    this.root.classList.toggle('grown', this.open !== null || this.pointed || this.focused);
    this.strip.setAttribute('aria-expanded', String(this.open !== null));
  }

  /**
   * @param {number | null} x
   * @param {boolean} [resized]
   */
  place(x, resized = false) {
    const line = this.root.getBoundingClientRect();
    const gutter = this.gutters();
    const css = getComputedStyle(this.window);
    const least = parseFloat(css.getPropertyValue('--meter-least')) || 0;
    const most = parseFloat(css.getPropertyValue('--meter-most')) || Infinity;
    let right = line.width - gutter.right;
    const panel = document.querySelector('.screen:not([hidden]) .panel');
    if (panel instanceof HTMLElement) {
      const box = panel.getBoundingClientRect();
      const room = box.left - line.left - PANEL_ROOM;
      if (room >= gutter.left + least) right = Math.min(right, room);
    }
    if (!this.open || resized) {
      // The browser counts the list's scroll bar in the width only once the height is capped.
      const style = this.window.style;
      this.fitHeight();
      style.removeProperty('--meter-width');
      style.setProperty('--meter-room', `${Math.max(least, Math.min(most, right - gutter.left))}px`);
      style.setProperty('--meter-width', `${Math.ceil(this.window.getBoundingClientRect().width)}px`);
    }
    const width = this.window.offsetWidth;
    const wanted = x === null ? gutter.left : x - line.left - width / 2;
    const left = Math.max(gutter.left, Math.min(wanted, right - width));
    this.window.style.left = `${left}px`;
  }

  fitHeight() {
    const top = this.root.getBoundingClientRect().top + this.window.offsetTop;
    const room = window.innerHeight - top - this.gutters().edge;
    this.window.style.maxHeight = `${Math.max(LEAST_HEIGHT, room)}px`;
  }

  gutters() {
    const header = this.root.closest('.masthead');
    const css = getComputedStyle(header ?? document.documentElement);
    const edge = parseFloat(css.getPropertyValue('--gutter')) || 16;
    if (!header) return { left: edge, right: edge, edge };
    return { left: parseFloat(css.paddingLeft), right: parseFloat(css.paddingRight), edge };
  }
}

/**
 * @param {MarkTally} tally
 * @param {number[]} answered
 */
function windowContent(tally, answered) {
  const change = tally.change;
  const marked = change.marked;
  const { answers, others, jev, pending } = segmentLines(change);
  const largest = Math.max(1, ...tally.files.map((file) => file.tally.share.changed));
  return [
    h(
      'div',
      { class: 'meter-totals' },
      h('span', { class: 'meter-share' }, `${change.share.percent}% reviewed`),
      h('span', { class: 'meter-lines' }, lineShare(change.share.marked, change.share.changed)),
      h('span', { class: 'meter-size' }, changeSize(tally), ` · ${fileCount(tally.files.length)}`),
    ),
    h(
      'div',
      { class: 'meter-legend' },
      legendRow('answers', answered.length > 0 ? `Your answers · ${answered.map((n) => `Q${n}`).join(', ')}` : 'Your answers', answers),
      others > 0 ? legendRow('others', othersLabel(marked.by_hand, marked.other_rounds), others) : null,
      legendRow('jev', `Jev at the start ${marked.jev} · not relevant ${marked.not_relevant}`, jev),
      pending > 0 ? legendRow('pending', 'This question marks when you answer', pending) : null,
      legendRow('left', 'Left to explore', change.left),
    ),
    h(
      'div',
      { class: 'meter-files', role: 'list', 'aria-label': 'Files of the change' },
      tally.files.map((file) => {
        const size = file.tally.share.changed;
        const whole = file.whole;
        const state = whole ? (whole.marked_by ? 'marked whole' : 'left whole') : `${file.tally.left} left`;
        return h(
          'div',
          { class: 'meter-file', role: 'listitem', title: file.path, 'aria-label': file.path },
          filePath(file.path),
          whole ? h('span', {}) : fileBar(file.tally, (size / largest) * 100),
          h('span', { class: 'meter-file-state' }, state, file.cited ? [' · ', h('span', { class: 'cited' }, 'cited here')] : null),
        );
      }),
    ),
  ];
}

/** @param {string} path */
function filePath(path) {
  const slash = path.lastIndexOf('/') + 1;
  return h(
    'span',
    { class: 'meter-path' },
    h(
      'bdi',
      { dir: 'ltr' },
      slash > 0 ? h('span', { class: 'meter-dir' }, path.slice(0, slash)) : null,
      h('span', { class: 'meter-name' }, path.slice(slash)),
    ),
  );
}

/**
 * @param {number} byHand
 * @param {number} otherRounds
 */
function othersLabel(byHand, otherRounds) {
  const parts = [];
  if (byHand > 0) parts.push(`By hand ${byHand}`);
  if (otherRounds > 0) parts.push(`${parts.length > 0 ? 'earlier' : 'Earlier'} rounds ${otherRounds}`);
  return parts.join(' · ');
}

/**
 * @param {string} kind
 * @param {string} label
 * @param {number} count
 */
function legendRow(kind, label, count) {
  return [h('span', { class: `meter-swatch ${kind}` }), h('span', {}, label), h('span', { class: 'meter-count' }, String(count))];
}

/**
 * @param {Tally} tally
 * @param {number} width
 */
function fileBar(tally, width) {
  const total = tally.share.changed;
  const parts = segmentLines(tally);
  const bar = h('span', { class: 'meter-file-bar' });
  bar.style.width = `${width}%`;
  for (const name of SEGMENTS) {
    if (parts[name] === 0) continue;
    const part = h('span', { class: `meter-swatch ${name}` });
    part.style.width = `${(parts[name] / total) * 100}%`;
    bar.append(part);
  }
  return bar;
}

/**
 * @param {Tally} tally
 * @returns {Record<typeof SEGMENTS[number], number>}
 */
function segmentLines(tally) {
  const marked = tally.marked;
  return {
    answers: marked.answers,
    others: marked.by_hand + marked.other_rounds,
    jev: marked.jev + marked.not_relevant,
    pending: tally.pending.reviewed + tally.pending.not_relevant,
  };
}

/**
 * @param {number} marked
 * @param {number} changed
 */
function lineShare(marked, changed) {
  return `${marked} of ${changed} changed ${changed === 1 ? 'line' : 'lines'}`;
}

/** @param {RailStep[]} rail */
function answeredSteps(rail) {
  return rail.flatMap((step) => {
    if (step.step.kind !== 'question') return [];
    const state = step.state;
    const answered = state.kind === 'done' || (state.kind === 'current' && state.working);
    return answered ? [step.step.number] : [];
  });
}
