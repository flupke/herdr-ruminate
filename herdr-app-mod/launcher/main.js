import { appHome, focused, run as runCommand, terminals } from "herdr";
import { FOCUSED_TERMINAL, REVIEWER, fail, keyMod, machineOf, onMachine, reviewPanes, reviewerOn } from "./common.js";

// herdr-core starts the pane with its own environment, not the window's.
const HERDR_APP_HOME = "--herdr-app-home";

// Herdr's rule (`PaneSize::split_direction`): a cell is about twice as tall as it is wide.
const WIDE = 5 / 2;

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
  // herdr-app runs a split on the machine of the terminal it splits, whatever its space's.
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
  runCommand("pane.split", {
    pane: at.pane,
    direction: wide ? "right" : "down",
    argv: [reviewer, FOCUSED_TERMINAL, onMachine(at.terminal, machine)],
  });
}

export default keyMod(toggle);
