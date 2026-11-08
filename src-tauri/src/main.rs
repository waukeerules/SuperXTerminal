//! The app's entry point. Run as ssh's askpass helper when ssh starts it for one (askpass.rs), or as a stand-in device
//! when demo mode starts it for a session (demo_device.rs); otherwise start the app (`superxterminal_lib::run` in
//! lib.rs).

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Answer ssh's question as its askpass helper, act as a demo's stand-in device, or start the app
fn main() {
    if superxterminal_lib::askpass_helper() || superxterminal_lib::demo_device() {
        return;
    }
    superxterminal_lib::run()
}
