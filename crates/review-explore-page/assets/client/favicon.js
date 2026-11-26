/** @import { PageView } from "./types.ts" */

import { keyOf } from './dom.js';
import { measured } from './meter.js';

const STYLE =
  '.t{fill:#1b1d21}.c{stroke:#3fb950}.g{fill:#3fb950}.k{fill:#3a3d42}.a{fill:#4493f8}' +
  '@media (prefers-color-scheme:light){.t{fill:#f5f7f9;stroke:#d2d4d7;stroke-width:1}' +
  '.c{stroke:#1a7f37}.g{fill:#1a7f37}.k{fill:#d2d4d7}.a{fill:#0969da}}';

const BAR = { x: 4, y: 21.5, w: 24, h: 6 };
const RUNNER = [
  [4, 6],
  [8.5, 8],
  [15.5, 8],
  [22, 6],
];
const FRAME_MS = 400;

/** @typedef {{ kind: 'mark' } | { kind: 'progress', step: number } | { kind: 'busy' }} Shown */

export class Favicon {
  constructor() {
    const link = document.querySelector('link[rel="icon"][type="image/svg+xml"]');
    if (!(link instanceof HTMLLinkElement)) throw new Error('the page names no SVG icon');
    this.link = link;
    this.mark = link.href;
    /** @type {Shown} */
    this.shown = { kind: 'mark' };
    this.frame = 0;
    this.reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
    this.reducedMotion.addEventListener('change', () => this.render());
    this.ticker = new Ticker(() => {
      this.frame = (this.frame + 1) % RUNNER.length;
      this.show(busySvg(this.frame));
    });
  }

  /** @param {PageView} view */
  update(view) {
    const tally = measured(view);
    /** @type {Shown} */
    let shown = { kind: 'mark' };
    if (view.title?.kind === 'agent_working') shown = { kind: 'busy' };
    else if (tally) shown = { kind: 'progress', step: progressStep(tally.change.share.marked, tally.change.share.changed) };
    if (keyOf(shown) === keyOf(this.shown)) return;
    this.shown = shown;
    this.render();
  }

  render() {
    this.ticker.stop();
    switch (this.shown.kind) {
      case 'mark':
        this.link.href = this.mark;
        return;
      case 'progress':
        this.show(progressSvg(this.shown.step));
        return;
      case 'busy':
        this.frame = this.reducedMotion.matches ? 1 : 0;
        this.show(busySvg(this.frame));
        if (!this.reducedMotion.matches) this.ticker.start(FRAME_MS);
    }
  }

  /** @param {string} svg */
  show(svg) {
    this.link.href = `data:image/svg+xml,${encodeURIComponent(svg)}`;
  }
}

/**
 * @param {number} marked
 * @param {number} changed
 */
function progressStep(marked, changed) {
  if (changed <= 0 || marked <= 0) return 0;
  if (marked >= changed) return 4;
  return Math.min(3, Math.max(1, Math.round((marked / changed) * 4)));
}

/** @param {string} bar */
function icon(bar) {
  return (
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">' +
    `<style>${STYLE}</style>` +
    '<rect class="t" x=".5" y=".5" width="31" height="31" rx="6"/>' +
    '<path class="c" d="M8 11.6 L12.6 16 L23.6 5.4" fill="none" stroke-width="4.2" ' +
    'stroke-linecap="round" stroke-linejoin="round"/>' +
    `<rect class="k" x="${BAR.x}" y="${BAR.y}" width="${BAR.w}" height="${BAR.h}" rx="3"/>` +
    bar +
    '</svg>'
  );
}

/** @param {number} step */
function progressSvg(step) {
  const width = (BAR.w * step) / 4;
  return icon(width ? `<rect class="g" x="${BAR.x}" y="${BAR.y}" width="${width}" height="${BAR.h}" rx="3"/>` : '');
}

/** @param {number} frame */
function busySvg(frame) {
  const [x, width] = RUNNER[frame];
  return icon(`<rect class="a" x="${x}" y="${BAR.y}" width="${width}" height="${BAR.h}" rx="3"/>`);
}

// A browser throttles a hidden page's timers to one a second, and Chrome to one a minute after
// five minutes, but a worker's less.
class Ticker {
  /** @param {() => void} tick */
  constructor(tick) {
    this.tick = tick;
    /** @type {ReturnType<typeof setInterval> | undefined} */
    this.timer = undefined;
    this.period = 0;
    /** @type {Worker | null} */
    this.worker = null;
    try {
      const worker = new Worker('/assets/client/favicon-ticker.js', { type: 'module' });
      worker.onmessage = () => {
        if (this.period > 0) this.tick();
      };
      // A browser worker that cannot load fires an error event; its constructor does not throw.
      worker.onerror = () => {
        this.worker = null;
        if (this.period > 0) this.start(this.period);
      };
      this.worker = worker;
    } catch {
      this.worker = null;
    }
  }

  /** @param {number} ms */
  start(ms) {
    this.stop();
    this.period = ms;
    if (this.worker) this.worker.postMessage(ms);
    else this.timer = setInterval(this.tick, ms);
  }

  stop() {
    this.period = 0;
    this.worker?.postMessage(0);
    clearInterval(this.timer);
  }
}
