/** @import { DesignView, PageView, RailStep, ReviewName, Step, TabTitle } from "./types.ts" */
/** @import { Notifier, NotifyState } from "./notify.js" */

import { requestChat } from './chat.js';
import { DESK } from './desk.js';
import { h, keyOf, markdown, Region } from './dom.js';
import { issueLink } from './issue-link.js';
import { drawingBoardConfirmation } from './drawing-board.js';
import { backLink, requestPastList } from './past.js';
import { revisionCode, revisionText } from './revision.js';
import { earlierQuestion, quizItem, STAGE } from './route.js';

/** @typedef {'map' | 'menu' | null} Open */
/** @typedef {{ kind: 'design' } | { kind: 'question', number: number } | { kind: 'quiz' } | null} Viewing */

const RESET_HINT =
  'Reset closes this round for good: its agent can no longer post to it, and the page offers to start a new round. It stays readable as a past round, from this menu.';

export class Masthead {
  /**
   * @param {HTMLElement} header
   * @param {() => void} redraw
   * @param {Notifier} notifier
   */
  constructor(header, redraw, notifier) {
    this.header = header;
    this.redraw = redraw;
    this.notifier = notifier;
    const identity = h('div', { class: 'identity' }, h('p', { class: 'product' }, 'Explore'));
    header.replaceChildren(identity);
    this.review = new Region(identity, 'review');
    this.rail = new Region(header, 'rail');
    this.menu = new Region(header, 'menu');
    this.chatPlace = h('div', { class: 'masthead-chat', id: 'masthead-chat' });
    this.placeChat();
    DESK.addEventListener('change', () => this.placeChat());
    header.append(h('div', { class: 'masthead-line', id: 'masthead-line' }));
    /** @type {Open} */
    this.open = null;
    /** @type {'reset' | 'drawing-board' | null} */
    this.confirming = null;
    /** @type {string | null} */
    this.resets = null;
    /** @type {string | null} */
    this.boardFor = null;
    /** @type {Set<string | undefined>} */
    this.near = new Set();
    /** @type {string | null} */
    this.aimed = null;
    document.addEventListener('click', (event) => this.clickOutside(event));
    addEventListener('hashchange', () => {
      if (this.open) this.close(false);
    });
    addEventListener('resize', () => this.fit());
    document.addEventListener('keydown', (event) => {
      if (event.key !== 'Escape' || !this.open) return;
      event.preventDefault();
      this.close(true);
    });
  }

  /**
   * @param {PageView} view
   * @param {Viewing} viewing
   * @param {number} [unread]
   * @param {boolean} [quizLink]
   */
  update(view, viewing, unread = 0, quizLink = false) {
    const cover = view.start !== null;
    this.review.show(keyOf([view.review, cover]), () => reviewLine(view.review, cover));
    const rail = view.rail;
    const focused = document.activeElement === this.toggleOf('map');
    this.rail.show(keyOf([rail, view.design, viewing, quizLink]), () =>
      rail.length > 0
        ? railNav(rail, view.design, viewing, quizLink, () => this.toggle('map'), () => this.close(false))
        : null,
    );
    if (focused && document.activeElement !== this.toggleOf('map')) this.toggleOf('map')?.focus();
    this.markChips();
    if (view.reset !== this.resets) {
      this.resets = view.reset;
      this.confirming = null;
    }
    const boardFor = view.question?.answerable ? keyOf([view.question.id, view.question.version]) : null;
    if (boardFor !== this.boardFor) {
      this.boardFor = boardFor;
      if (this.confirming === 'drawing-board') {
        this.confirming = null;
        if (this.open === 'menu') this.open = null;
      }
    }
    this.drawMenu(view);
    this.showOpen();
    const title = tabTitle(view.title, view.review);
    document.title = unread > 0 ? `(${unread}) ${title}` : title;
  }

