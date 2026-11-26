/** @import { PageView } from "./types.ts" */

export const DESIGN = '#design';
export const STAGE = '#round';
export const PAST_ROUNDS = '#past-rounds';

const PART = /^#design-part-([1-9][0-9]*)$/;
const PAST = /@([A-Za-z0-9-]+)$/;
const QUESTION = /^#question-([1-9][0-9]*)$/;
const QUIZ = /^#quiz-([1-9][0-9]*)$/;

/** @param {number} number */
export function designPart(number) {
  return `#design-part-${number}`;
}

/** @param {number} number */
export function earlierQuestion(number) {
  return `#question-${number}`;
}

/** @param {number} number */
export function quizItem(number) {
  return `#quiz-${number}`;
}

/**
 * @typedef {{ screen: 'design', part: number | null } | { screen: 'question', number: number }
 *   | { screen: 'quiz', item: number } | { screen: 'stage' }} Route
 * @returns {Route}
 */
export function route() {
  const hash = location.hash.replace(PAST, '');
  if (hash === DESIGN) return { screen: 'design', part: null };
  const part = PART.exec(hash);
  if (part) return { screen: 'design', part: Number(part[1]) };
  const question = QUESTION.exec(hash);
  if (question) return { screen: 'question', number: Number(question[1]) };
  const quiz = QUIZ.exec(hash);
  return quiz ? { screen: 'quiz', item: Number(quiz[1]) } : { screen: 'stage' };
}

export function pastRound() {
  return PAST.exec(location.hash)?.[1] ?? null;
}

/**
 * @param {string} address
 * @param {string | null} past
 */
function inPast(address, past) {
  return past === null ? address : `${address}@${past}`;
}

/** @param {string} id */
export function pastAddress(id) {
  return inPast(STAGE, id);
}

/** @param {string} address */
export function here(address) {
  return inPast(address, pastRound());
}

export function leavePast() {
  history.replaceState(history.state, '', location.hash.replace(PAST, '') || STAGE);
}

document.addEventListener('click', (event) => {
  const past = pastRound();
  if (past === null || event.defaultPrevented || !(event.target instanceof Element)) return;
  const link = event.target.closest('a[href^="#"]');
  if (!(link instanceof HTMLAnchorElement) || link.dataset.leavePast !== undefined) return;
  const address = link.getAttribute('href') ?? '';
  if (PAST.test(address)) return;
  event.preventDefault();
  location.hash = inPast(address, past);
});

const storage = (() => {
  try {
    return window.sessionStorage;
  } catch {
    return null;
  }
})();
const opened = new Set();
/** @type {string | null} */
let shownBySelf = null;

/**
 * @param {PageView} view
 * @param {number | null} waitingFor
 */
export function openRound(view, waitingFor) {
  const round = view.question?.round ?? null;
  const opening = view.design !== null && round !== null && waitingFor === 1 && view.past === null;
  if (shownBySelf !== null && !(opening && round === shownBySelf)) {
    if (location.hash === DESIGN) history.replaceState(history.state, '', STAGE);
    shownBySelf = null;
  }
  if (!opening || round === null || opened.has(round)) return;
  opened.add(round);
  const name = `explore-opened:${round}`;
  try {
    if (storage?.getItem(name)) return;
    storage?.setItem(name, '1');
  } catch {
    // The browser may refuse this page its storage.
  }
  if (route().screen !== 'design') history.replaceState(history.state, '', DESIGN);
  shownBySelf = round;
}

addEventListener('hashchange', () => {
  shownBySelf = null;
});
