/** @import { PageView, TabTitle } from "./types.ts" */

import { revisionText } from './revision.js';

const KEY = 'explore-notify';

/** @typedef {'on' | 'off' | 'blocked' | 'unavailable'} NotifyState */

export class Notifier {
  /** @param {() => void} changed */
  constructor(changed) {
    this.changed = changed;
    this.wanted = read() === 'on';
    /** @type {{ title: TabTitle | null } | null} */
    this.last = null;
    /** @type {Set<string>} */
    this.replies = new Set();
  }

  /** @returns {NotifyState} */
  state() {
    // Browsers offer notifications to secure pages only.
    if (!('Notification' in window) || !window.isSecureContext) return 'unavailable';
    if (Notification.permission === 'denied') return 'blocked';
    return this.wanted && Notification.permission === 'granted' ? 'on' : 'off';
  }

  async toggle() {
    const state = this.state();
    if (state === 'unavailable' || state === 'blocked') return;
    this.wanted = state === 'off';
    if (this.wanted && Notification.permission === 'default') {
      this.wanted = (await Notification.requestPermission()) === 'granted';
    }
    write(this.wanted ? 'on' : 'off');
    this.changed();
  }

  /** @param {PageView} view */
  update(view) {
    const last = this.last;
    this.last = { title: view.title };
    let added = 0;
    for (const message of view.conversation?.messages ?? []) {
      if (message.author !== 'agent' || this.replies.has(message.id)) continue;
      this.replies.add(message.id);
      added += 1;
    }
    if (!last || this.state() !== 'on' || document.hasFocus()) return;
    const finished = last.title?.kind === 'agent_working' ? turnEnd(view.title) : null;
    const what = finished ?? (added > 0 ? replied(added) : null);
    if (!what) return;
    const review = view.review ? view.review.title || revisionText(view.review.revision) : 'Explore';
    const notification = new Notification(what, { body: review, tag: 'explore' });
    notification.onclick = () => {
      window.focus();
      notification.close();
    };
  }
}

/** @param {TabTitle | null} title */
function turnEnd(title) {
  switch (title?.kind) {
    case 'your_turn':
      return `Question ${title.question} is ready`;
    case 'conclusion':
      return 'The conclusion is ready';
    case 'retry_needed':
      return 'The agent stopped: the round needs a retry';
    default:
      return null;
  }
}

/** @param {number} count */
function replied(count) {
  return count === 1 ? 'The agent replied in the chat' : `The agent replied ${count} times in the chat`;
}

function read() {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

/** @param {'on' | 'off'} value */
function write(value) {
  try {
    localStorage.setItem(KEY, value);
  } catch {
  }
}
