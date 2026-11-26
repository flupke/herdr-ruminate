/** @import { ChangeReferences, PageView, PastChange, PastRoundEntry, PastView, ReviewName } from "./types.ts" */

import { BESIDE } from './desk.js';
import { h, keyOf, Region } from './dom.js';
import { issueLink } from './issue-link.js';
import { LeftColumn } from './left-column.js';
import { revisionCode } from './revision.js';
import { PAST_ROUNDS, pastAddress, STAGE } from './route.js';

const DATE = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });
const DAY = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' });
const TIME = new Intl.DateTimeFormat(undefined, { timeStyle: 'short' });

const OPEN = 'explore:past-rounds';

/** @param {() => Element | null} [returnTo] */
export function requestPastList(returnTo) {
  document.dispatchEvent(new CustomEvent(OPEN, { detail: { returnTo } }));
}

/** @param {number} ms */
export function savedAt(ms) {
  return DATE.format(new Date(ms));
}

/** @param {string} kind */
export function backLink(kind) {
  return h('a', { class: kind, href: STAGE, 'data-leave-past': '' }, 'Back to the current round');
}

/**
 * @param {PastView} past
 * @param {ReviewName | null} review
 */
export function pastBanner(past, review) {
  return h(
    'section',
    { class: 'past-banner', 'aria-label': 'Past round' },
    h(
      'p',
      { class: 'past-what' },
      h('span', { class: 'eyebrow' }, 'Past round · read only'),
      ' ',
      h('span', { class: 'past-title' }, review?.title),
      ' ',
      referenceLinks(past.references),
      ' ',
      h('time', { class: 'past-date', datetime: new Date(past.saved_at_ms).toISOString() }, savedAt(past.saved_at_ms)),
    ),
    h(
      'p',
      { class: 'past-current' },
      past.current_waits ? h('span', { class: 'past-waits' }, 'The current round waits for you.') : null,
      ' ',
      backLink('past-back'),
    ),
  );
}

export class PastList {
  /** @param {() => void} redraw */
  constructor(redraw) {
    this.column = new LeftColumn({
      tag: 'nav',
      kind: 'past-list',
      label: 'Past rounds',
      title: 'Past rounds',
      closeLabel: 'Close the past rounds',
      closed: (refocus) => this.closed(refocus),
    });
    this.body = h('div', { class: 'past-body', tabindex: '-1' });
    this.column.element.append(this.body);
    this.rounds = new Region(this.body, 'rounds');
    /** @type {(() => Element | null) | null} */
    this.opener = null;
    this.asked = location.hash === PAST_ROUNDS;
    addEventListener('hashchange', () => {
      if (location.hash === PAST_ROUNDS) {
        this.asked = true;
        redraw();
      } else if (!BESIDE.matches) this.column.close(false);
    });
    document.addEventListener(OPEN, (event) => {
      const detail = /** @type {CustomEvent<{ returnTo?: () => Element | null }>} */ (event).detail;
      this.show(detail.returnTo);
    });
    this.column.element.addEventListener('click', (event) => {
      if (BESIDE.matches || !(event.target instanceof Element)) return;
      if (event.target.closest('.past-round[aria-current="page"]')) this.column.close(false);
    });
  }

  /** @param {PageView} view */
  update(view) {
    const changes = view.past_rounds;
    if (changes === null) {
      this.column.close(false);
      this.rounds.clear();
      if (this.asked) this.leaveAsked();
      return;
    }
    const count = changes.reduce((sum, change) => sum + change.rounds.length, 0);
    this.column.meta.textContent = count === 0 ? '' : `${count} ${count === 1 ? 'round' : 'rounds'}`;
    const shown = view.past?.round ?? null;
    this.rounds.show(keyOf([changes, shown]), () => pastChanges(changes, shown));
    if (this.asked) {
      this.leaveAsked();
      this.show();
    }
  }

  leaveAsked() {
    this.asked = false;
    history.replaceState(history.state, '', STAGE);
  }

  /** @param {() => Element | null} [returnTo] */
  show(returnTo) {
    const focused = document.activeElement;
    if (this.column.show()) this.opener = returnTo ?? (() => focused);
    const element = this.column.element;
    const marked = element.querySelector('.past-round[aria-current="page"]');
    if (marked instanceof HTMLElement) {
      marked.focus({ preventScroll: true });
      marked.closest('.past-change')?.scrollIntoView({ block: 'nearest' });
      marked.scrollIntoView({ block: 'nearest' });
      return;
    }
    this.body.focus({ preventScroll: true });
    this.body.scrollTo(0, 0);
  }

  /** @param {boolean} refocus */
  closed(refocus) {
    const opener = this.opener?.() ?? null;
    if (refocus && opener instanceof HTMLElement && opener.isConnected) opener.focus();
    this.opener = null;
  }
}

/** @param {ChangeReferences} references */
function referenceLinks(references) {
  const pullRequest = references.pull_request;
  if (!pullRequest && references.tickets.length === 0) return null;
  return h(
    'span',
    { class: 'past-references' },
    pullRequest ? issueLink(pullRequest.url, `PR #${pullRequest.number}`, `Pull request ${pullRequest.number}`) : null,
    references.tickets.map((ticket) => issueLink(ticket.url, ticket.id, `Ticket ${ticket.id}`)),
  );
}

/**
 * @param {PastChange[]} changes
 * @param {string | null} shown
 */
function pastChanges(changes, shown) {
  if (changes.length === 0) return h('p', { class: 'past-none' }, 'No past round yet.');
  return changes.map((change) =>
    h(
      'section',
      { class: 'past-change', 'aria-label': change.title },
      h(
        'div',
        { class: 'past-change-head' },
        h('h3', { class: 'past-change-title', title: change.title }, change.title),
        revisionCode(change.revision),
        referenceLinks(change.references),
      ),
      h(
        'ul',
        { class: 'past-rounds' },
        change.rounds.map((round, index) => pastRound(round, shown, change.rounds[index - 1])),
      ),
    ),
  );
}

/**
 * @param {PastRoundEntry} round
 * @param {string | null} shown
 * @param {PastRoundEntry | undefined} above
 */
function pastRound(round, shown, above) {
  const day = DAY.format(new Date(round.saved_at_ms));
  const sameDay = above !== undefined && DAY.format(new Date(above.saved_at_ms)) === day;
  return h(
    'li',
    {},
    h(
      'a',
      {
        class: 'past-round',
        href: pastAddress(round.id),
        'aria-current': round.id === shown ? 'page' : null,
        'aria-label': savedAt(round.saved_at_ms),
        'data-focus-key': `past:${round.id}`,
      },
      h('span', { class: sameDay ? 'past-day same' : 'past-day', 'aria-hidden': 'true' }, day),
      h(
        'time',
        { class: 'past-time', datetime: new Date(round.saved_at_ms).toISOString(), 'aria-hidden': 'true' },
        TIME.format(new Date(round.saved_at_ms)),
      ),
    ),
  );
}
