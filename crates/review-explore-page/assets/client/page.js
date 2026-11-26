/** @import { AskedUnder, Call, PageView, Reply, StatusCard } from "./types.ts" */
/** @import { Current } from "./design.js" */

import { Chat } from './chat.js';
import { QuotePicker } from './chat-quote.js';
import { ConclusionScreen } from './conclusion.js';
import { DesignScreen } from './design.js';
import { Glossary } from './glossary.js';
import { earlierScreen } from './earlier.js';
import { drawDiagrams, fitDiagrams } from './diagrams.js';
import { Favicon } from './favicon.js';
import { h, keyOf, Region } from './dom.js';
import { Masthead } from './masthead.js';
import { Meter } from './meter.js';
import { Notifier } from './notify.js';
import { QuestionScreen } from './question.js';
import { itemOpens, quizOpens, QuizScreen, railShowing } from './quiz.js';
import { pastBanner, PastList } from './past.js';
import { DESIGN, earlierQuestion, here, openRound, pastRound, quizItem, route, STAGE } from './route.js';
import { SentScreen } from './sent.js';
import { startCover } from './start.js';
import { statusCard } from './status.js';
import { Swipe } from './swipe.js';
import { turnStrip } from './turn.js';

export class Page {
  /**
   * @param {HTMLElement} main
   * @param {HTMLElement} header
   * @param {(call: Call) => Promise<Reply>} call
   */
  constructor(main, header, call) {
    this.main = main;
    const redraw = () => {
      if (this.view) this.render(this.view);
    };
    this.notifier = new Notifier(redraw);
    this.masthead = new Masthead(header, redraw, this.notifier);
    this.meter = new Meter(/** @type {HTMLElement} */ (header.querySelector('#masthead-line')));
    this.favicon = new Favicon();
    this.chat = new Chat(/** @type {HTMLElement} */ (header.querySelector('#masthead-chat')), call, () => {
      if (this.view) this.render(this.view);
    });
    this.pastList = new PastList(redraw);
    this.quotes = new QuotePicker(main);
    this.glossary = new Glossary([main, this.chat.element]);
    this.designScreen = h('div', { class: 'screen', hidden: true });
    this.stage = h('div', { class: 'screen' });
    this.quizScreen = h('div', { class: 'screen', hidden: true });
    main.append(this.designScreen, this.stage, this.quizScreen);
    this.past = new Region(main, 'past');
    main.prepend(this.past.anchor);
    /** @type {Map<number, { screen: HTMLElement, region: Region }>} */
    this.earlierScreens = new Map();
    this.design = new Region(this.designScreen, 'design');
    this.cards = new Region(this.stage, 'cards');
    this.start = new Region(this.stage, 'start');
    this.turn = new Region(this.stage, 'turn');
    this.sent = new Region(this.stage, 'sent');
    this.question = new Region(this.stage, 'question');
    this.conclusion = new Region(this.stage, 'conclusion');
    this.answeredQuiz = new Region(this.quizScreen, 'answered-quiz');
    this.answeredItem = 0;
    /** @type {string | null} */
    this.shown = null;
    /** @type {string | null} */
    this.shownPast = null;
    /** @type {import('./masthead.js').Viewing} */
    this.viewed = null;
    this.stageScroll = 0;
    /** @type {number | null} */
    this.swipedTo = null;
    /** @type {DesignScreen | null} */
    this.shownDesign = null;
    addEventListener('hashchange', () => {
      if (this.view) this.render(this.view);
    });
    this.swipe = new Swipe(main, {
      track: () => this.track(),
      settled: (address, top) => {
        if (address !== null) {
          this.swipedTo = top;
          location.hash = here(address);
        }
        if (this.view) this.render(this.view);
      },
      chip: (step, on) => this.masthead.target(step, on),
    });
    /** @type {PageView | null} */
    this.view = null;
    /** @type {StatusCard | null} */
    this.notice = null;
    /** @type {import('./quiz.js').QuizAt | null} */
    this.quizAt = null;
  }

