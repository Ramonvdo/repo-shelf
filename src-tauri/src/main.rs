// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        repo_shelf_lib::run_mcp()
    } else {
        repo_shelf_lib::run()
    }
}
