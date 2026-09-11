use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dist = manifest_dir.join("../dashboard/dist");
    let index = dist.join("index.html");

    if !index.is_file() {
        panic!(
            "dashboard/dist/index.html not found.\n\
             Build the OSS dashboard first:\n\
               cd dashboard && npm install && npm run build\n\
             Or from the repo root: make frontend"
        );
    }

    emit_rerun_if_changed_recursive(&dist);
    println!("cargo:rerun-if-changed={}", dist.display());
}

fn emit_rerun_if_changed_recursive(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            emit_rerun_if_changed_recursive(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
