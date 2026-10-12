use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::path::Path;

use super::url::RepoRef;

/// Facts fetched from GitHub when a repo is added.
#[derive(Debug, Clone, Default)]
pub struct RepoMeta {
    pub description: Option<String>,
    pub stars: i64,
    pub topics: Vec<String>,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Repo {
    pub id: i64,
    pub full_name: String,
    pub category: String,
    pub note: String,
    pub description: Option<String>,
    pub stars: i64,
    pub topics: Vec<String>,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Category {
    pub name: String,
    pub count: i64,
}

pub struct Store {
    conn: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS categories (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);
CREATE TABLE IF NOT EXISTS repos (
    id             INTEGER PRIMARY KEY,
    owner          TEXT NOT NULL COLLATE NOCASE,
    name           TEXT NOT NULL COLLATE NOCASE,
    category_id    INTEGER NOT NULL REFERENCES categories(id),
    note           TEXT NOT NULL DEFAULT '',
    description    TEXT,
    stars          INTEGER NOT NULL DEFAULT 0,
    topics         TEXT NOT NULL DEFAULT '',
    default_branch TEXT NOT NULL DEFAULT 'main',
    added_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (owner, name)
);
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

const REPO_SELECT: &str = "
SELECT r.id, r.owner || '/' || r.name, c.name, r.note, r.description, r.stars, r.topics, r.default_branch
FROM repos r JOIN categories c ON c.id = r.category_id";

fn row_to_repo(row: &rusqlite::Row) -> rusqlite::Result<Repo> {
    let topics: String = row.get(6)?;
    Ok(Repo {
        id: row.get(0)?,
        full_name: row.get(1)?,
        category: row.get(2)?,
        note: row.get(3)?,
        description: row.get(4)?,
        stars: row.get(5)?,
        topics: topics.split_whitespace().map(str::to_string).collect(),
        default_branch: row.get(7)?,
    })
}

impl Store {
    /// Opens (creating if needed) the library. The GUI and the MCP server share this file.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn add_category(&self, name: &str) -> Result<()> {
        self.category_id(name).map(|_| ())
    }

    /// Returns the id of `name`, creating the category if it does not exist.
    fn category_id(&self, name: &str) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            bail!("Category name is empty");
        }
        self.conn
            .execute("INSERT OR IGNORE INTO categories (name) VALUES (?1)", [name])?;
        Ok(self
            .conn
            .query_row("SELECT id FROM categories WHERE name = ?1", [name], |r| r.get(0))?)
    }

    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.name, COUNT(r.id) FROM categories c
             LEFT JOIN repos r ON r.category_id = c.id
             GROUP BY c.id ORDER BY c.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| Ok(Category { name: r.get(0)?, count: r.get(1)? }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_repo(&self, repo: &RepoRef, category: &str, meta: &RepoMeta) -> Result<Repo> {
        if let Some(existing) = self.find_repo(&repo.full_name())? {
            bail!("{} is already on the shelf in {}", existing.full_name, existing.category);
        }
        let category_id = self.category_id(category)?;
        self.conn.execute(
            "INSERT INTO repos (owner, name, category_id, description, stars, topics, default_branch)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                repo.owner,
                repo.name,
                category_id,
                meta.description,
                meta.stars,
                meta.topics.join(" "),
                meta.default_branch,
            ],
        )?;
        self.get_repo(self.conn.last_insert_rowid())
    }

    fn get_repo(&self, id: i64) -> Result<Repo> {
        Ok(self
            .conn
            .query_row(&format!("{REPO_SELECT} WHERE r.id = ?1"), [id], row_to_repo)?)
    }

    /// Looks a repo up by `owner/name`, ignoring case.
    pub fn find_repo(&self, full_name: &str) -> Result<Option<Repo>> {
        Ok(self
            .conn
            .query_row(
                &format!("{REPO_SELECT} WHERE (r.owner || '/' || r.name) = ?1 COLLATE NOCASE"),
                [full_name.trim()],
                row_to_repo,
            )
            .optional()?)
    }

