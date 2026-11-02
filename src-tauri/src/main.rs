// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if superxterminal_lib::askpass_helper() {
        return;
    }
    superxterminal_lib::run()
}
