# aru File Contracts

All files are UTF-8. `aru.toml` is edited with `toml_edit` so unrelated comments and keys survive. Generated lock/state/journal maps and entries use lexical key order; package, skill, target, baseline, and state-entry arrays are explicitly sorted.

## `aru.toml`

```toml
[project]
agents = ["codex", "claude-code"]

[skills]
"owner/repository" = { version = "0.5.0", include = ["writing-plans"], exclude = [], paths = { writing-plans = "skills/writing-plans" } }
"owner/development" = { branch = "main", include = ["reviewing-code"], exclude = [] }

[mcp.docs]
transport = "streamable-http"
url = "https://docs.example.com/mcp"
bearer-token-env = "DOCS_TOKEN"
```

The manifest is intentionally unversioned during early development. Skill `version`, `branch`, and `rev` are mutually exclusive. `branch` stores moving user intent while the lock records its resolved commit; ordinary sync stays pinned and only `skill update` re-resolves it. `include` is either `['*']` or one or more validated names. `exclude` applies only to wildcard mode. `paths` is stable user intent, not transient resolution data. An MCP entry has exactly one of `server` (Registry) or `url` (direct remote). Secret-bearing fields contain environment names only.

`package-input-hash` canonicalizes every skill requirement with credential-free source identity plus every MCP requirement. It excludes `project.agents`.

## `aru.lock`

`version = 1`. Each `skill-package` locks a normalized source, original requirement descriptor, selected SemVer/branch/revision label, full 40-hex commit, repository root name, and selected `{name,path,sha256}` entries. Branch requirements use `branch:<name>` while `revision` remains immutable. Each `mcp-server` locks exact normalized metadata and one concrete target per selected agent.

`projection-input-hash` covers exact package lock identity, sorted agents, targets, and adapter capability schema version. `projection-baseline` contains only currently desired semantic entries. It can bootstrap ownership after state loss but cannot authorize historical deletion.

The canonical skill digest byte stream is:

1. ASCII `aru-skill-digest-v1` plus NUL;
2. for each regular file sorted by portable `/` path: big-endian u64 path length, path bytes, one executable-marker byte, big-endian u64 content length, and raw content bytes.

Directories add no direct digest record. Symlinks and special files are rejected.

## `.aru/state.toml`

`version = 1`. Each `entry` has project-relative destination, kind/key, actual deployment mode (`copy`, `symlink`, or `merge`), last-applied semantic digest, and owning lock identity. State proves local ownership but never replaces the committed baseline.

## `.aru/transaction.toml`

`version = 1`, phase (`prepared`, `applying`, or `committed`), and ordered entries. Each entry records project-relative destination/stage/backup paths, optional old/new physical digests, and whether journal persistence observed apply. Secret data and file bytes are never journaled.

On recovery, only old/new digest matches are actionable. Unknown content stops recovery and remains untouched.

Golden fixtures are under `tests/fixtures/contracts/` and are parsed by the normal test suite.
