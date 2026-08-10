//! Distribution contract tests: the skill package layout must stay in sync
//! across every platform build script.

use std::fs;
use std::path::Path;

#[test]
fn windows_builders_target_skill_bin() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust-cli should live under the repo root");
    for script in [
        "scripts/build_windows_bin.sh",
        "scripts/build_windows_bin.ps1",
    ] {
        let content = fs::read_to_string(repo.join(script))
            .unwrap_or_else(|error| panic!("read {script}: {error}"));
        assert!(
            content.contains("skill/stata-cli/bin"),
            "{script} must target skill/stata-cli/bin"
        );
    }
}
