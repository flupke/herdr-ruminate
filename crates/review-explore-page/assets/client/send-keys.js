export const SEND_KEYS_LABEL = /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘↵' : 'Ctrl+↵';

/**
 * @template {HTMLTextAreaElement} B
 * @param {B} box
 * @returns {B}
 */
export function sendsOnKeys(box) {
  box.setAttribute('aria-keyshortcuts', 'Control+Enter Meta+Enter');
  box.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter' || !(event.metaKey || event.ctrlKey) || event.isComposing) return;
    event.preventDefault();
    // `requestSubmit` shows the browser's bubble on a required field left empty.
    const send = box.form?.querySelector('button[type="submit"]');
    if (send instanceof HTMLButtonElement && !send.disabled) box.form?.requestSubmit(send);
  });
  return box;
}
