// The progressive reviewer's launcher: its key (keys.json: `mod.open:herdr-progressive-reviewer`)
// opens this mod as an overlay, which toggles the review pane of the focused tab and closes at
// once. The review pane opens beside the focused terminal, in its folder, right of it when the
// terminal is wide and below it otherwise; the terminal is the first agent it reviews for.
// `make install` writes it here from `herdr-app-mod/` of the reviewer's checkout, with the
// path of that checkout's reviewer in place of `@REVIEWER@`.

import { View, div } from "gpui-kit";
import { closeOverlay, focused, notify, panes, props, run, spaces, terminals } from "herdr";

// The review pane's program.
const REVIEWER = "@REVIEWER@";

// The option the review pane's program takes the focused terminal with: with a program ending
// in `/bin/reviewer`, it tells a review pane apart, whichever checkout installed it.
const FOCUSED_TERMINAL = "--focused-terminal";

// A terminal at least this many times as many columns as rows splits right, else down: a
// cell is about twice as tall as it is wide (Herdr's rule, `PaneSize::split_direction`).
const WIDE = 5 / 2;

// Whether `terminal` runs a review pane.
function isReview(terminal) {
  return terminal.state === "live" && Boolean(terminal.argv?.[0]?.endsWith("/bin/reviewer")) && terminal.argv[1] === FOCUSED_TERMINAL;
}

// The panes showing a review pane in `tab`.
function reviewPanes(tab) {
  const reviews = new Set(terminals().filter((t) => isReview(t) && t.labels?.["herdr.tab"] === tab).map((t) => t.address));
  return panes().filter((p) => reviews.has(p.terminal));
}

function fail(body) {
  notify({ title: "Progressive reviewer", body, level: "error" });
}

// Closes the review panes of the focused tab, or opens one beside the focused terminal.
function toggle() {
  const at = focused();
  if (!at?.tab) return;
  // Another machine's terminals run there, where this computer's reviewer is not, and its ids
  // carry a `<machine>/` prefix its labels lack.
  if (spaces().find((s) => s.id === at.space)?.machine) {
    return fail("The review pane runs on this computer only.");
  }
  const open = reviewPanes(at.tab);
  if (open.length) return open.forEach((p) => run("pane.close", { pane: p.pane }));
  if (!at.terminal || !at.pane) return fail("Focus your agent's terminal, then press the key again.");
  const terminal = terminals().find((t) => t.address === at.terminal);
  const wide = terminal && terminal.cols >= WIDE * terminal.rows;
  run("terminal.open", {
    argv: [REVIEWER, FOCUSED_TERMINAL, at.terminal],
    folder: at.cwd ?? terminal?.cwd,
    place: wide ? "right" : "down",
  });
}

export default class Launcher extends View {
  init() {
    // Shown elsewhere than as the key's overlay (`herdr-app check`), it does nothing.
    const overlay = props()?.overlay;
    if (!overlay) return;
    closeOverlay(overlay);
    toggle();
  }

  render() {
    return div();
  }
}
