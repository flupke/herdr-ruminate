/**
 * @import { DiagramParams } from "./types.ts"
 */

/**
 * @typedef {{ source: string, block: Element, figure: HTMLElement, failed: boolean,
 *   turned: boolean, wide: number }} Diagram
 * @typedef {{ initialize(options: object): void, parse(source: string): Promise<unknown>,
 *   render(id: string, source: string): Promise<{ svg: string }> }} Mermaid
 */

import { disclosure } from './disclosure.js';
import { h, setDiagramDrawing } from './dom.js';

const FENCE = 'mermaid';
export const DIAGRAM_SOURCE = `pre:has(> code.language-${FENCE})`;
export const DIAGRAMS_RESIZED = 'explore:diagrams-resized';
const dark = matchMedia('(prefers-color-scheme: dark)');
const SMALLEST = 11 / 14;
const phone = matchMedia('(max-width: 40rem)');
const SIDEWAYS = /^(\s*(?:%%[^\n]*\n\s*)*(?:flowchart|graph)\s+)(?:LR|RL)\b/;
const CLASSED = /:::\s*(?:new|changed)\b|^\s*class\s+\S+\s+(?:new|changed)\s*;?\s*$/m;
const PARTICIPANT = /^\s*(?:participant|actor)\s/gm;

/** @type {Diagram[]} */
const diagrams = [];
/** @type {Map<string, { svg: string, id: string, turned: boolean, wide: number } | { message: string }>} */
const drawings = new Map();
let themes = 0;
/** @type {Promise<Mermaid> | null} */
let library = null;
let renders = 0;
let draws = Promise.resolve();
/** @type {(error: DiagramParams) => void} */
let report = () => {};
let script = '';

/**
 * @param {string} address
 * @param {(error: DiagramParams) => void} reporter
 */
export function configureDiagrams(address, reporter) {
  script = address;
  report = reporter;
}

/**
 * @param {Element} root
 * @returns {Promise<void>}
 */
export function drawDiagrams(root) {
  watch(root);
  for (let index = diagrams.length - 1; index >= 0; index--) {
    const { block, figure } = /** @type {Diagram} */ (diagrams[index]);
    if (!block.isConnected && !figure.isConnected) diagrams.splice(index, 1);
  }
  const blocks = root.querySelectorAll(`.markdown pre > code.language-${FENCE}`);
  /** @type {Diagram[]} */
  const found = [];
  for (const code of blocks) {
    const block = code.parentElement;
    if (!block || block.closest('figure.diagram') || diagrams.some((diagram) => diagram.block === block)) continue;
    const figure = document.createElement('figure');
    figure.className = 'diagram';
    const diagram = { source: code.textContent ?? '', block, figure, failed: false, turned: false, wide: 0 };
    diagrams.push(diagram);
    const drawn = drawings.get(diagram.source);
    if (drawn && 'message' in drawn) fail(diagram, drawn.message);
    else if (drawn && !document.getElementById(drawn.id)) {
      diagram.turned = drawn.turned;
      diagram.wide = drawn.wide;
      show(diagram, drawn.svg);
    } else found.push(diagram);
  }
  number();
  if (found.length > 0) queue(found);
  return settled();
}

async function settled() {
  let last;
  do {
    last = draws;
    await last;
  } while (last !== draws);
}

export function fitDiagrams() {
  const turning = diagrams.filter((diagram) => !fit(diagram));
  if (turning.length > 0) queue(turning);
  document.dispatchEvent(new Event(DIAGRAMS_RESIZED));
}

let pending = 0;

/** @param {Diagram[]} some */
function queue(some) {
  pending++;
  document.body.dataset.diagrams = 'drawing';
  draws = draws
    .then(async () => {
      const mermaid = await load();
      for (const diagram of some) await draw(mermaid, diagram);
    })
    .catch(() => {
    })
    .finally(() => {
      if (--pending === 0) delete document.body.dataset.diagrams;
      document.dispatchEvent(new Event(DIAGRAMS_RESIZED));
    });
}

const REFIT_DELAY = 100;
let resizing = 0;
/** @type {WeakMap<Element, number>} */
const widths = new WeakMap();
const resized = new ResizeObserver((entries) => {
  const changed = entries.filter((entry) => entry.contentRect.width !== widths.get(entry.target));
  if (changed.length === 0) return;
  for (const entry of changed) widths.set(entry.target, entry.contentRect.width);
  clearTimeout(resizing);
  resizing = setTimeout(fitDiagrams, REFIT_DELAY);
});

/** @param {Element} region */
function watch(region) {
  if (widths.has(region)) return;
  widths.set(region, 0);
  resized.observe(region);
}

function load() {
  library ??= new Promise((resolve, reject) => {
    const element = document.createElement('script');
    element.src = script;
    element.onload = () => {
      dark.addEventListener('change', () => {
        themes++;
        drawings.clear();
        queue([...diagrams]);
      });
      resolve(/** @type {{ mermaid: Mermaid }} */ (/** @type {unknown} */ (window)).mermaid);
    };
    element.onerror = () => reject(new Error('Mermaid did not load'));
    document.head.append(element);
  });
  return library;
}