  /** @param {PageView} view */
  drawMenu(view) {
    const round = view.reset;
    const chat = view.conversation !== null;
    const notify = this.notifier.state();
    const asked = view.question?.answerable ? view.question : null;
    const pastRounds = view.past_rounds ?? [];
    const shownPast = view.past?.round ?? null;
    const confirmation = () => {
      if (round !== null && this.confirming === 'reset') return resetConfirmation(round);
      if (asked !== null && this.confirming === 'drawing-board') return drawingBoardConfirmation(asked);
      return null;
    };
    const key = [view.review, round, view.rail.length > 0, this.confirming, chat, notify, this.boardFor, pastRounds.length > 0, shownPast];
    this.menu.show(keyOf(key), () =>
      h(
        'div',
        { class: 'menu' },
        h(
          'button',
          {
            class: 'menu-toggle',
            type: 'button',
            'aria-label': 'Round menu',
            'aria-expanded': 'false',
            'aria-controls': 'round-menu',
            onclick: () => this.toggle('menu'),
          },
          '⋯',
        ),
        h(
          'div',
          { class: 'menu-popover', id: 'round-menu', hidden: true },
          view.review ? h('p', { class: 'menu-review' }, reviewParts(view.review), view.review.issues.length > 0 ? [' ', issueLinks(view.review)] : null) : null,
          confirmation() ?? [
                shownPast !== null ? backLink('menu-item') : null,
                copyLink(view.rail.length > 0 ? "Copy the round's link" : "Copy the page's link"),
                chat
                  ? h(
                      'button',
                      {
                        class: 'menu-item',
                        type: 'button',
                        onclick: () => {
                          this.close(false);
                          requestChat();
                        },
                      },
                      "Open the agent's conversation",
                    )
                  : null,
                notifyItem(notify, () => this.toggleNotify()),
                pastRounds.length > 0
                  ? h(
                      'button',
                      {
                        class: 'menu-item',
                        type: 'button',
                        onclick: () => {
                          this.close(true);
                          requestPastList(() => this.toggleOf('menu'));
                        },
                      },
                      'Past rounds',
                      h('span', { class: 'menu-note' }, 'read only'),
                    )
                  : null,
                round !== null || asked !== null ? h('hr', {}) : null,
                asked !== null
                  ? h(
                      'button',
                      { class: 'menu-item', type: 'button', onclick: () => this.confirm('drawing-board') },
                      'Back to the drawing board…',
                      h('span', { class: 'menu-note' }, 'ends the questions'),
                    )
                  : null,
                round !== null
                  ? h(
                      'button',
                      { class: 'menu-item', type: 'button', onclick: () => this.confirm('reset') },
                      'Reset this round…',
                      h('span', { class: 'menu-note' }, 'closes it for good'),
                    )
                  : null,
              ],
        ),
      ),
    );
  }

  /**
   * @param {string} step
   * @param {boolean} on
   */
  target(step, on) {
    if (on) this.aimed = step;
    else if (this.aimed === step) this.aimed = null;
    this.markChips();
  }

  /** @param {(string | undefined)[]} steps */
  neighbours(steps) {
    this.near = new Set(steps);
    this.markChips();
  }

  markChips() {
    for (const step of this.header.querySelectorAll('.rail .step[data-screen]')) {
      if (!(step instanceof HTMLElement)) continue;
      step.classList.toggle('near', this.near.has(step.dataset.screen));
      step.classList.toggle('target', step.dataset.screen === this.aimed);
    }
    this.fit();
  }

  fit() {
    const identity = this.header.querySelector('.identity');
    const product = this.header.querySelector('.product');
    if (!(identity instanceof HTMLElement) || !(product instanceof HTMLElement)) return;
    this.header.classList.remove('crowded');
    this.header.classList.toggle('crowded', identity.getBoundingClientRect().width < product.scrollWidth);
  }

  placeChat() {
    const focused = document.activeElement;
    if (DESK.matches) this.header.prepend(this.chatPlace);
    else this.rail.anchor.after(this.chatPlace);
    if (focused instanceof HTMLElement && this.chatPlace.contains(focused)) focused.focus();
  }

  async toggleNotify() {
    await this.notifier.toggle();
    const entry = this.header.querySelector('#round-menu [data-focus-key="notify"]');
    if (entry instanceof HTMLElement) entry.focus({ preventScroll: true });
  }

  /** @param {'reset' | 'drawing-board'} what */
  confirm(what) {
    this.confirming = what;
    this.redraw();
    const confirm = this.header.querySelector('#round-menu button[type="submit"]');
    if (confirm instanceof HTMLElement) confirm.focus();
  }

  /** @param {Open} what */
  toggle(what) {
    if (this.open === what) {
      this.close(false);
      return;
    }
    this.open = what;
    // The browser's notification permission may have changed since the menu last opened.
    if (what === 'menu') this.redraw();
    else this.showOpen();
  }

  /** @param {boolean} refocus */
  close(refocus) {
    const was = this.open;
    this.open = null;
    if (this.confirming) {
      this.confirming = null;
      this.redraw();
    }
    this.showOpen();
    if (refocus && was) this.toggleOf(was)?.focus();
  }

  /** @param {'map' | 'menu'} what */
  toggleOf(what) {
    const button = this.header.querySelector(what === 'map' ? '.design-toggle' : '.menu-toggle');
    return button instanceof HTMLElement ? button : null;
  }

