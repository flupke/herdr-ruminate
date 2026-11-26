import { appHome, focused, hc } from "herdr";
import { REVIEWER, fail, keyMod, machineOf, reviewTerminals, reviewerOn, run } from "./common.js";

const OPEN_PAGE = `${appHome()}/mods/herdr-progressive-reviewer-page/open-page.sh`;

const NO_PAGE = 3;

const OPEN_TIMEOUT_MS = 20000;

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

function controlOf(reviewer) {
  return `${reviewer.slice(0, reviewer.lastIndexOf("/"))}/reviewer-control`;
}

async function pageAddress(control, place, machine) {
  const result = await run(control, ["explore-page-address", ...place], { machine });
  const url = result.stdout.trim();
  if (result.code === 0 && url) return url;
  const where = machine ? ` on ${machine}` : "";
  const why = result.stderr.trim() || `exit ${result.code}`;
  if (result.code === NO_PAGE) fail("Wait for the review pane to start.", `No Explore page${where}`);
  else if (why.includes("usage:")) fail("Run make install in a checkout there.", `Update the reviewer${where}`);
  else fail(why, `Cannot find the Explore page${where}`);
  return null;
}

// ssh reads options after its destination too.
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
  }
  fail(result.stderr.trim() || `exit ${result.code}`, "Cannot list herdr-core's machines");
  return undefined;
}

// hc runs the `--via` command of a machine as an sh line.
function sshCommand(added) {
  if (added.via) {
    const program = added.target.trim().split(/\s+/)[0] ?? "";
    return program.slice(program.lastIndexOf("/") + 1) === "ssh" ? ["sh", "-c", `exec ${added.target} "$@"`, "sh"] : null;
  }
  const hostPort = /^((?:[^@]*@)?)(?:\[([^\]]+)\]|([^:]+)):(\d+)$/.exec(added.target);
  return hostPort ? ["ssh", "-p", hostPort[4], `${hostPort[1]}${hostPort[2] ?? hostPort[3]}`] : ["ssh", added.target];
}

// ssh runs its remote command in the remote user's shell.
function quoted(word) {
  return `'${word.replaceAll("'", "'\\''")}'`;
}

export default keyMod(openPage);