/**
 * @param {Mermaid} mermaid
 * @param {Diagram} diagram
 */
async function draw(mermaid, diagram) {
  if (diagram.failed || !(diagram.block.isConnected || diagram.figure.isConnected)) return;
  const { turned } = diagram;
  const source = turned ? diagram.source.replace(SIDEWAYS, '$1TD') : diagram.source;
  const theme = themes;
  mermaid.initialize(configuration());
  try {
    await mermaid.parse(diagram.source);
    const id = `diagram-${++renders}`;
    const { svg } = await mermaid.render(id, source);
    show(diagram, svg);
    if (theme === themes) drawings.set(diagram.source, { svg, id, turned, wide: diagram.wide });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (theme === themes) drawings.set(diagram.source, { message });
    fail(diagram, message);
  }
  number();
}

function configuration() {
  const colours = tokens();
  const font = getComputedStyle(document.body).getPropertyValue('--font').trim() || 'system-ui, sans-serif';
  return {
    startOnLoad: false,
    // Mermaid's strict security level encodes HTML in labels and turns click directives off.
    securityLevel: 'strict',
    theme: 'base',
    fontFamily: font,
    themeVariables: {
      fontFamily: font,
      fontSize: '14px',
      background: colours.canvas,
      primaryColor: colours.panel,
      mainBkg: colours.panel,
      actorBkg: colours.panel,
      primaryBorderColor: colours.lineStrong,
      nodeBorder: colours.lineStrong,
      actorBorder: colours.lineStrong,
      primaryTextColor: colours.ink,
      textColor: colours.ink,
      signalColor: colours.ink,
      signalTextColor: colours.ink,
      actorTextColor: colours.ink,
      labelTextColor: colours.ink,
      lineColor: colours.muted,
      actorLineColor: colours.line,
      loopTextColor: colours.muted,
      edgeLabelBackground: colours.panel,
      labelBoxBkgColor: colours.canvas,
      labelBoxBorderColor: colours.lineStrong,
      noteBkgColor: colours.warnTint,
      noteBorderColor: colours.warn,
      noteTextColor: colours.ink,
      sequenceNumberColor: colours.canvas,
    },
    themeCSS: [
      `.node.new rect, .node.new polygon, .node.new path { fill: ${colours.goodTint} !important;`,
      `stroke: ${colours.good} !important; stroke-width: 2px !important; }`,
      `.node.changed rect, .node.changed polygon, .node.changed path { fill: ${colours.warnTint} !important;`,
      `stroke: ${colours.warn} !important; stroke-width: 2px !important; }`,
    ].join(' '),
    sequence: {
      mirrorActors: false,
      actorMargin: 22,
      width: 112,
      height: 40,
      messageMargin: 30,
      diagramMarginX: 8,
      diagramMarginY: 18,
      useMaxWidth: false,
    },
    flowchart: { curve: 'basis', nodeSpacing: 30, rankSpacing: 34, padding: 10, useMaxWidth: false },
  };
}

// Mermaid needs literal, opaque colours.
function tokens() {
  const probe = h('span', { hidden: true });
  document.body.append(probe);
  /** @param {string} value */
  const resolve = (value) => {
    probe.style.color = value;
    return getComputedStyle(probe).color;
  };
  const panel = resolve('var(--panel)');
  const canvas = document.createElement('canvas');
  canvas.width = 1;
  canvas.height = 1;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  /** @param {string} value */
  const opaque = (value) => {
    if (!context) return resolve(value);
    context.fillStyle = panel;
    context.fillRect(0, 0, 1, 1);
    context.fillStyle = resolve(value);
    context.fillRect(0, 0, 1, 1);
    const [red, green, blue] = context.getImageData(0, 0, 1, 1).data;
    return `#${[red, green, blue].map((channel) => (channel ?? 0).toString(16).padStart(2, '0')).join('')}`;
  };
  const colours = {
    canvas: opaque('var(--canvas)'),
    panel: opaque('var(--panel)'),
    ink: opaque('var(--ink)'),
    muted: opaque('var(--muted)'),
    line: opaque('var(--line)'),
    lineStrong: opaque('var(--line-strong)'),
    good: opaque('var(--good)'),
    goodTint: opaque('color-mix(in srgb, var(--good) 18%, var(--panel))'),
    warn: opaque('var(--warn)'),
    warnTint: opaque('color-mix(in srgb, var(--warn) 12%, var(--panel))'),
  };
  probe.remove();
  return colours;
}

/**
 * @param {Diagram} diagram
 * @param {string} svg
 */
