//! Tauri commands: the window's thin bridge onto the shared `Shelf`.

use serde::Serialize;
use tauri::State;

use crate::shelf::Shelf;
use crate::shelf::store::{Category, Repo};

type Res<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[tauri::command]
pub fn list_categories(shelf: State<'_, Shelf>) -> Res<Vec<Category>> {
    shelf.with_store(|s| s.categories()).map_err(err)
}

#[tauri::command]
pub fn list_repos(shelf: State<'_, Shelf>, category: Option<String>) -> Res<Vec<Repo>> {
    shelf.with_store(|s| s.list_repos(category.as_deref())).map_err(err)
}

#[tauri::command]
pub async fn add_repo(shelf: State<'_, Shelf>, link: String, category: String) -> Res<Repo> {
    shelf.add(&link, &category).await.map_err(err)
}

#[tauri::command]
pub fn set_note(shelf: State<'_, Shelf>, id: i64, note: String) -> Res<()> {
    shelf.with_store(|s| s.set_note(id, &note)).map_err(err)
}

#[tauri::command]
pub fn move_repo(shelf: State<'_, Shelf>, id: i64, category: String) -> Res<()> {
    shelf.with_store(|s| s.move_repo(id, &category)).map_err(err)
}

#[tauri::command]
pub fn delete_repo(shelf: State<'_, Shelf>, id: i64) -> Res<()> {
    shelf.with_store(|s| s.delete_repo(id)).map_err(err)
}

#[tauri::command]
pub fn add_category(shelf: State<'_, Shelf>, name: String) -> Res<()> {
    shelf.with_store(|s| s.add_category(&name)).map_err(err)
}

#[tauri::command]
pub fn delete_category(shelf: State<'_, Shelf>, name: String) -> Res<()> {
    shelf.with_store(|s| s.delete_category(&name)).map_err(err)
}

#[tauri::command]
pub fn has_token(shelf: State<'_, Shelf>) -> Res<bool> {
    shelf
        .with_store(|s| s.setting("github_token"))
        .map(|t| t.is_some_and(|t| !t.is_empty()))
        .map_err(err)
}

#[tauri::command]
pub fn set_token(shelf: State<'_, Shelf>, token: String) -> Res<()> {
    shelf.with_store(|s| s.set_setting("github_token", token.trim())).map_err(err)
}

#[derive(Serialize)]
pub struct ConnectInfo {
    /// Absolute path of this executable; agents launch it with the `mcp` argument.
    exe: String,
}

#[tauri::command]
pub fn connect_info() -> Res<ConnectInfo> {
    // An AppImage runs from a temporary mount; agents need the AppImage file itself.
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        return Ok(ConnectInfo { exe: appimage });
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(ConnectInfo { exe: exe.display().to_string() })
}
