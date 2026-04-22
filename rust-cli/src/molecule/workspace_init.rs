use crate::atom::boilerplate_copy::copy_tree;
use crate::atom::path_ops::boilerplate_dir;
use anyhow::{bail, Result};
use serde_json::json;
use std::env;
use std::path::Path;

pub(crate) fn init_command(repo_root: &Path) -> Result<()> {
    let source = boilerplate_dir(repo_root);
    if !source.exists() {
        bail!("Boilerplate directory not found at {}", source.display());
    }

    let target_dir = env::current_dir()?;
    copy_tree(&source, &target_dir)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "success",
            "target_dir": target_dir,
            "source_dir": source,
            "message": format!("Copied boilerplate from {} into {}", source.display(), target_dir.display()),
        }))?
    );
    Ok(())
}
