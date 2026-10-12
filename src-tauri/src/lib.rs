mod commands;
mod mcp;
pub mod shelf;

/// `repo-shelf mcp`: serve the library to an agent over stdio.
pub fn run_mcp() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    if let Err(e) = rt.block_on(mcp::serve()) {
        eprintln!("repo-shelf mcp: {e:#}");
        std::process::exit(1);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shelf = shelf::Shelf::open_default().expect("could not open the repo-shelf library");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(shelf)
        .invoke_handler(tauri::generate_handler![
            commands::list_categories,
            commands::list_repos,
            commands::add_repo,
            commands::set_note,
            commands::move_repo,
            commands::delete_repo,
            commands::add_category,
            commands::delete_category,
            commands::has_token,
            commands::set_token,
            commands::connect_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
