use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::shelf::{Shelf, describe};


#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddRepoArgs {
    /// GitHub link or owner/name.
    pub link: String,
    /// Category to file it under; created if new.
    pub category: String,
    /// Optional reminder of why this repo is useful.
    pub note: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListReposArgs {
    /// Category name. Omit to list every repo.
    pub category: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchShelfArgs {
    /// Words to match against repo names, descriptions, topics, notes and categories.
    pub query: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TreeArgs {
    /// Repo as owner/name, e.g. shadcn-ui/ui.
    pub repo: String,
    /// Folder to list. Omit for the repo root.
    pub path: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReadFileArgs {
    /// Repo as owner/name.
    pub repo: String,
    /// File path inside the repo, e.g. apps/www/components/ui/dialog.tsx.
    pub path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchCodeArgs {
    /// Repo as owner/name.
    pub repo: String,
    /// Code search terms.
    pub query: String,
}

#[derive(Clone)]
pub struct ShelfServer {
    shelf: Arc<Shelf>,
}

fn text(r: anyhow::Result<String>) -> Result<String, String> {
    r.map_err(|e| format!("{e:#}"))
}

#[tool_router]
impl ShelfServer {
    pub fn new(shelf: Shelf) -> Self {
        Self { shelf: Arc::new(shelf) }
    }

    #[tool(description = "List the shelf's categories with repo counts.")]
    fn list_categories(&self) -> Result<String, String> {
        text(self.shelf.with_store(|s| s.categories()).map(|cats| {
            if cats.is_empty() {
                return "The shelf is empty. The user adds repos in the repo-shelf app.".into();
            }
            cats.iter()
                .map(|c| format!("- {} ({})", c.name, c.count))
                .collect::<Vec<_>>()
                .join("\n")
        }))
    }

    #[tool(
        description = "List repos on the shelf, optionally only one category, with descriptions and the user's notes."
    )]
    fn list_repos(&self, Parameters(args): Parameters<ListReposArgs>) -> Result<String, String> {
        text(self.shelf.with_store(|s| s.list_repos(args.category.as_deref())).map(|mut repos| {
            repos.sort_by_key(|r| r.category.to_lowercase());
            if repos.is_empty() {
                return "No repos found. Call list_categories to see valid names.".into();
            }
            let mut out = String::new();
            let mut current = "";
            for r in &repos {
                if r.category != current {
                    out.push_str(&format!("\n## {}\n", r.category));
                    current = &r.category;
                }
                out.push_str(&describe(r));
                out.push('\n');
            }
            out.trim_start().to_string()
        }))
    }

    #[tool(
        description = "Find shelf repos whose name, description, topics, note or category contain all the given words."
    )]
    fn search_shelf(&self, Parameters(args): Parameters<SearchShelfArgs>) -> Result<String, String> {
        text(self.shelf.with_store(|s| s.search(&args.query)).map(|repos| {
            if repos.is_empty() {
                return format!("Nothing on the shelf matches {:?}.", args.query);
            }
            repos
                .iter()
                .map(|r| format!("{} [{}]", describe(r), r.category))
                .collect::<Vec<_>>()
                .join("\n")
        }))
    }

    #[tool(description = "Show a shelf repo's file tree (two levels below `path`) without cloning it.")]
    async fn get_tree(&self, Parameters(args): Parameters<TreeArgs>) -> Result<String, String> {
        text(self.shelf.tree(&args.repo, args.path.as_deref().unwrap_or("")).await)
    }

    #[tool(description = "Read one file from a shelf repo without cloning it. Long files are truncated.")]
    async fn read_file(&self, Parameters(args): Parameters<ReadFileArgs>) -> Result<String, String> {
        text(self.shelf.read_file(&args.repo, &args.path).await)
    }

    #[tool(description = "Add a GitHub repo to the shelf. Only when the user asks to save one.")]
    async fn add_repo(&self, Parameters(args): Parameters<AddRepoArgs>) -> Result<String, String> {
        let added = self.shelf.add(&args.link, &args.category).await;
        text(added.and_then(|repo| {
            if let Some(note) = args.note.as_deref() {
                self.shelf.with_store(|s| s.set_note(repo.id, note))?;
            }
            Ok(format!("Added {} to {}.", repo.full_name, repo.category))
        }))
    }

    #[tool(
        description = "Search code inside a shelf repo via GitHub (needs a GitHub token in repo-shelf settings). Returns matching file paths."
    )]
    async fn search_code(&self, Parameters(args): Parameters<SearchCodeArgs>) -> Result<String, String> {
        text(self.shelf.search_code(&args.repo, &args.query).await)
    }
}

#[tool_handler(name = "repo-shelf", instructions = "repo-shelf is the user's curated library of reference GitHub repos, \
sorted into categories. When you start domain work (a website, a CLI, auth, a database layer), \
call list_categories or search_shelf first and use matching repos as reference. Read their code \
remotely with get_tree and read_file. Never clone them into the workspace.")]
impl ServerHandler for ShelfServer {}

pub async fn serve() -> anyhow::Result<()> {
    let server = ShelfServer::new(Shelf::open_default()?);
    server.serve(stdio()).await?.waiting().await?;
    Ok(())
}
