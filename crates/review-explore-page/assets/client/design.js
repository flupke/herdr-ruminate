/** @import { DesignView, DesignPartView, QuestionView } from "./types.ts" */

import { DOORS, doorChip } from './chips.js';
import { h, keyOf, markdown, Region, setRenderedMarkdown } from './dom.js';
import { drawingBoard } from './drawing-board.js';
import { glossaryList } from './glossary.js';
import { designPart, STAGE } from './route.js';

/**
 * @typedef {{ kind: 'question', number: number, working: boolean } | { kind: 'quiz' }
 *   | { kind: 'conclusion' }} Current
 */

export class DesignScreen {
  constructor() {
    this.element = h('section', { class: 'design-screen desk', 'aria-label': 'Design of the change' });
    this.head = new Region(this.element, 'head');
    this.map = new Region(this.element, 'map');
    this.parts = new Region(this.element, 'parts');
    this.glossary = new Region(this.element, 'glossary');
    this.bar = new Region(this.element, 'bar');
    this.spy = new PartSpy(this.element);
  }

  /**
   * @param {DesignView} design
   * @param {Current | null} current
   * @param {QuestionView | null} question
   * @param {{ [term in string]?: string }} glossary
   */
  update(design, current, question, glossary) {
    const asked = current?.kind === 'question' && question !== null ? question : null;
    const before = current?.kind === 'question' ? current.number : null;
    this.head.show(keyOf([design.thesis_html, design.parts.length, design.minutes, design.changed_files, before]), () =>
      head(design, before),
    );
    const map = this.map.show(
      keyOf([design.parts, current, asked?.text_html, asked?.door, asked?.version, asked?.answerable]),
      () =>
      designMap(design.parts, current, asked),
    );
    const parts = this.parts.show(keyOf(design.parts), () => [
      ...design.parts.map(partSection),
      h('span', { class: 'design-end', 'aria-hidden': 'true' }),
    ]);
    this.glossary.show(keyOf(glossary), () => glossaryList(glossary));
    const door = asked?.door ?? null;
    this.bar.show(keyOf([current, door]), () => (current ? bar(current, door) : null));
    if (map || parts) this.spy.watch();
  }

  /** @param {number | null} number */
  scrollTo(number) {
    const target = number === null ? null : document.getElementById(designPart(number).slice(1));
    if (target) target.scrollIntoView();
    else window.scrollTo(0, 0);
    this.spy.reach(number ?? 1);
  }

  stop() {
    this.spy.stop();
  }
}

/**
 * @param {DesignView} design
 * @param {number | null} before
 */
function head(design, before) {
  const thesis = h('h2', { id: 'design-title' });
  setRenderedMarkdown(thesis, design.thesis_html);
  const minutes = design.minutes === 1 ? 'about 1 minute' : `about ${design.minutes} minutes`;
  const files = design.changed_files === 1 ? '1 file' : `${design.changed_files} files`;
  const meta = [`${design.parts.length} parts`, minutes, ...(design.changed_files > 0 ? [files] : [])];
  return h(
    'header',
    { class: 'design-head' },
    h('p', { class: 'eyebrow' }, before === null ? 'Design of the change' : `Design of the change · read before question ${before}`),
    thesis,
    h('p', { class: 'meta' }, meta.join(' · ')),
  );
}

/**
 * @param {DesignPartView[]} parts
 * @param {Current | null} current
 * @param {QuestionView | null} question
 */
function designMap(parts, current, question) {
  return h(
    'nav',
    { class: 'design-nav panel', 'aria-label': 'Design map' },
    h('p', { class: 'eyebrow' }, 'Design map'),
    h(
      'ol',
      {},
      parts.map((part, index) => {
        const gist = h('span', { class: 'gist', id: `design-gist-${index + 1}`, 'aria-hidden': 'true' });
        setRenderedMarkdown(gist, part.thesis_html);
        return h(
          'li',
          { 'data-part': index + 1 },
          h(
            'a',
            { href: designPart(index + 1), 'aria-describedby': gist.id },
            h('span', { class: 'n', 'aria-hidden': 'true' }, index + 1),
            h('span', { class: 'name' }, part.title),
            gist,
          ),
        );
      }),
    ),
    current ? next(current, question) : null,
  );
}

/**
 * @param {Current} current
 * @param {QuestionView | null} question
 */
