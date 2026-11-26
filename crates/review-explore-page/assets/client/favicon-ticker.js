// A browser throttles a worker's timers less than a hidden page's.

const scope = /** @type {{ postMessage(message: unknown): void, addEventListener(type: 'message', listener: (event: MessageEvent) => void): void }} */ (
  /** @type {unknown} */ (globalThis)
);

/** @type {ReturnType<typeof setInterval> | undefined} */
let timer;

scope.addEventListener('message', (event) => {
  clearInterval(timer);
  const ms = Number(event.data);
  if (ms > 0) timer = setInterval(() => scope.postMessage(0), ms);
});

export {};