function show(diagram, svg) {
  const drawing = h('div', { class: 'drawing' });
  setDiagramDrawing(drawing, svg);
  const caption = h(
    'figcaption',
    {},
    CLASSED.test(diagram.source)
      ? h(
          'span',
          { class: 'legend' },
          h('span', { class: 'key new' }, 'new'),
          h('span', { class: 'key changed' }, 'changed'),
          h('span', { class: 'key unchanged' }, 'unchanged'),
        )
      : null,
    h('span', { class: 'scroll-hint' }, scrollHint(diagram.source)),
    h('button', { class: 'diagram-open', type: 'button', hidden: true, onclick: () => openLarge(diagram) }, 'Open large'),
  );
  diagram.figure.replaceChildren(drawing, caption);
  if (diagram.block.isConnected) diagram.block.replaceWith(diagram.figure);
  const width = naturalWidth(diagram.figure);
  if (!diagram.turned && SIDEWAYS.test(diagram.source)) diagram.wide = width;
  if (!fit(diagram)) queue([diagram]);
}

/** @param {string} source */
function scrollHint(source) {
  const participants = source.match(PARTICIPANT)?.length ?? 0;
  return participants > 0 ? `Scroll sideways · ${participants} participants` : 'Scroll sideways';
}

/** @param {HTMLElement} figure */
export function wholeWidth(figure) {
  return figure.offsetWidth - roomOf(figure) + naturalWidth(figure);
}

/** @param {Element} figure */
function naturalWidth(figure) {
  const drawing = figure.querySelector('.drawing > svg');
  return drawing instanceof SVGSVGElement ? (drawing.viewBox.baseVal?.width ?? 0) : 0;
}

/**
 * @param {Diagram} diagram
 * @returns {boolean}
 */
function fit(diagram) {
  const { figure } = diagram;
  const drawing = figure.querySelector('.drawing > svg');
  if (!(drawing instanceof SVGSVGElement) || diagram.failed || figure.classList.contains('large')) return true;
  figure.classList.remove('wide');
  const room = roomOf(figure);
  if (room <= 0) return true;
  figure.classList.add('wide');
  const column = roomOf(figure);
  figure.classList.remove('wide');
  if (diagram.wide > 0) {
    const turn = diagram.wide * SMALLEST > column;
    if (turn !== diagram.turned) {
      diagram.turned = turn;
      return false;
    }
  }
  const width = naturalWidth(diagram.figure);
  if (width <= 0) return true;
  const readable = width * SMALLEST <= room;
  const widens = !readable && width > room && column > room;
  const scrolls = width * SMALLEST > column && phone.matches;
  const space = widens ? column : room;
  figure.classList.toggle('wide', widens && !scrolls);
  figure.classList.toggle('scrolls', scrolls);
  drawing.style.maxWidth = 'none';
  drawing.style.width = `${scrolls ? width : Math.min(width, space)}px`;
  const open = figure.querySelector('.diagram-open');
  if (open instanceof HTMLElement) open.hidden = !scrolls && width <= space;
  return true;
}

/** @param {HTMLElement} element */
export function roomOf(element) {
  const style = getComputedStyle(element);
  return element.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
}

/** @type {HTMLDialogElement | null} */
let dialog = null;

/** @param {Diagram} diagram */
function openLarge(diagram) {
  const drawing = diagram.figure.querySelector('.drawing > svg');
  if (!(drawing instanceof SVGSVGElement)) return;
  const name = diagram.figure.getAttribute('aria-label') ?? 'Diagram';
  const frame = h('div', { class: 'diagram-large' });
  const close = h('button', { class: 'button secondary', type: 'button' }, 'Close');
  dialog?.remove();
  const shown = /** @type {HTMLDialogElement} */ (
    h('dialog', { class: 'diagram-dialog', 'aria-label': `${name} at full size` }, h('div', { class: 'diagram-dialog-head' }, h('p', { class: 'eyebrow' }, name), close), frame)
  );
  dialog = shown;
  const home = drawing.parentElement;
  const width = drawing.style.width;
  drawing.style.width = `${naturalWidth(diagram.figure)}px`;
  frame.append(drawing);
  diagram.figure.classList.add('large');
  close.addEventListener('click', () => shown.close());
  shown.addEventListener('close', () => {
    drawing.style.width = width;
    home?.append(drawing);
    diagram.figure.classList.remove('large');
    shown.remove();
    if (dialog === shown) dialog = null;
    fit(diagram);
    const opener = diagram.figure.querySelector('.diagram-open');
    if (opener instanceof HTMLElement) opener.focus();
  });
  document.body.append(shown);
  shown.showModal();
}

/**
 * @param {Diagram} diagram
 * @param {string} message
 */
function fail(diagram, message) {
  diagram.failed = true;
  const caption = h('figcaption', {}, 'This diagram could not be drawn; here is its source.');
  const why = disclosure('Mermaid’s message', h('pre', { class: 'diagram-error' }, message));
  diagram.figure.classList.add('failed');
  diagram.figure.replaceChildren(caption, diagram.block.cloneNode(true), why.element);
  if (diagram.block.isConnected) diagram.block.replaceWith(diagram.figure);
  const question = diagram.figure.closest('[data-question]');
  if (question instanceof HTMLElement) {
    report({
      question: question.dataset.question ?? '',
      version: Number(question.dataset.version),
      source: diagram.source,
      message,
    });
  }
}

function number() {
  document.querySelectorAll('figure.diagram').forEach((figure, index) => {
    figure.setAttribute('aria-label', `Diagram ${index + 1}`);
  });
}
