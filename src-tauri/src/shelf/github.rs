use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::path::PathBuf;

use super::store::RepoMeta;
use super::url::RepoRef;

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct TreeEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String, // "blob" | "tree" | "commit"
    pub size: Option<u64>,
}

/// Directories and files that never help an agent read reference code.
const NOISE_DIRS: &[&str] = &["node_modules", ".git", "dist", "vendor", "__pycache__", ".next", "target"];
const NOISE_FILES: &[&str] = &[
    "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb", "bun.lock", "Cargo.lock",
    "poetry.lock", "composer.lock", "go.sum", ".DS_Store",
];
const BINARY_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "bmp", "avif", "woff", "woff2", "ttf", "otf", "eot",
    "mp3", "mp4", "webm", "mov", "wav", "zip", "gz", "tar", "7z", "pdf", "exe", "dll", "so", "dylib",
    "wasm", "jar", "class", "pyc", "icns", "tiff", "psd", "heic", "bin", "dat", "db", "sqlite",
    "rlib", "o", "a", "lib", "obj", "pdb", "node", "msi", "dmg", "deb", "rpm", "apk", "ipa",
];

fn is_noise(path: &str) -> bool {
    let mut segments = path.split('/');
    let file = segments.next_back().unwrap_or("");
    if path.split('/').any(|s| NOISE_DIRS.contains(&s)) || NOISE_FILES.contains(&file) {
        return true;
    }
    let ext = file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    BINARY_EXTS.contains(&ext.as_str())
}

/// Checks that `path` names a file inside a repo, so it is safe to join onto the cache dir
/// on every OS: plain `/`-separated segments, no `.`/`..`, no backslashes, drives or NULs.
pub fn repo_path(path: &str) -> Result<&str> {
    let path = path.trim_matches('/');
    let ok = !path.is_empty()
        && path.split('/').all(|s| {
            !s.is_empty() && s != "." && s != ".." && !s.contains(['\\', ':', '\0'])
        });
    if !ok {
        bail!("Invalid path: {path:?}. Use a path from get_tree, like src/main.rs.");
    }
    Ok(path)
}

/// Renders the part of a repo tree under `prefix`, `max_depth` levels deep, as one path per line.
/// Directories at the depth limit show how many files sit below them.
pub fn render_tree(entries: &[TreeEntry], prefix: &str, max_depth: usize, max_lines: usize) -> String {
    let prefix = prefix.trim_matches('/');
    let base = if prefix.is_empty() { String::new() } else { format!("{prefix}/") };
    let mut visible: Vec<&TreeEntry> = entries
        .iter()
        .filter(|e| e.path.starts_with(&base) && !is_noise(&e.path) && e.kind != "commit")
        .collect();
    // Keep only directories that still hold a visible file.
    let mut nonempty = std::collections::HashSet::new();
    for e in visible.iter().filter(|e| e.kind == "blob") {
        let mut p = e.path.as_str();
        while let Some((dir, _)) = p.rsplit_once('/') {
            if !nonempty.insert(dir) {
                break;
            }
            p = dir;
        }
    }
    visible.retain(|e| e.kind != "tree" || nonempty.contains(e.path.as_str()));
    if visible.is_empty() {
        return format!("No files under {prefix:?}.\n");
    }
    visible.sort_by(|a, b| a.path.cmp(&b.path));

    let depth = |e: &TreeEntry| e.path[base.len()..].split('/').count();
    let shown: Vec<&TreeEntry> = visible.iter().copied().filter(|e| depth(e) <= max_depth).collect();

    let mut out = String::new();
    for e in shown.iter().take(max_lines) {
        if e.kind == "tree" {
            out.push_str(&e.path);
            out.push('/');
            if depth(e) == max_depth {
                let inner = format!("{}/", e.path);
                let files = visible
                    .iter()
                    .filter(|x| x.kind == "blob" && x.path.starts_with(&inner))
                    .count();
                out.push_str(&format!(" ({files} files)"));
            }
        } else {
            out.push_str(&format!("{} ({})", e.path, human_size(e.size.unwrap_or(0))));
        }
        out.push('\n');
    }
    if shown.len() > max_lines {
        out.push_str(&format!(
            "… {} more entries. Pass a deeper path to narrow.\n",
            shown.len() - max_lines
        ));
    }
    out
}

fn human_size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

