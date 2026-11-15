// What the reviewer's two herdr-app mods share, the launcher (`launcher/`) and the Explore page
// key (`explore-page/`): a mod that acts once on its key, the review panes of a tab, the machines
// they run on, the reviewer `make install` linked on another machine, and toasts.
// `scripts/herdr-app-mod` copies this file into each mod's folder, since a mod imports nothing
// from outside its own, with the path of the checkout's reviewer in place of `@REVIEWER@`.

import { View, div } from "gpui-kit";
import { closeOverlay, exec, notify, panes, props, terminals } from "herdr";

// The review pane's program on this computer.
export const REVIEWER = "@REVIEWER@";

// The option the review pane's program takes the focused terminal with: with a program ending
// in `/bin/reviewer`, it tells a review pane apart, whichever checkout installed it.
export const FOCUSED_TERMINAL = "--focused-terminal";

// A script that prints the path of the reviewer `make install` linked on another machine, run
// there in its home with its core's environment, or exits with NOT_INSTALLED when there is none:
// the link is `herdr-progressive-reviewer/reviewer` in the XDG data folder of the shell that ran
// `make install`, which may set an `XDG_DATA_HOME` the core lacks, or lack one the core has.
const NOT_INSTALLED = 3;
const LINKED_REVIEWER = `for data in "\${XDG_DATA_HOME:-}" "$HOME/.local/share"; do link="$data/herdr-progressive-reviewer/reviewer"; [ -n "$data" ] && [ -x "$link" ] && exec readlink -f "$link"; done; exit ${NOT_INSTALLED}`;

// How long a key waits for another machine to answer.
export const MACHINE_TIMEOUT_MS = 5000;

// Whether `terminal` runs a review pane.
function isReview(terminal) {
  return terminal.state === "live" && Boolean(terminal.argv?.[0]?.endsWith("/bin/reviewer")) && terminal.argv[1] === FOCUSED_TERMINAL;
}

// `id` of a space, a tab or a terminal on `machine` (null for this computer) as that machine's
// core names it: the window gives another machine's ids a `<machine>/` prefix, which the core
// there, and the labels of its terminals, lack.
export function onMachine(id, machine) {
  const prefix = `${machine}/`;
  return machine && id.startsWith(prefix) ? id.slice(prefix.length) : id;
}

// The machine a terminal runs on, by its address (`<machine>/t7`): null for this computer.
export function machineOf(address) {
  const slash = address.indexOf("/");
  return slash < 0 ? null : address.slice(0, slash);
}

// The terminals of the review panes in `tab`. A review pane's terminal runs on the machine of
// the agent it opened beside, which may not be its space's: its `herdr.tab` label names the tab
// as that terminal's machine does, without the prefix of its own machine.
export function reviewTerminals(tab) {
  return terminals().filter((t) => isReview(t) && t.labels?.["herdr.tab"] === onMachine(tab, machineOf(t.address)));
}

// The panes showing a review pane in `tab`.
export function reviewPanes(tab) {
  const reviews = new Set(reviewTerminals(tab).map((t) => t.address));
  return panes().filter((p) => reviews.has(p.terminal));
}

// A toast of what went wrong: its body shows on one line of about 45 characters, so a longer
// message puts what matters in its `title`.
export function fail(body, title = "Progressive reviewer") {
  notify({ title, body, level: "error" });
}

// Runs `program` with `args` on `machine`, or on this computer, for at most `timeout` ms; a
// program that cannot start comes back as a result with code -1 and the reason on stderr.
export async function run(program, args, { machine = null, timeout = MACHINE_TIMEOUT_MS } = {}) {
  try {
    return await exec(program, args, { on: machine, timeout });
  } catch (error) {
    return { code: -1, stdout: "", stderr: String(error?.message ?? error) };
  }
}

// The path of the reviewer `make install` linked on `machine`, or null once a toast said why
// there is none.
export async function reviewerOn(machine) {
  const result = await run("sh", ["-c", LINKED_REVIEWER], { machine });
  const reviewer = result.stdout.trim();
  if (result.code === 0 && reviewer) return reviewer;
  if (result.code === NOT_INSTALLED) fail("Run make install in a checkout there.", `No reviewer on ${machine}`);
  else fail(result.stderr.trim() || `exit ${result.code}`, `Cannot ask ${machine} for its reviewer`);
  return null;
}

// The mod a key opens as its overlay, which runs `act` once with a function that closes the
// overlay, and closes it after `act` in any case. The mod's copy, and its tasks, live only while
// the overlay shows, so a promise awaited after the close never resumes: `act` closes it only
// once it has nothing left to report. The overlay is a window of 2×2 pixels that shows nothing:
// unlike a popup, it takes no key and no click closes it meanwhile (`mod.json`). Shown elsewhere
// than as the key's overlay (`herdr-app check`), the mod does nothing.
export function keyMod(act) {
  return class extends View {
    init(_props, cx) {
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
          await act(close);
        } finally {
          close();
        }
      });
    }

    render() {
      return div();
    }
  };
}
