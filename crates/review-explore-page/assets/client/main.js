/** @import { Seq, StateParams } from "./types.ts" */

import { Actions } from './actions.js';
import { showConnection } from './connection.js';
import { reloadOnChange } from './dev.js';
import { configureDiagrams } from './diagrams.js';
import { Page } from './page.js';
import { leavePast, pastRound } from './route.js';
import { Link } from './socket.js';

const main = /** @type {HTMLElement} */ (document.getElementById('round'));
const line = /** @type {HTMLElement} */ (document.getElementById('connection'));
const header = /** @type {HTMLElement} */ (document.querySelector('header.masthead'));
const page = new Page(main, header, (call) => link.call(call));
/** @type {Seq} */
const NONE_SHOWN = { revision: -1, picks: -1, threads: -1, rounds: -1 };
/** @type {{ epoch: string, seq: Seq }} */
let shown = { epoch: '', seq: NONE_SHOWN };

/**
 * @param {Seq} seq
 * @param {Seq} than
 */
function after(seq, than) {
  if (seq.revision !== than.revision) return seq.revision > than.revision;
  if (seq.picks !== than.picks) return seq.picks > than.picks;
  if (seq.threads !== than.threads) return seq.threads > than.threads;
  return seq.rounds > than.rounds;
}

const link = new Link({
  state: (/** @type {StateParams} */ params) => {
    const newer = params.epoch !== shown.epoch || after(params.seq, shown.seq);
    if (!newer) return;
    page.notice = null;
    shown = { epoch: params.epoch, seq: params.seq };
    if (link.past !== null && params.view.past === null) {
      leavePast();
      link.pastRefused();
    }
    configureDiagrams(params.view.mermaid, (error) => {
      link.call({ method: 'diagram-failed', params: error }).catch(() => {
      });
    });
    page.render(params.view);
    main.dataset.seq = `${params.seq.revision}.${params.seq.picks}.${params.seq.threads}.${params.seq.rounds}`;
    actions.enable();
  },
  link: (state) => {
    if (state === 'open') shown = { epoch: '', seq: NONE_SHOWN };
    showConnection(line, state);
    actions.connection(state === 'open');
  },
}, pastRound());
const actions = new Actions(main, link, page);
link.start();
addEventListener('hashchange', () => link.follow(pastRound()));

const dev = main.dataset.dev;
if (dev) reloadOnChange(Number(dev));
