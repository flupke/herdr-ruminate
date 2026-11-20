//! The tests of the plugin's hooks module (`plugin/hooks/register.test.ts`), which Claude Code's own
//! test kit runs: `claude plugin test`, with the `claude` the user installed.

use std::path::Path;
use std::process::Command;

#[test]
fn the_hooks_module_passes_its_tests_in_claude_code_s_test_kit() {
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugin");
    let output = Command::new("claude")
        .args(["plugin", "test", "."])
        .current_dir(&plugin)
        .output()
        .expect("the hooks module's tests need Claude Code's `claude` on the PATH");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{text}");
    assert!(text.contains(" 0 fail"), "{text}");
}
