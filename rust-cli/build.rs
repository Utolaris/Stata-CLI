use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let repo_root = Path::new(&manifest_dir)
        .parent()
        .expect("rust-cli must live one level below the repo root");
    println!("cargo:rustc-env=STATACLI_REPO_ROOT={}", repo_root.display());
}
