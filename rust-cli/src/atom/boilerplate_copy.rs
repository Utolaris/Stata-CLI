use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub(crate) fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::metadata(source)
        .with_context(|| format!("Failed to read boilerplate source {}", source.display()))?;
    if !metadata.is_dir() {
        anyhow::bail!(
            "Boilerplate source is not a directory: {}",
            source.display()
        );
    }

    fs::create_dir_all(destination).with_context(|| {
        format!(
            "Failed to create boilerplate destination {}",
            destination.display()
        )
    })?;

    for entry in fs::read_dir(source)
        .with_context(|| format!("Failed to read directory {}", source.display()))?
    {
        let entry = entry
            .with_context(|| format!("Failed to enumerate entries inside {}", source.display()))?;
        let entry_path = entry.path();
        let target_path = destination.join(entry.file_name());
        let entry_metadata = entry
            .metadata()
            .with_context(|| format!("Failed to read metadata for {}", entry_path.display()))?;

        if entry_metadata.is_dir() {
            copy_tree(&entry_path, &target_path)?;
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create parent directory {}", parent.display())
            })?;
        }
        fs::copy(&entry_path, &target_path).with_context(|| {
            format!(
                "Failed to copy boilerplate file {} to {}",
                entry_path.display(),
                target_path.display()
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::copy_tree;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn copy_tree_copies_nested_files_and_overwrites_existing_targets() {
        let source_root = tempdir().unwrap();
        let destination_root = tempdir().unwrap();
        let source = source_root.path().join("boilerplate");
        let destination = destination_root.path().join("workspace");

        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("AGENTS.md"), "source agents\n").unwrap();
        fs::write(source.join("nested").join("analysis.do"), "display 1+1\n").unwrap();

        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("AGENTS.md"), "old agents\n").unwrap();

        copy_tree(&source, &destination).unwrap();

        assert_eq!(
            fs::read_to_string(destination.join("AGENTS.md")).unwrap(),
            "source agents\n"
        );
        assert_eq!(
            fs::read_to_string(destination.join("nested").join("analysis.do")).unwrap(),
            "display 1+1\n"
        );
    }
}
