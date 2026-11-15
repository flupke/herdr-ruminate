// The progressive reviewer's launcher: its key (keys.json: `mod.open:herdr-progressive-reviewer`)
// opens this mod as an overlay, which toggles the review pane of the focused tab and closes at
// once. The review pane opens beside the focused terminal, in its folder, right of it when the
// terminal is wide and below it otherwise; the terminal is the first agent it reviews for.
// `make install` writes it here from `herdr-app-mod/launcher/` of the reviewer's checkout.
//
// Beside an agent of another machine, the review pane runs there, with the agent's repository
// and herdr-core: the reviewer that `make install` linked on that machine.

import { appHome, focused, run as runCommand, terminals } from "herdr";
import { FOCUSED_TERMINAL, REVIEWER, fail, keyMod, machineOf, onMachine, reviewPanes, reviewerOn } from "./common.js";

// The option that names this window's home, whose `view.json` the review pane follows the focus
// in: herdr-core starts the pane with its own environment, not the window's.
const HERDR_APP_HOME = "--herdr-app-home";

// A terminal at least this many times as many columns as rows splits right, else down: a
// cell is about twice as tall as it is wide (Herdr's rule, `PaneSize::split_direction`).
const WIDE = 5 / 2;

// Closes the review panes of the focused tab, or opens one beside the focused terminal; `close`
// closes the key's overlay (`keyMod`).
async function toggle(close) {
  const at = focused();
  if (!at?.tab) return;
  const open = reviewPanes(at.tab);
  if (open.length) {
    close();
    return open.forEach((p) => runCommand("pane.close", { pane: p.pane }));
  }
  if (!at.terminal || !at.pane) return fail("Focus your agent's terminal, then press the key again.");
  const terminal = terminals().find((t) => t.address === at.terminal);
  const wide = terminal && terminal.cols >= WIDE * terminal.rows;
  // A split runs on the machine of the terminal it splits, whatever its space's.
  const machine = machineOf(at.terminal);
  if (!machine) {
    close();
    return runCommand("terminal.open", {
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
  runCommand("pane.split", {
    pane: at.pane,
    direction: wide ? "right" : "down",
    argv: [reviewer, FOCUSED_TERMINAL, onMachine(at.terminal, machine)],
  });
}

export default keyMod(toggle);
