---
name: explore-page-tools
description: "Tools for the Explore page: make explore-page, explore-script, explore-gallery, e2e-explore, explore-types, the README screenshots, and adding a screen to the page's client. Use when changing, testing or shooting the Explore page."
---

# Explore page tools

Run every command below in the dev shell (`scripts/dev-shell ...`). It provides Node, the
headless Chromium of nixpkgs (`E2E_CHROMIUM`) and `tsc`, and turns e2e's telemetry off. The
first run in a checkout installs the npm packages from the committed lockfile, which needs the
network (`scripts/dev-shell make e2e-explore-deps` does only that).

## Serve the page alone

```sh
scripts/dev-shell make explore-page
```

- It runs `crates/review-explore-page-server`, with no pane, no agent and no Herdr, and prints
  the address of a page that shows a fixed question: `http://127.0.0.1:8790/?token=dev`.
- The server reads the shell, the client's modules and the stylesheets from disk on every
  request, and an open page reloads when one changes: a client or style edit shows at once.
- A Rust edit needs a restart of the command. The open page reconnects by itself, and the token
  stays `dev`.
- The server's options are at the top of `crates/review-explore-page-server/src/main.rs`.
  `make explore-page EXPLORE_PAGE_ARGS='--data rich'` serves a round as long as a real one, with
  weeks of past rounds.

## Act on the page with a throwaway script

```sh
scripts/dev-shell make explore-script SCRIPT=path/to/script.ts
```

- It runs a Playwright script against a fresh session of the standalone server, in the
  headless Chromium.
- `make explore-script` without `SCRIPT` prints the header of `tests/explore-page/script.ts`:
  an example script, what it gets, how screenshots are named, what a failure prints, and the
  variables.
- Keep the script in a scratch folder outside the repository.
- Use it to see a state the gallery lacks, or to try a client behaviour before deciding whether
  it earns an e2e journey.

## Screenshot gallery

```sh
scripts/dev-shell make explore-gallery
```

- It shoots every state of the page at 1280 and 390 pixels wide, in the light and dark theme,
  with a contact sheet, `index.html`. It calls no model and is not part of `make check`.
- A state that fails to reach its page stops the run with its name.
- Images are named `<state>-<width>-<theme>.png`, so two runs compare file by file. Set
  `GALLERY_COMPARE` to an earlier gallery's folder to see before and after for each image that
  differs. The other variables are at the top of `tests/explore-page/gallery/gallery.ts`; the
  state names for `GALLERY_STATES` are the `name` fields of `tests/explore-page/gallery/states.ts`,
  and an unknown name stops the run.
- Two runs on the same code give the same files on any machine; the comparison is byte for
  byte, so a browser that draws differently also counts as a change.
- To add a state, add one entry to `tests/explore-page/gallery/states.ts` (its header says what
  an entry holds). Before an action the round no longer offers, hold the page with `refused` so
  it does not follow the round first. A new move of the round needs a control route in
  `crates/review-explore-page-server/src/control.rs` first.

## E2E tests

Also load the `e2e` skill: it says how to write e2e tests. Here its parts on agent steps,
models and the replay cache do not apply.

```sh
scripts/dev-shell make e2e-explore
```

- It checks the client's types, then runs every test at a desktop size against its own server,
  in about 10 seconds. `make e2e-explore E2E_ARGS=tests/round.e2e.ts` runs one file: `E2E_ARGS`
  go to `e2e run`, with paths relative to `tests/explore-page`.
- `tests/explore-page/run.sh` refuses to run outside the dev shell. Run every `npx e2e`
  command inside it too.
- The server's log is in `tests/explore-page/.e2e/logs/`; a failed run prints its end. A
  failing test leaves the page's accessibility tree and a Playwright trace under
  `tests/explore-page/.e2e/artifacts/`.
- The run also fails when the browser reports a breach of the page's content security policy
  (set in `crates/review-explore-page/src/page.rs`).

Write a test:

- The suite holds journeys: the reviewer's main paths through a round. Pick them to run the
  client's own logic (drafts kept across a reload, reconnection, the request each control
  sends, the chat's focus and unread count): the client has no unit tests.
- A rule the server enforces goes in a Rust test instead
  (`crates/review-explore-page/src/socket.tests.rs`, `page.tests.rs`, the tests of
  `crates/review-explore-session`). How the page draws a state is not tested: look at it with
  the gallery or `explore-script`.
- Exact steps only: each step a locator and an action
  (`screen.getByRole('button', 'Send answer').tap()`, `.check()`, `.fill()`), each outcome a
  locator and a matcher. The config names no model, so any `agent.*` call fails.
- Set the round up through the `explore` fixture of `tests/explore-page/tests/session.ts`; its
  `Session` interface documents each helper. A move no helper makes needs a control route in
  `crates/review-explore-page-server/src/control.rs` first.
