//! The app's entry point. Run as ssh's askpass helper when ssh starts it for one (askpass.rs); otherwise start the app
//! (`superxterminal_lib::run` in lib.rs).

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Answer ssh's question as its askpass helper, or start the app
fn main() {
    if superxterminal_lib::askpass_helper() {
        return;
    }
    superxterminal_lib::run()
}
