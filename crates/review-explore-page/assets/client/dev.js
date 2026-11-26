/** @param {number} version */
export async function reloadOnChange(version) {
  for (;;) {
    try {
      const response = await fetch(`/dev/changes?since=${version}`, { cache: 'no-store' });
      const changes = await response.json();
      if (changes.version !== version) {
        location.reload();
        return;
      }
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 1000));
    }
  }
}