  /** @param {PageView} view */
  render(view) {
    this.view = view;
    this.glossary.set(view.glossary);
    const cards = this.notice ? [this.notice, ...view.cards] : view.cards;
    this.past.show(keyOf([view.past, view.review]), () => (view.past ? pastBanner(view.past, view.review) : null));
    this.cards.show(keyOf(cards), () => cards.map(statusCard));
    const start = view.start;
    const tally = view.tally;
    this.start.show(keyOf([start, view.review, tally?.change.changed, tally?.files.length]), () =>
      start ? startCover(start, view.review, tally) : null,
    );
    this.renderQuestion(view);
    const quizShown = this.renderConclusion(view);
    const turn = turnOf(view);
    this.turn.show(view.question || view.conclusion || view.sent ? null : keyOf(turn), () => turnStrip(turn));
    this.renderSent(view);
    this.renderScreens(view, quizShown !== null);
    this.chat.update(view, chatPlace(view, this.viewed));
    this.pastList.update(view);
    this.quotes.update(view.conversation?.writable ?? false);
    const quiz = view.conclusion?.quiz;
    const rail = quiz && quizShown !== null ? railShowing(view.rail, quizShown, quiz.items.length) : view.rail;
    this.masthead.update(rail === view.rail ? view : { ...view, rail }, this.viewed, this.chat.unread(), this.quizLink());
    const { screens, shown } = this.track();
    this.masthead.neighbours([screens[shown - 1]?.step, screens[shown + 1]?.step]);
    this.meter.update(view);
    this.favicon.update(view);
    this.notifier.update(view);
    drawDiagrams(this.main);
  }

  /**
   * @param {PageView} view
   * @param {boolean} asking
   */
  renderScreens(view, asking) {
    const current = currentStep(view);
    openRound(view, current?.kind === 'question' && !current.working ? current.number : null);
    const design = view.design;
    if (design) {
      const screen = this.design.component('design', () => new DesignScreen());
      if (screen !== this.shownDesign) this.shownDesign?.stop();
      this.shownDesign = screen;
      screen.update(design, current, view.question, view.glossary);
    } else {
      this.shownDesign?.stop();
      this.shownDesign = null;
      this.design.clear();
    }
    this.renderEarlier(view, current);
    const answered = this.renderAnsweredQuiz(view, asking);
    if (this.swipe.active) return;
    const asked = route();
    const showsDesign = asked.screen === 'design' && design !== null;
    const earlier = asked.screen === 'question' ? this.earlierScreens.get(asked.number) : undefined;
    if ((asked.screen === 'question' && !earlier) || (asked.screen === 'quiz' && !answered)) {
      history.replaceState(history.state, '', here(STAGE));
    }
    const showsQuiz = asked.screen === 'quiz' && answered;
    this.designScreen.hidden = !showsDesign;
    for (const { screen } of this.earlierScreens.values()) screen.hidden = screen !== earlier?.screen;
    this.quizScreen.hidden = !showsQuiz;
    this.stage.hidden = showsDesign || earlier !== undefined || showsQuiz;
    const shown = showsDesign
      ? `design:${asked.screen === 'design' ? asked.part : ''}`
      : earlier && asked.screen === 'question'
        ? `question:${asked.number}`
        : showsQuiz
          ? 'quiz'
          : 'stage';
    const past = view.past?.round ?? null;
    if (shown === this.shown && past === this.shownPast) return;
    if (past !== this.shownPast) this.stageScroll = 0;
    else if (this.shown === 'stage') this.stageScroll = window.scrollY;
    this.shown = shown;
    this.shownPast = past;
    this.viewed = showsDesign
      ? { kind: 'design' }
      : earlier && asked.screen === 'question'
        ? { kind: 'question', number: asked.number }
        : showsQuiz
          ? { kind: 'quiz' }
          : null;
    fitDiagrams();
    const swiped = this.swipedTo;
    this.swipedTo = null;
    if (swiped !== null) {
      const screen = showsDesign ? this.designScreen : showsQuiz ? this.quizScreen : (earlier?.screen ?? this.stage);
      window.scrollTo(0, window.scrollY + screen.getBoundingClientRect().top - swiped);
    } else if (showsDesign) this.shownDesign?.scrollTo(asked.screen === 'design' ? asked.part : null);
    else if (earlier || showsQuiz) window.scrollTo(0, 0);
    else scrollToFragment(this.stageScroll);
  }