  showOpen() {
    for (const [what, popover] of /** @type {const} */ ([
      ['map', '#design-map'],
      ['menu', '#round-menu'],
    ])) {
      const open = this.open === what;
      this.toggleOf(what)?.setAttribute('aria-expanded', String(open));
      const element = this.header.querySelector(popover);
      if (element instanceof HTMLElement) element.hidden = !open;
    }
  }

  /** @param {MouseEvent} event */
  clickOutside(event) {
    if (!this.open) return;
    const container = this.open === 'map' ? 'design-step' : 'menu';
    const inside = event.composedPath().some((node) => node instanceof Element && node.classList.contains(container));
    if (!inside) this.close(false);
  }
}

/**
 * @param {ReviewName | null} review
 * @param {boolean} cover
 */
function reviewLine(review, cover) {
  if (!review) return null;
  const line = h('p', { class: 'review' }, cover ? review.repository : reviewParts(review));
  return review.issues.length > 0 ? [line, h('p', { class: 'issues' }, issueLinks(review))] : line;
}

/** @param {ReviewName} review */
function reviewParts(review) {
  return [revisionCode(review.revision), review.title ? [' ', h('span', { class: 'review-title' }, review.title)] : null];
}

/** @param {ReviewName} review */
function issueLinks(review) {
  return review.issues.map((issue) => issueLink(issue.url, issue.id));
}

/**
 * @param {RailStep[]} rail
 * @param {DesignView | null} design
 * @param {Viewing} viewing
 * @param {boolean} quizLink
 * @param {() => void} toggleMap
 * @param {() => void} closeMap
 */
function railNav(rail, design, viewing, quizLink, toggleMap, closeMap) {
  return h(
    'nav',
    { class: 'rail', 'aria-label': 'Round' },
    h(
      'ol',
      {},
      rail.map((step) => {
        const isDesign = step.step.kind === 'design';
        const viewed = isViewed(step.step, viewing);
        const state = viewed ? 'current' : step.state.kind;
        const shown = viewing && !viewed && state === 'current' ? 'next' : state;
        const done = step.state.kind === 'done' && !(isDesign && viewed);
        const name = [
          done ? h('span', { class: 'check', 'aria-hidden': 'true' }, '✓') : null,
          done ? ' ' : null,
          stepName(step.step),
          step.state.kind === 'current' && step.state.working ? ' · working' : null,
        ];
        const current = step.state.kind === 'current' ? 'step' : viewed ? 'page' : null;
        const link = isDesign && design ? null : stepLink(step, viewing, quizLink, name);
        if (link && current) link.setAttribute('aria-current', current);
        return h(
          'li',
          {
            class: `step ${shown}${isDesign ? ' design-step' : ''}`,
            'aria-current': link ? null : current,
            'data-screen': screenOf(step, quizLink),
          },
          isDesign && design
            ? [
                h(
                  'button',
                  {
                    class: 'design-toggle',
                    type: 'button',
                    'aria-expanded': 'false',
                    'aria-controls': 'design-map',
                    onclick: toggleMap,
                  },
                  name,
                  h('span', { class: 'caret', 'aria-hidden': 'true' }, '▾'),
                ),
                designMap(design, closeMap),
              ]
            : (link ?? name),
        );
      }),
    ),
  );
}

/**
 * @param {RailStep} step
 * @param {Viewing} viewing
 * @param {boolean} quizLink
 * @param {(Node | string | null)[]} name
 */
function stepLink(step, viewing, quizLink, name) {
  if (step.step.kind === 'question' && step.state.kind === 'done' && !isViewed(step.step, viewing)) {
    return h('a', { class: 'step-link', href: earlierQuestion(step.step.number) }, name);
  }
  if (step.step.kind === 'quiz' && quizLink && !isViewed(step.step, viewing)) {
    return h('a', { class: 'step-link', href: quizItem(1) }, name);
  }
  if (step.state.kind === 'current' && viewing) return h('a', { class: 'step-link', href: STAGE }, name);
  return null;
}

/**
 * @param {Step} step
 * @param {Viewing} viewing
 */
function isViewed(step, viewing) {
  if (!viewing) return false;
  if (viewing.kind === 'design') return step.kind === 'design';
  if (viewing.kind === 'quiz') return step.kind === 'quiz';
  return step.kind === 'question' && step.number === viewing.number;
}

/**
 * @param {RailStep} step
 * @param {boolean} quizLink
 */
function screenOf(step, quizLink) {
  if (step.state.kind === 'current') return 'round';
  if (step.step.kind === 'design') return 'design';
  if (step.step.kind === 'question' && step.state.kind === 'done') return `question-${step.step.number}`;
  if (step.step.kind === 'quiz' && quizLink) return 'quiz';
  return null;
}

