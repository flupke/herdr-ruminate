const storage = (() => {
  try {
    return window.sessionStorage;
  } catch {
    return null;
  }
})();

/**
 * @template {HTMLTextAreaElement | HTMLInputElement} B
 * @param {B} box
 * @param {string} key
 * @param {string} initial
 * @returns {B}
 */
export function keepDraft(box, key, initial) {
  box.dataset.draft = key;
  box.value = kept(key) ?? initial;
  box.addEventListener('input', () => keep(key, box.value));
  return box;
}

/**
 * @param {string} key
 * @returns {string | null}
 */
export function kept(key) {
  try {
    return storage?.getItem(`explore-draft:${key}`) ?? null;
  } catch {
    // The browser may refuse this page its storage.
    return null;
  }
}

/**
 * @param {string} key
 * @returns {string | null}
 */
export function typed(key) {
  for (const box of document.querySelectorAll('[data-draft]')) {
    if ((box instanceof HTMLTextAreaElement || box instanceof HTMLInputElement) && box.dataset.draft === key) {
      return box.value;
    }
  }
  return kept(key);
}

/**
 * @param {string} key
 * @param {string | null} value
 */
export function keep(key, value) {
  try {
    if (value === null) storage?.removeItem(`explore-draft:${key}`);
    else storage?.setItem(`explore-draft:${key}`, value);
  } catch {
    // The browser may refuse this page its storage.
  }
}