  /**
   * @param {PageView} view
   * @param {Current | null} current
   */
  renderEarlier(view, current) {
    const numbers = new Set(view.earlier_questions.map((question) => question.number));
    for (const [number, { screen }] of this.earlierScreens) {
      if (numbers.has(number)) continue;
      screen.remove();
      this.earlierScreens.delete(number);
    }
    for (const question of view.earlier_questions) {
      let earlier = this.earlierScreens.get(question.number);
      if (!earlier) {
        const screen = h('div', { class: 'screen', hidden: true });
        this.main.append(screen);
        earlier = { screen, region: new Region(screen, `question-${question.number}`) };
        this.earlierScreens.set(question.number, earlier);
      }
      earlier.region.show(keyOf([question, current]), () => earlierScreen(question, current));
    }
  }

  /**
   * @param {PageView} view
   * @param {boolean} asking
   * @returns {boolean}
   */
  renderAnsweredQuiz(view, asking) {
    const conclusion = view.conclusion;
    const quiz = conclusion?.quiz;
    if (!conclusion || !quiz || quiz.items.length === 0 || asking) {
      this.answeredQuiz.clear();
      this.answeredItem = 0;
      return false;
    }
    const asked = route();
    if (asked.screen === 'quiz') {
      this.answeredItem = Math.min(asked.item, quiz.items.length) - 1;
      if (asked.item > quiz.items.length) history.replaceState(history.state, '', here(quizItem(this.answeredItem + 1)));
    }
    this.answeredItem = Math.min(this.answeredItem, quiz.items.length - 1);
    this.answeredQuiz
      .component(`quiz:${conclusion.request}`, () => new QuizScreen())
      .update(quiz, conclusion.request, turnOf(view), {
        shown: this.answeredItem,
        asking: false,
        open: (item) => {
          location.hash = here(quizItem(item + 1));
        },
        done: () => {},
      });
    return true;
  }

  quizLink() {
    const step = this.view?.rail.find((each) => each.step.kind === 'quiz');
    return this.answeredQuiz.current !== undefined && step !== undefined && quizOpens(step);
  }

  /** @returns {import('./swipe.js').Track} */
  track() {
    /** @type {import('./swipe.js').Screen[]} */
    const screens = [];
    if (this.view?.design) screens.push({ address: DESIGN, element: this.designScreen, step: 'design' });
    const numbers = [...this.earlierScreens.keys()].sort((a, b) => a - b);
    for (const number of numbers) {
      const { screen } = /** @type {{ screen: HTMLElement }} */ (this.earlierScreens.get(number));
      screens.push({ address: earlierQuestion(number), element: screen, step: `question-${number}` });
    }
    const viewed = this.viewed;
    if (this.quizLink() || (this.answeredQuiz.current && viewed?.kind === 'quiz')) {
      screens.push({ address: quizItem(this.answeredItem + 1), element: this.quizScreen, step: 'quiz' });
    }
    screens.push({ address: STAGE, element: this.stage, step: 'round' });
    const step =
      viewed === null ? 'round' : viewed.kind === 'question' ? `question-${viewed.number}` : viewed.kind;
    return { screens, shown: Math.max(screens.findIndex((screen) => screen.step === step), 0) };
  }

