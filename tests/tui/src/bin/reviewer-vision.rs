//! The vision MCP server, on standard input and output. Register it with an agent (the
//! repository's `.mcp.json` does) and call its `start` tool.

use anyhow::Result;
use reviewer_tui_tests::vision::{Setup, serve};

fn main() -> Result<()> {
    if std::env::args()
        .nth(1)
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!(
            "reviewer-vision: the vision MCP server, on standard input and output.\n\nThe repository's .mcp.json registers it through scripts/vision-mcp, which gives it the\npinned herdr-app and hc of the dev shell: an agent that runs in the repository gets its\ntools. Their start tool runs the reviewer of this checkout in herdr-common's app sandbox, a\nprivate herdr-app on a headless display that never shows on your screen; screenshot shows\nit. See .agents/wiki/tui-vision.md."
        );
        return Ok(());
    }
    serve(Setup::from_env())
}