- Check the facts that make the outcome, not every element: listing each choice and sentence
  breaks the test on every markup change.
- Locators use roles and accessible names, so the markup must name what a test looks for (a
  `<section>` labelled by its heading, a `<fieldset>` with a `<legend>`, `role="status"`).

Gotchas of e2e 0.16:

- A role locator whose name holds an apostrophe fails with `ENGINE_FAILURE`: match it with a
  pattern, `/^In the pane.s editor$/`.
- `count()` and the other reads do not wait: use a matcher for a value that settles.

Let an agent try locators before writing them: register e2e's MCP server once for Claude Code,
from the repository root, after `scripts/dev-shell make e2e-explore-deps`:

```sh
claude mcp add e2e -- "$PWD/tests/explore-page/mcp.sh"
```

Without it, drive the same server from the shell with `tests/explore-page/mcp-cli.mjs`, whose
header shows how. Each run starts a new page server; `/?token=e2e` opens its round.

When `tests/explore-page/package.json` moves to another e2e version, run `npx e2e init --yes` in
a scratch directory and copy its `.agents/skills/e2e` over this repository's. Take nothing else
that `init` writes: its example test needs a model.

## Add a screen or a region to the client

The client is plain JavaScript ES modules in `crates/review-explore-page/assets/client`, with
no build step. Keep its rendering rules:

- One state, one entry point: the latest view from the tool is the only model, and
  `render(view)` in `page.js` draws it. What only the page knows (a notice, the quiz item just
  answered or moved to) lives beside it and is never overwritten by a push.
- Stable regions: the page is a fixed list of regions, each marked by an anchor in its parent.
  A region is rebuilt only when its key changes, so the nodes the reviewer uses (a text box, a
  selection, an open fold) survive every push.
- Text from the data goes in with `textContent` (through `h`); HTML only through
  `setRenderedMarkdown`, for the agent's Markdown the tool rendered, and `setDiagramDrawing`,
  for the SVG Mermaid drew in strict mode. The agent's plain texts go through `codeSpans`.
- A text box's value is set only when it is built, from the reviewer's draft (`drafts.js`), never
  on a push. A rebuild that takes the focus from a text box gives it back to the text box of the
  same draft (or of the draft named by its `data-draft-from`), with its selection; one that takes
  it from a control with `data-focus-key` gives it back to the control of the same key.

1. Give the view the data it needs: a field of `PageView` or of its screen's type in
   `crates/review-explore-page/src/view.rs`, built from the round's snapshot.
2. Run `make explore-types`: it writes `assets/client/types.ts` from the Rust types. The file is
   committed, and a Rust test fails while it differs.
3. Write the module that builds the region. Name the types in its JSDoc
   (`/** @import { PageView } from "./types.ts" */`).
4. Add the region to `Page` in `page.js`.
5. List the module in `ASSETS` in `crates/review-explore-page/src/files.rs`, or the page cannot
   load it.
6. An action the region offers is a form with a `data-method`, an entry in `CALLS` in
   `actions.js`, a variant of `Call` in `src/rpc.rs`, and its check in `src/actions.rs`.
7. Run `make lint`: it runs `tsc` over the client
   (`tests/explore-page/tsconfig.client.json`), which fails on a field one side no longer has.

The page's content security policy allows scripts from the page itself only.

## README screenshots

The screenshots in `docs/screenshots/` all show the rich data set's change
(`crates/review-explore-page-server/src/rich/`), so the pane and the page tell one story.

`pane-review` (a file changed since review, with the agent's reply in its thread) and
`pane-threads` (the Threads tab), with the vision server (`visual-testing` skill):

- Replace the scratch repository's changes with two jj changes: the rich data set's `.old`
  files (`jj new 'root()'`), then its `.new` files on top (`jj new`).
- The scene: mark `CHANGELOG.md` and `flush.rs` reviewed; comment on the `notify_batch` line
  of `queue.rs` and mark it reviewed; comment on a line of `reply.rs`, left unanswered; then,
  as the agent, add a line to `queue.rs` and answer the first comment.
- Crop the reviewer's pane out of the 1600x1000 window, and cut its empty rows above the pane's
  bottom border.
- Write the agent's reply from a script, not a shell command line: the shell runs the
  backticks of its inline code.
- Look at the thread mark 💬 before the shot: how herdr-app draws it was not checked since the
  move to the app sandbox.

`explore-question` is the gallery state `question-1`, in a 1280x800 window:

- Shoot it with a throwaway `make explore-script` script: start a `StandaloneServer` with
  `--data rich --fixed-clock`, open a browser context with `deviceScaleFactor: 2` for each
  theme, run the state's `reach`, and screenshot the window, not the whole page.
- Park the pointer in a corner first: it can rest on the meter and open its window.
- Save a light and a dark file as lossless WebP (about 140 KB at 2x). The README picks one with
  `<picture>`.