  /** @param {PageView} view */
  renderSent(view) {
    const sent = view.sent;
    if (!sent) {
      this.sent.clear();
      return;
    }
    this.sent.component(`sent:${view.reset ?? ''}`, () => new SentScreen()).update(sent, view.cancellable);
  }

  /** @param {PageView} view */
  renderQuestion(view) {
    const question = view.question;
    if (!question) {
      this.question.clear();
      return;
    }
    const key = keyOf([question.round, question.id, question.number]);
    this.question.component(key, () => new QuestionScreen(question)).update(question, turnOf(view), view.past !== null);
  }

  /**
   * @param {PageView} view
   * @returns {number | null}
   */
  renderConclusion(view) {
    const conclusion = view.conclusion;
    if (!conclusion) {
      this.conclusion.clear();
      return null;
    }
    const quiz = conclusion.quiz;
    const request = conclusion.request;
    const at = this.quizAt;
    const moved = quiz && at?.conclusion === request && itemOpens(quiz, at.item, true) ? at.item : null;
    const shown = quiz ? (moved ?? quiz.next) : null;
    if (quiz && shown !== null) {
      /** @param {import('./quiz.js').QuizAt | null} where */
      const go = (where) => {
        this.quizAt = where;
        this.render(/** @type {PageView} */ (this.view));
      };
      this.conclusion
        .component(`quiz:${request}`, () => new QuizScreen())
        .update(quiz, request, turnOf(view), {
          shown,
          asking: true,
          open: (item) => go({ conclusion: request, item }),
          done: () => go(null),
        });
      return shown;
    }
    this.conclusion
      .component(`conclusion:${conclusion.request}`, () => new ConclusionScreen())
      .update(conclusion, view.reset, turnOf(view));
    return null;
  }

  /** @param {StatusCard | null} notice */
  showNotice(notice) {
    this.notice = notice;
    if (this.view) this.render(this.view);
  }
}

/**
 * @param {PageView} view
 * @param {import('./masthead.js').Viewing} viewed
 * @returns {import('./chat.js').Place}
 */
function chatPlace(view, viewed) {
  if (viewed?.kind === 'design') return { askedUnder: { stage: 'design' }, question: false };
  if (viewed?.kind === 'question') {
    const earlier = view.earlier_questions.find((question) => question.number === viewed.number);
    const askedUnder = earlier ? { stage: /** @type {const} */ ('question'), question: earlier.id, version: earlier.version } : null;
    return { askedUnder, question: false };
  }
  /** @type {AskedUnder | null} */
  let askedUnder = null;
  if (view.question) askedUnder = { stage: 'question', question: view.question.id, version: view.question.version };
  else if (view.conclusion) askedUnder = { stage: 'conclusion', conclusion: view.conclusion.request };
  return { askedUnder, question: view.question !== null };
}

/**
 * @param {PageView} view
 * @returns {Current | null}
 */
function currentStep(view) {
  for (const { step, state } of view.rail) {
    if (state.kind !== 'current') continue;
    switch (step.kind) {
      case 'question':
        return { kind: 'question', number: step.number, working: state.working };
      case 'quiz':
        return { kind: 'quiz' };
      case 'conclusion':
        return { kind: 'conclusion' };
      default:
        return null;
    }
  }
  return null;
}

/** @param {number} top */
function scrollToFragment(top) {
  const named = location.hash.length > 1 && location.hash !== STAGE && pastRound() === null;
  const target = named ? document.getElementById(location.hash.slice(1)) : null;
  if (target && !target.closest('[hidden]')) target.scrollIntoView();
  else window.scrollTo(0, top);
}

/**
 * @param {PageView} view
 * @returns {import('./turn.js').Turn}
 */
function turnOf(view) {
  const kept = view.cancellable
    ? null
    : (view.earlier_questions.find((question) => question.number === view.answered)?.answer ?? null);
  return { answer: view.cancellable, kept, response: view.response, number: view.answered };
}
