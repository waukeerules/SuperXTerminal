//! Tauri's build step: the config, the capabilities and, on Windows, the program's icon and version resource.
//!
//! The icons folder is watched too. Tauri's own step only reruns when the config changes, and `generate_context!`
//! embeds the window's icon only when the crate recompiles, so a new icon.ico (scripts/windows-icon.py) kept the old
//! one on the taskbar until something else changed. Rerunning this recompiles the crate as well.
//!
//! THIRD-PARTY-NOTICES.txt isn't in git: each build writes its own system's (`npm run notices`, before every
//! `tauri build`). Tauri's step stops when a bundled resource is missing, so in a fresh copy, before anything has
//! written it, `cargo build` and `cargo test` failed. [`notices_placeholder`] writes a short stand-in then.
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=icons");
    notices_placeholder(Path::new("../THIRD-PARTY-NOTICES.txt"));
    tauri_build::build()
}

/// Writes a stand-in THIRD-PARTY-NOTICES.txt at `path` when there's none, saying how to make the real one. A file
/// that's there, the real one or an earlier stand-in, is left alone. Release builds never ship the stand-in, as
/// `npm run notices` writes the real file before them.
fn notices_placeholder(path: &Path) {
    if path.exists() {
        return;
    }
    let text = "SuperXTerminal's third-party notices haven't been made in this copy of the source.\n\
                Run `npm run notices` to make them; every release build does that first.\n";
    if let Err(e) = std::fs::write(path, text) {
        println!("cargo:warning=couldn't write {}: {e}", path.display());
    }
}