function next(current, question) {
  return h(
    'div',
    { class: 'design-next' },
    h('p', { class: 'eyebrow' }, `Then · ${stepName(current)}`, question?.door ? doorChip(question.door) : null),
    question ? markdown(question.text_html, 'next-text') : null,
    question ? h('p', { class: 'hint' }, 'Its choices and evidence open on the next screen.') : null,
    goTo(current),
    question?.answerable ? drawingBoard(question) : null,
  );
}

/**
 * @param {Current} current
 * @param {import('./types.ts').Door | null} door
 */
function bar(current, door) {
  const name = stepName(current);
  const state = [`Current · ${name.charAt(0).toUpperCase()}${name.slice(1)}`];
  if (current.kind === 'question' && current.working) state.push('working');
  if (door) state.push(DOORS[door].name);
  return h('div', { class: 'design-bar' }, h('p', {}, state.join(' · ')), goTo(current));
}

/** @param {Current} current */
export function goTo(current) {
  return h(
    'a',
    { class: 'button primary block', href: STAGE },
    `Go to ${stepName(current)}`,
    h('span', { class: 'arrow', 'aria-hidden': 'true' }, '→'),
  );
}

/** @param {Current} current */
function stepName(current) {
  switch (current.kind) {
    case 'question':
      return `question ${current.number}`;
    case 'quiz':
      return 'the quiz';
    case 'conclusion':
      return 'the conclusion';
  }
}

/**
 * @param {DesignPartView} part
 * @param {number} index
 * @param {DesignPartView[]} parts
 */
function partSection(part, index, parts) {
  const number = index + 1;
  const id = designPart(number).slice(1);
  const thesis = h('p', { class: 'thesis' });
  setRenderedMarkdown(thesis, part.thesis_html);
  return h(
    'section',
    {
      class: 'design-part',
      id,
      'data-part': number,
      'aria-labelledby': `${id}-title`,
    },
    h(
      'header',
      { class: 'part-head' },
      h('span', { class: 'n', 'aria-hidden': 'true' }, number),
      h('h3', { class: 'eyebrow', id: `${id}-title` }, part.title),
      h('span', { class: 'part-of' }, `${number} / ${parts.length}`),
    ),
    thesis,
    markdown(part.body_html),
  );
}

class PartSpy {
  /** @param {HTMLElement} screen */
  constructor(screen) {
    this.screen = screen;
    /** @type {IntersectionObserver | null} */
    this.parts = null;
    this.ending = new IntersectionObserver(() => this.mark());
    this.onResize = () => this.watch();
    /** @type {{ number: number, until: number } | null} */
    this.reached = null;
  }

  watch() {
    this.parts?.disconnect();
    this.ending.disconnect();
    const below = Math.round(window.innerHeight - line());
    this.parts = new IntersectionObserver(() => this.mark(), { rootMargin: `0px 0px -${below}px 0px` });
    for (const section of this.screen.querySelectorAll('.design-part')) this.parts.observe(section);
    removeEventListener('resize', this.onResize);
    addEventListener('resize', this.onResize);
    const end = this.screen.querySelector('.design-end');
    if (end) this.ending.observe(end);
    this.mark();
  }

  /** @param {number} number */
  reach(number) {
    this.reached = { number, until: performance.now() + 300 };
    this.mark();
  }

  mark() {
    if (!this.screen.isConnected || this.screen.closest('[hidden]')) return;
    const sections = [...this.screen.querySelectorAll('.design-part')];
    let current = 1;
    sections.forEach((section, index) => {
      if (section.getBoundingClientRect().top <= line()) current = index + 1;
    });
    const end = this.screen.querySelector('.design-end');
    const atEnd = end !== null && end.getBoundingClientRect().top <= window.innerHeight;
    if (atEnd && window.scrollY > 0) current = sections.length;
    if (this.reached && performance.now() < this.reached.until) current = this.reached.number;
    this.screen.dataset.part = String(current);
    for (const marked of this.screen.querySelectorAll('[data-part]')) {
      if (!(marked instanceof HTMLElement)) continue;
      const number = Number(marked.dataset.part);
      marked.classList.toggle('current', number === current);
      marked.classList.toggle('read', number < current);
      const link = marked.tagName === 'LI' ? marked.querySelector('a') : null;
      if (link && number === current) link.setAttribute('aria-current', 'location');
      else link?.removeAttribute('aria-current');
    }
  }

  stop() {
    this.parts?.disconnect();
    this.ending.disconnect();
    removeEventListener('resize', this.onResize);
  }
}

function line() {
  return Math.min(window.innerHeight / 3, 240);
}