    pub fn list_repos(&self, category: Option<&str>) -> Result<Vec<Repo>> {
        let sql = format!(
            "{REPO_SELECT} WHERE ?1 IS NULL OR c.name = ?1 COLLATE NOCASE
             ORDER BY r.owner || '/' || r.name COLLATE NOCASE"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([category], row_to_repo)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every whitespace-separated term must appear in the name, description, topics, note or category.
    pub fn search(&self, query: &str) -> Result<Vec<Repo>> {
        let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut repos = self.list_repos(None)?;
        repos.retain(|r| {
            let haystack = format!(
                "{} {} {} {} {}",
                r.full_name,
                r.description.as_deref().unwrap_or(""),
                r.topics.join(" "),
                r.note,
                r.category
            )
            .to_lowercase();
            terms.iter().all(|t| haystack.contains(t.as_str()))
        });
        Ok(repos)
    }

    pub fn set_note(&self, id: i64, note: &str) -> Result<()> {
        self.conn
            .execute("UPDATE repos SET note = ?1 WHERE id = ?2", params![note.trim(), id])?;
        Ok(())
    }

    pub fn move_repo(&self, id: i64, category: &str) -> Result<()> {
        let category_id = self.category_id(category)?;
        self.conn
            .execute("UPDATE repos SET category_id = ?1 WHERE id = ?2", params![category_id, id])?;
        Ok(())
    }

    pub fn delete_repo(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM repos WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Deletes an empty category; refuses one that still holds repos.
    pub fn delete_category(&self, name: &str) -> Result<()> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM repos r JOIN categories c ON c.id = r.category_id WHERE c.name = ?1",
            [name],
            |r| r.get(0),
        )?;
        if count > 0 {
            bail!("{name} still holds {count} repo(s); move or remove them first");
        }
        self.conn.execute("DELETE FROM categories WHERE name = ?1", [name])?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("shelf.db")).unwrap();
        (dir, store)
    }

    fn rr(owner: &str, name: &str) -> RepoRef {
        RepoRef { owner: owner.into(), name: name.into() }
    }

    fn meta(desc: &str, topics: &[&str]) -> RepoMeta {
        RepoMeta {
            description: Some(desc.into()),
            stars: 10,
            topics: topics.iter().map(|t| t.to_string()).collect(),
            default_branch: "main".into(),
        }
    }

    #[test]
    fn add_and_list_by_category() {
        let (_d, s) = temp_store();
        s.add_category("Websites").unwrap();
        s.add_category("websites").unwrap(); // case-insensitive, no duplicate
        s.add_category("Databases").unwrap();
        s.add_repo(&rr("shadcn-ui", "ui"), "Websites", &meta("Components", &["react"])).unwrap();
        s.add_repo(&rr("drizzle-team", "drizzle-orm"), "Databases", &meta("ORM", &[])).unwrap();
        // Adding to an unknown category creates it.
        s.add_repo(&rr("tauri-apps", "tauri"), "Desktop", &meta("Apps", &[])).unwrap();

        assert!(s.add_repo(&rr("shadcn-ui", "UI"), "Databases", &RepoMeta::default()).is_err());

        let cats = s.categories().unwrap();
        let summary: Vec<(String, i64)> = cats.into_iter().map(|c| (c.name, c.count)).collect();
        assert_eq!(
            summary,
            vec![("Databases".into(), 1), ("Desktop".into(), 1), ("Websites".into(), 1)]
        );

        let web = s.list_repos(Some("websites")).unwrap();
        assert_eq!(web.len(), 1);
        assert_eq!(web[0].full_name, "shadcn-ui/ui");
        assert_eq!(web[0].category, "Websites");
        assert_eq!(web[0].topics, vec!["react".to_string()]);
        assert_eq!(s.list_repos(None).unwrap().len(), 3);
    }

    #[test]
    fn search_note_move_delete_settings() {
        let (_d, s) = temp_store();
        let ui = s.add_repo(&rr("shadcn-ui", "ui"), "Websites", &meta("Beautiful components", &["react", "tailwindcss"])).unwrap();
        let orm = s.add_repo(&rr("drizzle-team", "drizzle-orm"), "Databases", &meta("TypeScript ORM", &["sql"])).unwrap();

        let names = |v: Vec<Repo>| v.into_iter().map(|r| r.full_name).collect::<Vec<_>>();
        assert_eq!(names(s.search("TAILWIND").unwrap()), vec!["shadcn-ui/ui"]);
        assert_eq!(names(s.search("orm typescript").unwrap()), vec!["drizzle-team/drizzle-orm"]);
        assert_eq!(names(s.search("websites").unwrap()), vec!["shadcn-ui/ui"]);
        assert!(s.search("react sql").unwrap().is_empty());

        s.set_note(ui.id, "Use the modal pattern").unwrap();
        assert_eq!(names(s.search("modal").unwrap()), vec!["shadcn-ui/ui"]);

        s.move_repo(orm.id, "Backend").unwrap();
        assert_eq!(s.find_repo("DRIZZLE-team/drizzle-orm").unwrap().unwrap().category, "Backend");

        assert!(s.delete_category("Websites").is_err());
        s.delete_category("Databases").unwrap();
        s.delete_repo(ui.id).unwrap();
        assert_eq!(names(s.list_repos(None).unwrap()), vec!["drizzle-team/drizzle-orm"]);

        assert_eq!(s.setting("github_token").unwrap(), None);
        s.set_setting("github_token", "abc").unwrap();
        s.set_setting("github_token", "def").unwrap();
        assert_eq!(s.setting("github_token").unwrap().as_deref(), Some("def"));
    }
}
