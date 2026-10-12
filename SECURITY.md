# Security policy

## Reporting a vulnerability

Please report security problems privately through this repository's **Security** tab
(**Report a vulnerability**). Don't open a public issue for them.

## How repo-shelf handles your data

- There is no backend, no account and no telemetry. The app runs on your machine.
- The only network traffic is to GitHub (`api.github.com` and `raw.githubusercontent.com`)
  to fetch repo details, file trees and files.
- Your library, settings and file cache live in your app-data folder. See the README for
  where that is.
- **The optional GitHub token is stored unencrypted** in the local SQLite library
  (`shelf.db`). Anyone who can read your user folder can read it. Use a fine-grained token
  with no extra permissions, which is all repo-shelf needs for public repos.
- Paths an agent passes to `read_file` are checked before use. Only plain repo-relative
  paths are accepted, so an agent can't read or write files outside the cache.
- The window runs with a strict content security policy, and repo data is never inserted
  as HTML.

## Supported versions

Security fixes go into the latest release.
