# Regression tests from vision sessions

Add UI regression cases here after reproducing them in a vision session. A test drives the
reviewer in a private Herdr (`PrivateHerdrServer`), never the user's.
Each test should identify the finding from a vision session that it protects, document the
expected behavior, and reproduce the relevant interactions through the real UI.
Keep supporting evidence with the test rather than relying on ignored artifacts
under `target/`.

See the [visual testing guide](../../../.agents/wiki/visual-testing.md)
for the vision MCP server's tools. Run the resulting tests with
`scripts/dev-shell make e2e-tui`.
