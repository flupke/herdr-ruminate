/**
 * @typedef {{ address: string, element: HTMLElement, step: string }} Screen
 * @typedef {{ screens: Screen[], shown: number }} Track
 */

const THRESHOLD = 48;
const FLICK = 0.5;
const SLOP = 8;
const RUBBER_BAND = 0.35;
const SNAP = 280;
const EASING = 'cubic-bezier(.2, .8, .2, 1)';
const PHONE = matchMedia('(max-width: 40rem)');

export class Swipe {
  /**
   * @param {HTMLElement} main
   * @param {{ track: () => Track, settled: (address: string | null, top: number) => void,
   *   chip: (step: string, on: boolean) => void }} page
   */
  constructor(main, page) {
    this.main = main;
    this.page = page;
    /** @type {Gesture | null} */
    this.gesture = null;
    this.settling = false;
    this.swallowClick = false;
    this.edges = {
      left: edge('left'),
      right: edge('right'),
    };
    document.body.append(this.edges.left, this.edges.right);
    main.addEventListener('pointerdown', (event) => this.down(event));
    main.addEventListener('pointermove', (event) => this.move(event));
    main.addEventListener('pointerup', (event) => this.up(event));
    main.addEventListener('pointercancel', () => this.cancel());
    main.addEventListener(
      'click',
      (event) => {
        if (!this.swallowClick) return;
        this.swallowClick = false;
        event.preventDefault();
        event.stopPropagation();
      },
      true,
    );
  }

  get active() {
    return this.settling || Boolean(this.gesture?.drag);
  }

  /** @param {PointerEvent} event */
  down(event) {
    this.swallowClick = false;
    if (this.gesture || this.settling || !event.isPrimary || event.button !== 0 || !PHONE.matches) return;
    if (!(event.target instanceof Element) || keepsGesture(event.target, this.main)) return;
    this.gesture = {
      pointer: event.pointerId,
      x0: event.clientX,
      y0: event.clientY,
      lastX: event.clientX,
      lastT: event.timeStamp,
      velocity: 0,
      dx: 0,
      drag: null,
    };
  }

  /** @param {PointerEvent} event */
  move(event) {
    const gesture = this.gesture;
    if (!gesture || event.pointerId !== gesture.pointer) return;
    const dx = event.clientX - gesture.x0;
    const dy = event.clientY - gesture.y0;
    if (!gesture.drag) {
      if (Math.abs(dx) < SLOP && Math.abs(dy) < SLOP) return;
      if (Math.abs(dy) >= Math.abs(dx)) {
        this.gesture = null;
        return;
      }
      gesture.drag = this.startDrag();
      if (!gesture.drag) {
        this.gesture = null;
        return;
      }
      this.main.setPointerCapture?.(event.pointerId);
      getSelection()?.removeAllRanges();
    }
    const elapsed = Math.max(1, event.timeStamp - gesture.lastT);
    gesture.velocity = (event.clientX - gesture.lastX) / elapsed;
    gesture.lastX = event.clientX;
    gesture.lastT = event.timeStamp;
    gesture.dx = dx;
    this.follow(gesture.drag, dx);
  }

  /** @param {PointerEvent} event */
  up(event) {
    const gesture = this.gesture;
    if (!gesture || event.pointerId !== gesture.pointer) return;
    this.gesture = null;
    const drag = gesture.drag;
    if (!drag) return;
    this.swallowClick = true;
    // The browser fires no click after a touch drag.
    setTimeout(() => {
      this.swallowClick = false;
    });
    const { dx } = gesture;
    const velocity = event.timeStamp - gesture.lastT > 100 ? 0 : gesture.velocity;
    const forward = (dx <= -THRESHOLD || velocity < -FLICK) && drag.next !== null;
    const backward = (dx >= THRESHOLD || velocity > FLICK) && drag.previous !== null;
    const to = forward ? drag.next : backward ? drag.previous : null;
    this.settle(drag, to, forward ? -1 : backward ? 1 : 0);
  }

  cancel() {
    const drag = this.gesture?.drag;
    this.gesture = null;
    if (drag) this.settle(drag, null, 0);
  }