/// Turns raw file bytes into agent-ready text: refuses binaries and caps very long files.
pub fn cap_file(bytes: &[u8], max_chars: usize) -> Result<String> {
    if bytes.contains(&0) {
        bail!("This looks like a binary file; repo-shelf only returns text.");
    }
    let text = String::from_utf8_lossy(bytes);
    let total = text.chars().count();
    if total <= max_chars {
        return Ok(text.into_owned());
    }
    let head: String = text.chars().take(max_chars).collect();
    Ok(format!("{head}

[repo-shelf: truncated, showing {max_chars} of {total} characters]"))
}

/// Picks the fields repo-shelf keeps from `GET /repos/{owner}/{repo}`.
pub fn parse_meta(json: &serde_json::Value) -> RepoMeta {
    RepoMeta {
        description: json["description"].as_str().map(str::to_string),
        stars: json["stargazers_count"].as_i64().unwrap_or(0),
        topics: json["topics"]
            .as_array()
            .map(|t| t.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        default_branch: json["default_branch"].as_str().unwrap_or("main").to_string(),
    }
}

/// A repo's file list at one commit.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Tree {
    pub sha: String,
    pub tree: Vec<TreeEntry>,
    #[serde(default)]
    pub truncated: bool,
}

/// How long a cached tree is trusted before asking GitHub again.
const TREE_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Reads GitHub without cloning. Trees and files are cached under `cache_dir`,
/// files keyed by commit sha so a cached file is never stale for its tree.
pub struct GitHub {
    http: reqwest::Client,
    token: Option<String>,
    cache_dir: PathBuf,
}

impl GitHub {
    pub fn new(token: Option<String>, cache_dir: PathBuf) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("repo-shelf/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("http client");
        Self { http, token: token.filter(|t| !t.trim().is_empty()), cache_dir }
    }

    async fn get(&self, url: &str, accept: &str) -> Result<reqwest::Response> {
        let mut req = self.http.get(url).header("Accept", accept);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let res = req.send().await.with_context(|| format!("requesting {url}"))?;
        let status = res.status();
        if status.is_success() {
            return Ok(res);
        }
        let rate_limited = res
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|v| v == "0");
        if rate_limited {
            bail!(
                "GitHub rate limit reached (60 requests/hour without a token). Add a GitHub token in repo-shelf settings to raise it to 5,000."
            );
        }
        match status.as_u16() {
            404 => bail!("Not found on GitHub: {url}"),
            401 => bail!("GitHub rejected the token saved in repo-shelf settings."),
            code => bail!("GitHub returned HTTP {code} for {url}"),
        }
    }

    pub async fn repo_meta(&self, repo: &RepoRef) -> Result<RepoMeta> {
        let url = format!("https://api.github.com/repos/{}", repo.full_name());
        let json: serde_json::Value = self.get(&url, "application/vnd.github+json").await?.json().await?;
        Ok(parse_meta(&json))
    }

    pub async fn tree(&self, full_name: &str, branch: &str) -> Result<Tree> {
        let cache = self.cache_dir.join(full_name).join(format!("tree-{branch}.json"));
        let fresh = std::fs::metadata(&cache)
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().unwrap_or(TREE_TTL) < TREE_TTL);
        if fresh {
            if let Ok(tree) = serde_json::from_slice(&std::fs::read(&cache)?) {
                return Ok(tree);
            }
        }
        let url = format!("https://api.github.com/repos/{full_name}/git/trees/{branch}?recursive=1");
        let bytes = self.get(&url, "application/vnd.github+json").await?.bytes().await?;
        let tree: Tree = serde_json::from_slice(&bytes).context("parsing GitHub tree")?;
        std::fs::create_dir_all(cache.parent().unwrap())?;
        std::fs::write(&cache, serde_json::to_vec(&tree)?)?;
        Ok(tree)
    }

    /// Raw bytes of one file at `sha`.
    pub async fn file(&self, full_name: &str, sha: &str, path: &str) -> Result<Vec<u8>> {
        let path = repo_path(path)?;
        if !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("Invalid commit: {sha}");
        }
        let cache = self.cache_dir.join(full_name).join(sha).join(path);
        if let Ok(bytes) = std::fs::read(&cache) {
            return Ok(bytes);
        }
        let url = format!("https://raw.githubusercontent.com/{full_name}/{sha}/{path}");
        let bytes = self.get(&url, "*/*").await?.bytes().await?.to_vec();
        std::fs::create_dir_all(cache.parent().unwrap())?;
        std::fs::write(&cache, &bytes)?;
        Ok(bytes)
    }

    /// GitHub code search inside one repo. GitHub only allows this with a token.
    pub async fn search_code(&self, full_name: &str, query: &str) -> Result<Vec<String>> {
        if self.token.is_none() {
            bail!("Code search needs a GitHub token. Add one in repo-shelf settings, or use get_tree and read_file instead.");
        }
        let q = format!("{query} repo:{full_name}");
        let url = reqwest::Url::parse_with_params(
            "https://api.github.com/search/code",
            &[("q", q.as_str()), ("per_page", "30")],
        )
        .map_err(|e| anyhow!(e))?;
        let json: serde_json::Value = self.get(url.as_str(), "application/vnd.github+json").await?.json().await?;
        Ok(json["items"]
            .as_array()
            .map(|items| items.iter().filter_map(|i| i["path"].as_str().map(str::to_string)).collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_holding_only_noise_are_hidden() {
        let e = |path: &str, kind: &str| TreeEntry { path: path.into(), kind: kind.into(), size: Some(10) };
        let entries = [
            e("Assets", "tree"),
            e("Assets/logo.png", "blob"),
            e("src", "tree"),
            e("src/main.rs", "blob"),
        ];
        assert_eq!(render_tree(&entries, "", 2, 50), "src/\nsrc/main.rs (10 B)\n");
    }

    #[test]
    fn repo_paths_stay_inside_the_repo() {
        assert_eq!(repo_path("/src/main.rs/").unwrap(), "src/main.rs");
        assert_eq!(repo_path("README").unwrap(), "README");
        for bad in [
            "",
            "../../etc/passwd",
            "crates/../../x",
            r"..\..\..\..\..\canary.txt",
            r"C:\Users\me\canary.txt",
            "C:/Users/me/canary.txt",
            "src//main.rs",
            "./src",
            "a\0b",
        ] {
            assert!(repo_path(bad).is_err(), "accepted {bad:?}");
        }
    }

    fn blob(path: &str, size: u64) -> TreeEntry {
        TreeEntry { path: path.into(), kind: "blob".into(), size: Some(size) }
    }
    fn dir(path: &str) -> TreeEntry {
        TreeEntry { path: path.into(), kind: "tree".into(), size: None }
    }

    fn sample() -> Vec<TreeEntry> {
        vec![
            blob("README.md", 2048),
            blob("package-lock.json", 900000),
            dir("node_modules"),
            blob("node_modules/x/index.js", 10),
            dir("apps"),
            dir("apps/www"),
            blob("apps/www/page.tsx", 500),
            dir("apps/www/components"),
            blob("apps/www/components/button.tsx", 1200),
            blob("apps/www/components/logo.png", 4000),
            dir("apps/www/components/ui"),
            blob("apps/www/components/ui/dialog.tsx", 3000),
            blob("apps/www/components/ui/card.tsx", 800),
        ]
    }

    #[test]
    fn renders_root_with_depth_limit_and_noise_filtered() {
        let out = render_tree(&sample(), "", 2, 100);
        assert_eq!(
            out,
            "README.md (2.0 KB)\n\
             apps/\n\
             apps/www/ (4 files)\n"
        );
    }

    #[test]
    fn renders_subpath_and_truncates() {
        let out = render_tree(&sample(), "apps/www/", 2, 100);
        assert_eq!(
            out,
            "apps/www/components/\n\
             apps/www/components/button.tsx (1.2 KB)\n\
             apps/www/components/ui/ (2 files)\n\
             apps/www/page.tsx (500 B)\n"
        );
        let short = render_tree(&sample(), "apps/www", 2, 2);
        assert_eq!(
            short,
            "apps/www/components/\n\
             apps/www/components/button.tsx (1.2 KB)\n\
             … 2 more entries. Pass a deeper path to narrow.\n"
        );
        assert_eq!(render_tree(&sample(), "nope", 2, 10), "No files under \"nope\".\n");
    }

    #[test]
    fn caps_long_files_and_refuses_binaries() {
        assert_eq!(cap_file(b"hello
world
", 100).unwrap(), "hello
world
");
        assert!(cap_file(b"PNG\0\x01", 100).is_err());
        let capped = cap_file("aaaa
bbbb
cccc
".as_bytes(), 7).unwrap();
        assert_eq!(capped, "aaaa
bb

[repo-shelf: truncated, showing 7 of 15 characters]");
        // Never splits a multi-byte character.
        assert!(cap_file("ééé".as_bytes(), 2).unwrap().starts_with("éé
"));
    }

    #[test]
    fn parses_repo_meta() {
        let json = serde_json::json!({
            "full_name": "shadcn-ui/ui",
            "description": "Beautifully designed components",
            "stargazers_count": 90000,
            "topics": ["react", "tailwindcss"],
            "default_branch": "main"
        });
        let m = parse_meta(&json);
        assert_eq!(m.description.as_deref(), Some("Beautifully designed components"));
        assert_eq!(m.stars, 90000);
        assert_eq!(m.topics, vec!["react", "tailwindcss"]);
        assert_eq!(m.default_branch, "main");
        let empty = parse_meta(&serde_json::json!({"description": null}));
        assert_eq!((empty.description, empty.default_branch.as_str()), (None, "main"));
    }

    #[tokio::test]
    #[ignore = "hits the live GitHub API"]
    async fn live_tree_and_file() {
        let dir = tempfile::tempdir().unwrap();
        let gh = GitHub::new(None, dir.path().to_path_buf());
        let meta = gh.repo_meta(&RepoRef { owner: "octocat".into(), name: "Hello-World".into() }).await.unwrap();
        let tree = gh.tree("octocat/Hello-World", &meta.default_branch).await.unwrap();
        assert!(tree.tree.iter().any(|e| e.path == "README"));
        let readme = gh.file("octocat/Hello-World", &tree.sha, "README").await.unwrap();
        assert!(String::from_utf8_lossy(&readme).contains("Hello World"));
    }
}
