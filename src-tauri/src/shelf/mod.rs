//! The library itself: repos on disk plus remote reads. Shared by the window and the MCP server.

pub mod github;
pub mod store;
pub mod url;

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use github::GitHub;
use store::{Repo, Store};

/// Longest file returned to an agent, in characters (roughly 12k tokens).
const MAX_FILE_CHARS: usize = 50_000;
const TREE_DEPTH: usize = 2;
const TREE_LINES: usize = 400;

pub struct Shelf {
    store: Mutex<Store>,
    cache_dir: PathBuf,
}

impl Shelf {
    /// `REPO_SHELF_DIR` overrides the default app-data folder (used by tests and smoke runs).
    pub fn open_default() -> Result<Self> {
        let dir = match std::env::var_os("REPO_SHELF_DIR") {
            Some(d) => PathBuf::from(d),
            None => dirs::data_dir()
                .ok_or_else(|| anyhow!("no app-data folder on this system"))?
                .join("repo-shelf"),
        };
        Self::open(&dir)
    }

    pub fn open(dir: &Path) -> Result<Self> {
        Ok(Self {
            store: Mutex::new(Store::open(&dir.join("shelf.db"))?),
            cache_dir: dir.join("cache"),
        })
    }

    pub fn with_store<T>(&self, f: impl FnOnce(&Store) -> Result<T>) -> Result<T> {
        let store = self.store.lock().map_err(|_| anyhow!("library lock poisoned"))?;
        f(&store)
    }

    /// Built per call so a token saved in the window applies to a running MCP server.
    fn github(&self) -> Result<GitHub> {
        let token = self.with_store(|s| s.setting("github_token"))?;
        Ok(GitHub::new(token, self.cache_dir.clone()))
    }

    fn repo_on_shelf(&self, full_name: &str) -> Result<Repo> {
        self.with_store(|s| s.find_repo(full_name))?.ok_or_else(|| {
            anyhow!("{full_name} is not on the shelf. Use list_repos or search_shelf to see what is.")
        })
    }

    /// Parses a link, fetches the repo's description and topics, and files it under `category`.
    pub async fn add(&self, link: &str, category: &str) -> Result<Repo> {
        let repo = url::parse_github_url(link).map_err(|e| anyhow!(e))?;
        if let Some(existing) = self.with_store(|s| s.find_repo(&repo.full_name()))? {
            return Err(anyhow!(
                "{} is already on the shelf in {}",
                existing.full_name,
                existing.category
            ));
        }
        let meta = self.github()?.repo_meta(&repo).await?;
        self.with_store(|s| s.add_repo(&repo, category, &meta))
    }

    pub async fn tree(&self, full_name: &str, path: &str) -> Result<String> {
        let repo = self.repo_on_shelf(full_name)?;
        let tree = self.github()?.tree(&repo.full_name, "HEAD").await?;
        let mut out = format!("{} @ {}\n", repo.full_name, &tree.sha[..7.min(tree.sha.len())]);
        out.push_str(&github::render_tree(&tree.tree, path, TREE_DEPTH, TREE_LINES));
        if tree.truncated {
            out.push_str("(GitHub truncated this tree; pass a path to see deeper folders.)\n");
        }
        Ok(out)
    }

    pub async fn read_file(&self, full_name: &str, path: &str) -> Result<String> {
        let repo = self.repo_on_shelf(full_name)?;
        let gh = self.github()?;
        // The trees endpoint resolves HEAD to a commit sha, so cached files never go stale.
        let sha = gh.tree(&repo.full_name, "HEAD").await?.sha;
        let bytes = gh.file(&repo.full_name, &sha, path).await?;
        github::cap_file(&bytes, MAX_FILE_CHARS)
    }

    pub async fn search_code(&self, full_name: &str, query: &str) -> Result<String> {
        let repo = self.repo_on_shelf(full_name)?;
        let paths = self.github()?.search_code(&repo.full_name, query).await?;
        Ok(if paths.is_empty() {
            format!("No matches for {query:?} in {}.", repo.full_name)
        } else {
            paths.join("\n")
        })
    }
}

/// One repo as an agent reads it in a list.
pub fn describe(r: &Repo) -> String {
    let mut line = format!("- {} ({} stars)", r.full_name, r.stars);
    if let Some(d) = r.description.as_deref().filter(|d| !d.is_empty()) {
        line.push_str(&format!(": {d}"));
    }
    if !r.topics.is_empty() {
        line.push_str(&format!(" [topics: {}]", r.topics.join(", ")));
    }
    if !r.note.is_empty() {
        line.push_str(&format!("\n  Note: {}", r.note));
    }
    line
}
