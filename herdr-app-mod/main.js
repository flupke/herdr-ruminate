// The progressive reviewer's launcher: its key (keys.json: `mod.open:herdr-progressive-reviewer`)
// opens this mod as an overlay, which toggles the review pane of the focused tab and closes at
// once. The review pane opens beside the focused terminal, in its folder, right of it when the
// terminal is wide and below it otherwise; the terminal is the first agent it reviews for.
// `make install` writes it here from `herdr-app-mod/` of the reviewer's checkout, with the
// path of that checkout's reviewer in place of `@REVIEWER@`.
//
// Beside an agent of another machine, the review pane runs there, with the agent's repository
// and herdr-core: the reviewer that `make install` linked on that machine (`LINKED_REVIEWER`).

import { View, div } from "gpui-kit";
import { appHome, closeOverlay, exec, focused, notify, panes, props, run, terminals } from "herdr";

// The review pane's program on this computer.
const REVIEWER = "@REVIEWER@";

// A script that prints the path of the reviewer `make install` linked on another machine, run
// there in its home with its core's environment, or exits with NOT_INSTALLED when there is none:
// the link is `herdr-progressive-reviewer/reviewer` in the XDG data folder of the shell that ran
// `make install`, which may set an `XDG_DATA_HOME` the core lacks, or lack one the core has.
const NOT_INSTALLED = 3;
const LINKED_REVIEWER = `for data in "\${XDG_DATA_HOME:-}" "$HOME/.local/share"; do link="$data/herdr-progressive-reviewer/reviewer"; [ -n "$data" ] && [ -x "$link" ] && exec readlink -f "$link"; done; exit ${NOT_INSTALLED}`;

// How long the launcher waits for another machine to name its reviewer.
const MACHINE_TIMEOUT_MS = 5000;

// The option the review pane's program takes the focused terminal with: with a program ending
// in `/bin/reviewer`, it tells a review pane apart, whichever checkout installed it.
const FOCUSED_TERMINAL = "--focused-terminal";

// The option that names this window's home, whose `view.json` the review pane follows the focus
// in: herdr-core starts the pane with its own environment, not the window's.
const HERDR_APP_HOME = "--herdr-app-home";

// A terminal at least this many times as many columns as rows splits right, else down: a
// cell is about twice as tall as it is wide (Herdr's rule, `PaneSize::split_direction`).
const WIDE = 5 / 2;

// Whether `terminal` runs a review pane.
function isReview(terminal) {
  return terminal.state === "live" && Boolean(terminal.argv?.[0]?.endsWith("/bin/reviewer")) && terminal.argv[1] === FOCUSED_TERMINAL;
}

// `id` of a space, a tab or a terminal on `machine` (null for this computer) as that machine's
// core names it: the window gives another machine's ids a `<machine>/` prefix, which the core
// there, and the labels of its terminals, lack.
function onMachine(id, machine) {
  const prefix = `${machine}/`;
  return machine && id.startsWith(prefix) ? id.slice(prefix.length) : id;
}

// The machine a terminal runs on, by its address (`<machine>/t7`): null for this computer.
function machineOf(address) {
  const slash = address.indexOf("/");
  return slash < 0 ? null : address.slice(0, slash);
}

// The panes showing a review pane in `tab`. A review pane's terminal runs on the machine of the
// agent it opened beside, which may not be its space's: its `herdr.tab` label names the tab as
// that terminal's machine does, without the prefix of its own machine.
function reviewPanes(tab) {
  const reviews = new Set(
    terminals()
      .filter((t) => isReview(t) && t.labels?.["herdr.tab"] === onMachine(tab, machineOf(t.address)))
      .map((t) => t.address),
  );
  return panes().filter((p) => reviews.has(p.terminal));
}

function fail(body) {
  notify({ title: "Progressive reviewer", body, level: "error" });
}

// Closes the review panes of the focused tab, or opens one beside the focused terminal. `close`
// closes the key's overlay: the launcher's copy, and its tasks, live only while the overlay
// shows, so an opening beside an agent of another machine closes it only once that machine named
// its reviewer. The overlay is a window of 2×2 pixels that shows nothing: unlike a popup, it
// takes no key and no click closes it meanwhile (`mod.json`).
async function toggle(close) {
  const at = focused();
  if (!at?.tab) return;
  const open = reviewPanes(at.tab);
  if (open.length) {
    close();
    return open.forEach((p) => run("pane.close", { pane: p.pane }));
  }
  if (!at.terminal || !at.pane) return fail("Focus your agent's terminal, then press the key again.");
  const terminal = terminals().find((t) => t.address === at.terminal);
  const wide = terminal && terminal.cols >= WIDE * terminal.rows;
  // A split runs on the machine of the terminal it splits, whatever its space's.
  const machine = machineOf(at.terminal);
  if (!machine) {
    close();
    return run("terminal.open", {
      argv: [REVIEWER, FOCUSED_TERMINAL, at.terminal, HERDR_APP_HOME, appHome()],
      folder: at.cwd ?? terminal?.cwd,
      place: wide ? "right" : "down",
    });
  }
  const reviewer = await reviewerOn(machine);
  close();
  if (!reviewer) return;
  // The agent's pane by its ID, since the focus may have moved while the machine answered: the
  // split runs there, in the agent's folder. The pane gets no window home: the window's
  // `view.json` is on this computer.
  run("pane.split", {
    pane: at.pane,
    direction: wide ? "right" : "down",
    argv: [reviewer, FOCUSED_TERMINAL, onMachine(at.terminal, machine)],
  });
}

// The path of the reviewer `make install` linked on `machine`, or null once a toast said why
// there is none.
async function reviewerOn(machine) {
  let result;
  try {
    result = await exec("sh", ["-c", LINKED_REVIEWER], { on: machine, timeout: MACHINE_TIMEOUT_MS });
  } catch (error) {
    fail(`Cannot ask ${machine} for its reviewer: ${error?.message ?? error}`);
    return null;
  }
  const reviewer = result.stdout.trim();
  if (result.code === 0 && reviewer) return reviewer;
  const why = result.stderr.trim() || `exit ${result.code}`;
  fail(result.code === NOT_INSTALLED ? `No reviewer on ${machine}: run make install there.` : `Cannot ask ${machine} for its reviewer: ${why}`);
  return null;
}

export default class Launcher extends View {
  init(_props, cx) {
    // Shown elsewhere than as the key's overlay (`herdr-app check`), it does nothing.
    const overlay = props()?.overlay;
    if (!overlay) return;
    let closed = false;
    const close = () => {
      if (closed) return;
      closed = true;
      closeOverlay(overlay);
    };
    cx.spawn(async () => {
      try {
        await toggle(close);
      } finally {
        close();
      }
    });
  }

  render() {
    return div();
  }
}
