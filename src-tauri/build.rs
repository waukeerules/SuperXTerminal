//! Tauri's build step: the config, the capabilities and, on Windows, the program's icon and version resource.
//!
//! The icons folder is watched too. Tauri's own step only reruns when the config changes, and `generate_context!`
//! embeds the window's icon only when the crate recompiles, so a new icon.ico (scripts/windows-icon.py) kept the old
//! one on the taskbar until something else changed. Rerunning this recompiles the crate as well.
fn main() {
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
