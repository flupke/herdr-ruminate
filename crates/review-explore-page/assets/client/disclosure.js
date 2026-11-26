import { h } from './dom.js';

let next = 0;

/**
 * @param {import('./dom.js').Children} label
 * @param {Node | Node[]} content
 * @param {{ open?: boolean, button?: string, onToggle?: (opened: boolean) => void }} [options]
 * @returns {{ element: HTMLElement, toggle: HTMLButtonElement }}
 */
export function disclosure(label, content, { open = false, button = 'disclosure-toggle', onToggle } = {}) {
  next += 1;
  const id = `disclosure-${next}`;
  const body = h('div', { class: 'disclosure-body', id, hidden: !open }, content);
  const toggle = h(
    'button',
    {
      class: button,
      type: 'button',
      'aria-expanded': String(open),
      'aria-controls': id,
      onclick: () => show(toggle, toggle.getAttribute('aria-expanded') !== 'true', onToggle),
    },
    label,
  );
  return { element: h('div', { class: 'disclosure' }, toggle, body), toggle };
}

/** @param {Element} element */
export function openDisclosure(element) {
  const toggle = element.querySelector(':scope > button[aria-controls]');
  if (toggle instanceof HTMLButtonElement && toggle.getAttribute('aria-expanded') !== 'true') toggle.click();
}

/**
 * @param {HTMLButtonElement} toggle
 * @param {boolean} opened
 * @param {((opened: boolean) => void) | undefined} onToggle
 */
function show(toggle, opened, onToggle) {
  toggle.setAttribute('aria-expanded', String(opened));
  const body = toggle.nextElementSibling;
  if (body instanceof HTMLElement) body.hidden = !opened;
  onToggle?.(opened);
}
