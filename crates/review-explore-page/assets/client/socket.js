/**
 * @import { Call, Reply, Notification, StateParams } from "./types.ts"
 */

const SILENCE = 25_000;
const CHECK = 3_000;
const CHOSEN_DELAY = new URLSearchParams(location.search).get('reconnect-delay');
const FIRST_DELAY = CHOSEN_DELAY === null ? 250 : Number(CHOSEN_DELAY);
const LONGEST_DELAY = CHOSEN_DELAY === null ? 10_000 : Number(CHOSEN_DELAY);
const STABLE = 5_000;
const TOKEN_ENDED = 4001;

/**
 * @typedef {'connecting' | 'open' | 'down' | 'ended'} LinkState
 * @typedef {{ resolve: (reply: Reply) => void, reject: (error: Error) => void }} Pending
 */

export class Link {
  /**
   * @param {{ state: (params: StateParams) => void, link: (state: LinkState) => void }} listener
   * @param {string | null} past
   */
  constructor(listener, past) {
    this.listener = listener;
    this.past = past;
    /** @type {WebSocket | null} */
    this.socket = null;
    /** @type {LinkState} */
    this.state = 'connecting';
    this.attempts = 0;
    this.lastHeard = Date.now();
    this.nextId = 1;
    /** @type {Map<number, Pending>} */
    this.pending = new Map();
    /** @type {number | undefined} */
    this.retryTimer = undefined;
    /** @type {number | undefined} */
    this.stableTimer = undefined;
  }

  start() {
    this.connect();
    setInterval(() => this.watch(), 5_000);
    const check = () => this.check();
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') check();
    });
    window.addEventListener('pageshow', (event) => {
      if (event.persisted) this.connect();
    });
    window.addEventListener('online', check);
    window.addEventListener('focus', check);
    // A page with an open socket is not kept in the back-forward cache by every browser.
    window.addEventListener('pagehide', () => this.socket?.close(1000, 'The page is hidden'));
  }

  connect() {
    if (this.state === 'ended') return;
    clearTimeout(this.retryTimer);
    if (this.socket && this.socket.readyState <= WebSocket.OPEN) return;
    const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const query = this.past === null ? '' : `?past=${encodeURIComponent(this.past)}`;
    const socket = new WebSocket(`${scheme}//${location.host}/ws${query}`);
    this.socket = socket;
    this.lastHeard = Date.now();
    socket.addEventListener('open', () => {
      if (socket !== this.socket) return;
      this.setState('open');
      this.stableTimer = window.setTimeout(() => {
        this.attempts = 0;
      }, STABLE);
    });
    socket.addEventListener('message', (event) => {
      if (socket !== this.socket) return;
      this.lastHeard = Date.now();
      this.receive(String(event.data));
    });
    socket.addEventListener('close', (event) => {
      if (socket !== this.socket) return;
      this.socket = null;
      clearTimeout(this.stableTimer);
      this.failPending();
      if (event.code === TOKEN_ENDED) {
        this.setState('ended');
        return;
      }
      this.setState('down');
      this.retry(event.wasClean);
    });
  }

  /**
   * @param {Call} call
   * @returns {Promise<Reply>}
   */
  call(call) {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error('The socket is down'));
    }
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, ...call }));
    });
  }

  /** @param {string} text */
  receive(text) {
    /** @type {Reply | Notification} */
    const message = JSON.parse(text);
    if ('id' in message) {
      const pending = this.pending.get(message.id);
      this.pending.delete(message.id);
      pending?.resolve(message);
    } else if (message.method === 'state') {
      this.listener.state(message.params);
    }
  }

  /** @param {boolean} wasOpen */
  retry(wasOpen) {
    const ceiling = Math.min(LONGEST_DELAY, FIRST_DELAY * 2 ** this.attempts);
    this.attempts++;
    const delay = Math.random() * ceiling;
    this.retryTimer = window.setTimeout(async () => {
      if (!wasOpen && (await this.tokenEnded())) {
        this.setState('ended');
        return;
      }
      this.connect();
    }, delay);
  }

  async tokenEnded() {
    try {
      const response = await fetch('/token', { cache: 'no-store' });
      return response.status === 403;
    } catch {
      return false;
    }
  }

  watch() {
    if (this.state === 'open' && Date.now() - this.lastHeard > SILENCE) this.reopen();
  }

  async check() {
    if (this.state === 'ended') return;
    if (this.state !== 'open') {
      this.attempts = 0;
      this.connect();
      return;
    }
    const alive = await Promise.race([
      this.call({ method: 'ping' }).then(
        () => true,
        () => false,
      ),
      new Promise((resolve) => setTimeout(() => resolve(false), CHECK)),
    ]);
    if (!alive) this.reopen();
  }

  /** @param {string | null} past */
  follow(past) {
    if (past === this.past || this.state === 'ended') return;
    this.past = past;
    const socket = this.socket;
    this.socket = null;
    socket?.close(1000, 'The page shows another round');
    this.failPending();
    this.setState('connecting');
    this.attempts = 0;
    this.connect();
  }

  pastRefused() {
    this.past = null;
  }

  reopen() {
    const socket = this.socket;
    this.socket = null;
    socket?.close(4000, 'No word from the review tool');
    this.failPending();
    this.setState('down');
    this.attempts = 0;
    this.connect();
  }

  failPending() {
    for (const pending of this.pending.values()) pending.reject(new Error('The socket closed'));
    this.pending.clear();
  }

  /** @param {LinkState} state */
  setState(state) {
    if (this.state === state) return;
    this.state = state;
    this.listener.link(state);
  }
}
