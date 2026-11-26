// A herdr-app mod imports nothing from outside its own folder.

import { View, div } from "gpui-kit";
import { closeOverlay, exec, notify, panes, props, terminals } from "herdr";

export const REVIEWER = "@REVIEWER@";

export const FOCUSED_TERMINAL = "--focused-terminal";

// herdr-core's environment may lack the `XDG_DATA_HOME` of the shell that ran `make install`, or set another.
const NOT_INSTALLED = 3;
const LINKED_REVIEWER = `for data in "\${XDG_DATA_HOME:-}" "$HOME/.local/share"; do link="$data/herdr-progressive-reviewer/reviewer"; [ -n "$data" ] && [ -x "$link" ] && exec readlink -f "$link"; done; exit ${NOT_INSTALLED}`;

export const MACHINE_TIMEOUT_MS = 5000;

function isReview(terminal) {
  return terminal.state === "live" && Boolean(terminal.argv?.[0]?.endsWith("/bin/reviewer")) && terminal.argv[1] === FOCUSED_TERMINAL;
}

// herdr-app gives another machine's ids a `<machine>/` prefix, which the core there, and the
// labels of its terminals, lack.
export function onMachine(id, machine) {
  const prefix = `${machine}/`;
  return machine && id.startsWith(prefix) ? id.slice(prefix.length) : id;
}

export function machineOf(address) {
  const slash = address.indexOf("/");
  return slash < 0 ? null : address.slice(0, slash);
}

export function reviewTerminals(tab) {
  return terminals().filter((t) => isReview(t) && t.labels?.["herdr.tab"] === onMachine(tab, machineOf(t.address)));
}

export function reviewPanes(tab) {
  const reviews = new Set(reviewTerminals(tab).map((t) => t.address));
  return panes().filter((p) => reviews.has(p.terminal));
}

// A herdr-app toast shows its body on one line of about 45 characters.
export function fail(body, title = "Progressive reviewer") {
  notify({ title, body, level: "error" });
}

export async function run(program, args, { machine = null, timeout = MACHINE_TIMEOUT_MS } = {}) {
  try {
    return await exec(program, args, { on: machine, timeout });
  } catch (error) {
    return { code: -1, stdout: "", stderr: String(error?.message ?? error) };
  }
}

export async function reviewerOn(machine) {
  const result = await run("sh", ["-c", LINKED_REVIEWER], { machine });
  const reviewer = result.stdout.trim();
  if (result.code === 0 && reviewer) return reviewer;
  if (result.code === NOT_INSTALLED) fail("Run make install in a checkout there.", `No reviewer on ${machine}`);
  else fail(result.stderr.trim() || `exit ${result.code}`, `Cannot ask ${machine} for its reviewer`);
  return null;
}

// herdr-app: a mod's tasks live only while its overlay shows, so a promise awaited after the
// close never resumes. Unlike a popup, an overlay takes no key and no click closes it.
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