  /** @returns {Drag | null} */
  startDrag() {
    const { screens, shown } = this.page.track();
    const current = screens[shown];
    if (!current || screens.length < 2) return null;
    const box = current.element.getBoundingClientRect();
    const top = Math.max(box.top, 0);
    const drag = {
      current,
      previous: screens[shown - 1] ?? null,
      next: screens[shown + 1] ?? null,
      width: box.width + 2 * box.left,
      top,
      box,
      armed: /** @type {Screen | null} */ (null),
    };
    this.main.classList.add('swiping');
    for (const [neighbour, side] of /** @type {const} */ ([
      [drag.previous, -1],
      [drag.next, 1],
    ])) {
      if (!neighbour) continue;
      const element = neighbour.element;
      element.hidden = false;
      element.classList.add('swipe-neighbour');
      Object.assign(element.style, {
        top: `${top}px`,
        left: `${box.left}px`,
        width: `${box.width}px`,
        transform: `translateX(${side * drag.width}px)`,
      });
    }
    return drag;
  }

  /**
   * @param {Drag} drag
   * @param {number} dx
   */
  follow(drag, dx) {
    const toward = dx < 0 ? drag.next : drag.previous;
    const offset = toward ? dx : dx * RUBBER_BAND;
    this.place(drag, offset, false);
    const armed = toward && Math.abs(dx) >= THRESHOLD ? toward : null;
    if (armed === drag.armed) return;
    if (drag.armed) this.page.chip(drag.armed.step, false);
    drag.armed = armed;
    if (armed) {
      this.page.chip(armed.step, true);
      navigator.vibrate?.(10);
    }
    this.edges.left.classList.toggle('lit', armed !== null && dx > 0);
    this.edges.right.classList.toggle('lit', armed !== null && dx < 0);
    for (const element of [this.edges.left, this.edges.right]) element.style.top = `${drag.top}px`;
  }

  /**
   * @param {Drag} drag
   * @param {number} offset
   * @param {boolean} animate
   */
  place(drag, offset, animate) {
    const transition = animate ? `transform ${SNAP}ms ${EASING}` : 'none';
    drag.current.element.style.transition = transition;
    drag.current.element.style.transform = `translateX(${offset}px)`;
    if (drag.previous) {
      drag.previous.element.style.transition = transition;
      drag.previous.element.style.transform = `translateX(${offset - drag.width}px)`;
    }
    if (drag.next) {
      drag.next.element.style.transition = transition;
      drag.next.element.style.transform = `translateX(${offset + drag.width}px)`;
    }
  }

  /**
   * @param {Drag} drag
   * @param {Screen | null} to
   * @param {number} direction
   */
  settle(drag, to, direction) {
    this.settling = true;
    this.place(drag, to ? direction * drag.width : 0, true);
    const done = () => {
      this.settling = false;
      if (drag.armed) this.page.chip(drag.armed.step, false);
      this.edges.left.classList.remove('lit');
      this.edges.right.classList.remove('lit');
      for (const screen of [drag.current, drag.previous, drag.next]) {
        if (!screen) continue;
        screen.element.classList.remove('swipe-neighbour');
        for (const property of ['top', 'left', 'width', 'transform', 'transition']) {
          screen.element.style.removeProperty(property);
        }
      }
      this.main.classList.remove('swiping');
      if (!to) for (const screen of [drag.previous, drag.next]) if (screen) screen.element.hidden = true;
      this.page.settled(to ? to.address : null, drag.top);
    };
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) done();
    else setTimeout(done, SNAP);
  }
}

/**
 * @typedef {{ pointer: number, x0: number, y0: number, lastX: number, lastT: number,
 *   velocity: number, dx: number, drag: Drag | null }} Gesture
 * @typedef {{ current: Screen, previous: Screen | null, next: Screen | null, width: number,
 *   top: number, box: DOMRect, armed: Screen | null }} Drag
 */

/** @param {'left' | 'right'} side */
function edge(side) {
  const element = document.createElement('div');
  element.className = `swipe-edge ${side}`;
  element.setAttribute('aria-hidden', 'true');
  return element;
}

/**
 * @param {Element} target
 * @param {Element} main
 */
function keepsGesture(target, main) {
  if (target.closest('input, textarea, select, [contenteditable]')) return true;
  for (let element = /** @type {Element | null} */ (target); element && element !== main; element = element.parentElement) {
    if (element.scrollWidth <= element.clientWidth + 1) continue;
    const overflow = getComputedStyle(element).overflowX;
    if (overflow === 'auto' || overflow === 'scroll') return true;
  }
  return false;
}
