/** @import { StatusCard, StatusAction, CardTime } from "./types.ts" */

import { h } from './dom.js';

/**
 * @param {StatusCard} card
 * @returns {HTMLElement}
 */
export function statusCard(card) {
  return drawn(card, true);
}

/**
 * @param {StatusCard} card
 * @returns {HTMLElement}
 */
export function panelCard(card) {
  return drawn(card, false);
}

/**
 * @param {StatusCard} card
 * @param {boolean} actions
 * @returns {HTMLElement}
 */
function drawn(card, actions) {
  const title = `${card.id}-title`;
  const next =
    card.imperative || card.next
      ? h(
          'p',
          { class: 'status-next' },
          card.imperative ? h('strong', {}, card.imperative) : null,
          card.imperative && card.next ? ` ${card.next}` : card.next,
        )
      : null;
  return h(
    'div',
    { class: `status-card ${card.kind}`, id: card.id, role: card.role, 'aria-labelledby': title },
    h('span', { class: 'status-glyph', 'aria-hidden': 'true' }),
    h('p', { class: 'status-title', id: title }, card.title),
    card.reason || card.time || card.since
      ? h(
          'p',
          { class: 'status-reason' },
          card.time ? `${time(card.time)}${card.reason ? ' ' : ''}` : null,
          card.reason && card.code ? h('code', {}, card.reason) : card.reason,
          card.since ? [card.reason ? ' ' : null, since(card.since)] : null,
        )
      : null,
    card.kind === 'progress' ? h('div', { class: 'status-bar', 'aria-hidden': 'true' }) : null,
    next,
    actions && card.actions.length > 0
      ? h('div', { class: 'status-actions' }, card.actions.map((one) => action(one, '')))
      : null,
  );
}

/**
 * @param {StatusCard} card
 * @returns {HTMLElement[]}
 */
export function panelActions(card) {
  return card.actions.map((one) => action(one, ' block'));
}

/**
 * @param {StatusAction} action
 * @param {string} width
 */
function action(action, width) {
  return h(
    'form',
    { class: action.method, 'data-method': action.method },
    action.fields.map((field) => h('input', { type: 'hidden', name: field.name, value: field.value })),
    h('button', { class: `button ${action.tier}${width}`, type: 'submit' }, action.label),
    action.hint ? h('p', { class: 'hint' }, action.hint) : null,
  );
}

/** @param {number} ms */
export function clockTime(ms) {
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });
}

/** @param {CardTime} at */
function time(at) {
  const date = new Date(at.ms);
  const clock = clockTime(at.ms);
  const today = new Date().toDateString() === date.toDateString();
  const day = today ? '' : ` on ${date.toLocaleDateString([], { day: 'numeric', month: 'short' })}`;
  return `${at.words} ${clock}${day}.`;
}

/** @param {CardTime} at */
function since(at) {
  ticker ??= setInterval(tick, 1000);
  return [
    `${at.words} `,
    h('span', { class: 'status-elapsed', 'data-since': at.ms, 'aria-live': 'off' }, elapsed(at.ms)),
    ' ago',
  ];
}

/** @type {number | null} */
let ticker = null;

function tick() {
  const counts = document.querySelectorAll('.status-elapsed[data-since]');
  if (counts.length === 0 && ticker !== null) {
    clearInterval(ticker);
    ticker = null;
  }
  for (const count of counts) {
    if (count instanceof HTMLElement) count.textContent = elapsed(Number(count.dataset.since));
  }
}

/** @param {number} ms */
function elapsed(ms) {
  const seconds = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  const two = (/** @type {number} */ value) => String(value).padStart(2, '0');
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor(seconds / 60) % 60;
  return hours > 0 ? `${hours}:${two(minutes)}:${two(seconds % 60)}` : `${minutes}:${two(seconds % 60)}`;
}
