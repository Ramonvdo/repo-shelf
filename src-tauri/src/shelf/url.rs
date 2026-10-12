/// A GitHub repository identified by owner and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub owner: String,
    pub name: String,
}

impl RepoRef {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// GitHub paths that look like `owner/repo` but are not repositories.
const RESERVED_OWNERS: &[&str] = &[
    "settings", "orgs", "topics", "marketplace", "explore", "features", "sponsors",
    "login", "notifications", "apps", "collections", "trending", "search", "pulls", "issues",
];

/// Accepts `owner/repo`, GitHub web/clone URLs, and deep links such as `/tree/<branch>/<path>`.
pub fn parse_github_url(input: &str) -> Result<RepoRef, String> {
    let s = input.trim();
    let bad = || format!("Not a GitHub repository: {s:?}");

    let path = if let Some(rest) = s.strip_prefix("git@github.com:") {
        rest
    } else {
        let no_scheme = s
            .strip_prefix("https://")
            .or_else(|| s.strip_prefix("http://"))
            .unwrap_or(s);
        let no_www = no_scheme.strip_prefix("www.").unwrap_or(no_scheme);
        match no_www.strip_prefix("github.com/") {
            Some(rest) => rest,
            // A bare `owner/repo` has no host and exactly two segments.
            None if !no_www.contains(':') && no_www.split('/').count() == 2 => no_www,
            None => return Err(bad()),
        }
    };

    let path = path.split(['?', '#']).next().unwrap_or("");
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    let owner = parts.next().ok_or_else(bad)?;
    let name = parts.next().ok_or_else(bad)?;
    let name = name.strip_suffix(".git").unwrap_or(name);

    let valid_owner = owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let valid_name = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if name.is_empty()
        || owner.is_empty()
        || name.chars().all(|c| c == '.')
        || !valid_owner
        || !valid_name
        || RESERVED_OWNERS.contains(&owner.to_ascii_lowercase().as_str())
    {
        return Err(bad());
    }
    Ok(RepoRef { owner: owner.to_string(), name: name.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(input: &str) -> String {
        parse_github_url(input).unwrap().full_name()
    }

    #[test]
    fn accepts_common_forms() {
        assert_eq!(ok("shadcn-ui/ui"), "shadcn-ui/ui");
        assert_eq!(ok("https://github.com/shadcn-ui/ui"), "shadcn-ui/ui");
        assert_eq!(ok("  github.com/vercel/next.js/  "), "vercel/next.js");
        assert_eq!(ok("https://github.com/vercel/next.js.git"), "vercel/next.js");
        assert_eq!(ok("git@github.com:tauri-apps/tauri.git"), "tauri-apps/tauri");
        assert_eq!(
            ok("https://github.com/vercel/next.js/tree/canary/examples/cms-strapi"),
            "vercel/next.js"
        );
        assert_eq!(ok("https://www.github.com/a/b?tab=readme"), "a/b");
    }

    #[test]
    fn rejects_non_repos() {
        assert!(parse_github_url("").is_err());
        assert!(parse_github_url("https://gitlab.com/a/b").is_err());
        assert!(parse_github_url("https://github.com/onlyowner").is_err());
        assert!(parse_github_url("a/b/c").is_err());
        assert!(parse_github_url("https://github.com/settings/profile").is_err());
    }
}
