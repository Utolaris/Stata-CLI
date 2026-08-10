//! Architecture contract tests: mechanically enforce the layering rules
//! documented in AGENTS.md (unsafe confined to the engine atom, no lateral
//! molecule-to-molecule imports).

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

fn source_files() -> Vec<PathBuf> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn unsafe_is_confined_to_stata_engine() {
    let pattern = Regex::new(r"\bunsafe\s*(?:\{|fn|impl|trait|extern)").unwrap();
    let engine_file = Path::new("atom").join("stata_engine.rs");
    for file in source_files() {
        if file.ends_with(&engine_file) {
            continue;
        }
        let content = fs::read_to_string(&file).unwrap();
        for (index, line) in content.lines().enumerate() {
            assert!(
                !pattern.is_match(line),
                "unsafe usage outside stata_engine.rs at {}:{}: {}",
                file.display(),
                index + 1,
                line.trim()
            );
        }
    }
}

#[test]
fn molecules_do_not_import_molecules() {
    let molecule_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("molecule");
    let pattern = Regex::new(r"use\s+crate::molecule::").unwrap();
    for file in source_files() {
        if !file.starts_with(&molecule_dir) {
            continue;
        }
        let content = fs::read_to_string(&file).unwrap();
        for (index, line) in content.lines().enumerate() {
            assert!(
                !pattern.is_match(line),
                "lateral molecule import at {}:{}: {}",
                file.display(),
                index + 1,
                line.trim()
            );
        }
    }
}
