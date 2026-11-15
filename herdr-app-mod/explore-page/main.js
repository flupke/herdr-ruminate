// The progressive reviewer's Explore page key (keys.json:
// `mod.open:herdr-progressive-reviewer-page`): it opens the Explore page of the review pane of
// the focused tab in this computer's browser. `make install` writes it here from
// `herdr-app-mod/explore-page/` of the reviewer's checkout.
//
// A review pane of another machine serves its page on that machine's loopback interface, which
// only accepts its own address as the request's host: the key forwards the same port of this
// computer's loopback interface to it over ssh, for as long as the page answers there and
// herdr-app runs here (`open-page.sh`), then opens `http://127.0.0.1:<port>/...` here. A machine
// herdr-core reaches through another command than ssh gets no forward.

import { appHome, focused, hc } from "herdr";
import { REVIEWER, fail, keyMod, machineOf, reviewTerminals, reviewerOn, run } from "./common.js";

// The script that forwards a page's port and opens the browser, beside this file.
const OPEN_PAGE = `${appHome()}/mods/herdr-progressive-reviewer-page/open-page.sh`;

// The exit status of `reviewer-control explore-page-address` when no reviewer serves a page.
const NO_PAGE = 3;

// How long the script may take to forward the port and start the browser.
const OPEN_TIMEOUT_MS = 20000;

// Opens the page of the focused tab's review pane. The key's overlay stays until the script
// answered, so that its failure still shows (`keyMod`).
async function openPage() {
  const at = focused();
  if (!at?.tab) return;
  const review = reviewTerminals(at.tab)[0];
  if (!review) return fail("Open one with its key first.", "No review pane in this tab");
  const machine = machineOf(review.address);
  const place = [review.labels["herdr.space"], review.labels["herdr.tab"]];
  const reviewer = machine ? await reviewerOn(machine) : REVIEWER;
  if (!reviewer) return;
  const url = await pageAddress(controlOf(reviewer), place, machine);
  if (!url) return;
  const forward = machine ? await sshForward(machine, url, controlOf(reviewer)) : [];
  if (!forward) return;
  const result = await run("bash", [OPEN_PAGE, controlOf(REVIEWER), url, ...forward], { timeout: OPEN_TIMEOUT_MS });
  if (result.code !== 0) fail(result.stderr.trim() || `exit ${result.code}`, "Cannot open the Explore page");
}

// The `reviewer-control` beside `reviewer`, which `make build` writes in the same folder.
function controlOf(reviewer) {
  return `${reviewer.slice(0, reviewer.lastIndexOf("/"))}/reviewer-control`;
}

// The address of the page the review pane at `place` serves on `machine`, or null once a toast
// said why there is none.
async function pageAddress(control, place, machine) {
  const result = await run(control, ["explore-page-address", ...place], { machine });
  const url = result.stdout.trim();
  if (result.code === 0 && url) return url;
  const where = machine ? ` on ${machine}` : "";
  const why = result.stderr.trim() || `exit ${result.code}`;
  if (result.code === NO_PAGE) fail("Wait for the review pane to start.", `No Explore page${where}`);
  // An older reviewer there knows no explore-page-address, and prints its usage.
  else if (why.includes("usage:")) fail("Run make install in a checkout there.", `Update the reviewer${where}`);
  else fail(why, `Cannot find the Explore page${where}`);
  return null;
}

// What `open-page.sh` takes to forward the page at `url` from `machine` while it answers there:
// the machine, the port, and the ssh command with its options and its remote command; or null
// once a toast said why there is none. `machine` was added over ssh, or through a command that
// runs ssh, which reads options after its destination too.
async function sshForward(machine, url, control) {
  const port = /^http:\/\/127\.0\.0\.1:(\d+)\//.exec(url)?.[1];
  if (!port) {
    fail(url, `Cannot forward the Explore page from ${machine}`);
    return null;
  }
  const added = await listed(machine);
  if (added === undefined) return null;
  const ssh = added && sshCommand(added);
  if (!ssh) {
    fail("Share it over a tunnel: O in the review pane.", `${machine} is not reached over ssh`);
    return null;
  }
  const forward = [
    "-o", "ExitOnForwardFailure=yes",
    "-o", "BatchMode=yes",
    "-L", `127.0.0.1:${port}:127.0.0.1:${port}`,
    [control, "explore-page-wait", url].map(quoted).join(" "),
  ];
  return [machine, port, ...ssh, ...forward];
}

// The machine `machine` as `hc machine ls` lists it, null when it lists none of that name, or
// undefined once a toast said why the list failed.
async function listed(machine) {
  let result;
  try {
    result = await hc(["machine", "ls", "--json"]);
  } catch (error) {
    result = { code: -1, stdout: "", stderr: String(error?.message ?? error) };
  }
  try {
    if (result.code === 0) return JSON.parse(result.stdout).machines?.find((m) => m.name === machine) ?? null;
  } catch {
    // Not JSON: reported below.
  }
  fail(result.stderr.trim() || `exit ${result.code}`, "Cannot list herdr-core's machines");
  return undefined;
}

// The start of the ssh command that reaches the machine `added` of `hc machine ls`, to which
// options and a remote command may follow: for a machine added over ssh, its target (`host`,
// `user@host:2222`, `user@[::1]:2222`); for one added `--via` a command whose first word runs
// ssh, that command whole, as the sh line hc runs it as, which execs ssh so that the forward's
// watcher ends ssh itself; null for another command.
function sshCommand(added) {
  if (added.via) {
    const program = added.target.trim().split(/\s+/)[0] ?? "";
    return program.slice(program.lastIndexOf("/") + 1) === "ssh" ? ["sh", "-c", `exec ${added.target} "$@"`, "sh"] : null;
  }
  const hostPort = /^((?:[^@]*@)?)(?:\[([^\]]+)\]|([^:]+)):(\d+)$/.exec(added.target);
  return hostPort ? ["ssh", "-p", hostPort[4], `${hostPort[1]}${hostPort[2] ?? hostPort[3]}`] : ["ssh", added.target];
}

// `word` quoted for the remote shell ssh runs its command in.
function quoted(word) {
  return `'${word.replaceAll("'", "'\\''")}'`;
}

export default keyMod(openPage);