/** @param {Step} step */
function stepName(step) {
  switch (step.kind) {
    case 'design':
      return 'Design';
    case 'question':
      return `Q${step.number}`;
    case 'quiz': {
      const stage = step.stage;
      if (stage.kind === 'running') return `Quiz ${stage.item}/${stage.items}`;
      if (stage.kind === 'scored') return `Quiz ${stage.correct}/${stage.items}`;
      return 'Quiz';
    }
    case 'conclusion':
      return 'Conclusion';
  }
}

/**
 * @param {DesignView} design
 * @param {() => void} close
 */
function designMap(design, close) {
  const follow = () => close();
  return h(
    'div',
    { class: 'design-map', id: 'design-map', role: 'group', 'aria-labelledby': 'design-map-title', hidden: true },
    h('p', { class: 'eyebrow', id: 'design-map-title' }, 'Design of the change'),
    markdown(design.thesis_html, 'map-thesis'),
    h(
      'ol',
      { class: 'map-parts' },
      design.parts.map((part, index) =>
        h(
          'li',
          {},
          h(
            'a',
            { href: `#design-part-${index + 1}`, onclick: follow },
            h('span', { class: 'disc', 'aria-hidden': 'true' }, String(index + 1)),
            h('span', { class: 'part-name' }, part.title),
            markdown(part.thesis_html, 'part-gist'),
          ),
        ),
      ),
    ),
    h('a', { class: 'map-open', href: '#design', onclick: follow }, 'Open the design →'),
  );
}

/**
 * @param {NotifyState} state
 * @param {() => void} toggle
 */
function notifyItem(state, toggle) {
  const unavailable = state === 'unavailable' || state === 'blocked';
  return h(
    'button',
    {
      class: 'menu-item',
      type: 'button',
      'aria-pressed': state === 'on' ? 'true' : 'false',
      'aria-disabled': unavailable ? 'true' : false,
      'data-focus-key': 'notify',
      onclick: toggle,
    },
    'Notify me when the agent finishes',
    h('span', { class: 'menu-note' }, NOTIFY_NOTES[state]),
  );
}

/** @type {Record<NotifyState, string>} */
const NOTIFY_NOTES = {
  on: 'on',
  off: 'off',
  blocked: "blocked in the browser's settings",
  unavailable: 'needs localhost or the tunnel',
};

/** @param {string} label */
function copyLink(label) {
  const button = h('button', { class: 'menu-item', type: 'button' }, label);
  const told = h('span', { class: 'sr-only', 'aria-live': 'polite' });
  button.addEventListener('click', async () => {
    const copied = await copy(location.href);
    // The browser's older copy command takes the focus away to a text box of its own.
    button.focus();
    if (!copied) {
      // The browser may refuse the clipboard.
      const field = h('input', { class: 'menu-link', type: 'text', readonly: true, value: location.href, 'aria-label': 'Link' });
      button.replaceWith(field);
      field.select();
      return;
    }
    button.textContent = 'Link copied';
    told.textContent = 'Link copied';
    setTimeout(() => {
      button.textContent = label;
      told.textContent = '';
    }, 2000);
  });
  return [button, told];
}

/** @param {string} text */
async function copy(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const field = h('textarea', { class: 'sr-only', readonly: true });
    field.value = text;
    document.body.append(field);
    field.select();
    let copied = false;
    try {
      copied = document.execCommand('copy');
    } catch {
      copied = false;
    }
    field.remove();
    return copied;
  }
}

/**
 * @param {string} round
 * @param {{ hint: string, label: string }} [words]
 */
export function resetConfirmation(round, { hint, label } = { hint: RESET_HINT, label: 'Confirm reset' }) {
  return h(
    'form',
    { class: 'reset', 'data-method': 'reset' },
    h('input', { type: 'hidden', name: 'round', value: round }),
    h('p', { class: 'hint' }, hint),
    h('button', { class: 'button danger', type: 'submit' }, label),
  );
}

/**
 * @param {TabTitle | null} title
 * @param {ReviewName | null} review
 */
function tabTitle(title, review) {
  const name = review ? review.title || revisionText(review.revision) : null;
  if (!title) return name ? `Explore — ${name}` : 'Explore';
  return `${turn(title)} — ${name ?? 'Explore'}`;
}

/** @param {TabTitle} title */
function turn(title) {
  switch (title.kind) {
    case 'your_turn':
      return `Q${title.question} · your turn`;
    case 'agent_working':
      return 'Agent working…';
    case 'retry_needed':
      return 'Retry needed';
    case 'conclusion':
      return 'Conclusion';
  }
}
