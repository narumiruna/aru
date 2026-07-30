# aru File Contracts

All files are UTF-8. `aru.toml` is edited with `toml_edit` so unrelated comments and keys survive. Generated lock/state/journal maps and entries use lexical key order; instruction sources, packages, skills, targets, baselines, and state entries are explicitly sorted.

## `aru.toml`

```toml
[project]
targets = ["codex", "claude", "copilot", "opencode", "pi"]

[[instructions.sources]]
files = ["AGENTS.md", "src/**/AGENTS.md"]
exclude = ["target/**", "third_party/**"]
scope = "source-directory"

[[instructions.sources]]
files = ["docs/instructions/rust.md"]
apply-to = ["**/*.rs"]
targets = ["claude", "copilot"]

[skills]
"owner/repository" = { version = "0.5.0", include = ["writing-plans"], exclude = [], paths = { writing-plans = "skills/writing-plans" } }
"owner/development" = { branch = "main", include = ["reviewing-code"], exclude = [] }

[mcp.docs]
transport = "streamable-http"
url = "https://docs.example.com/mcp"
bearer-token-env = "DOCS_TOKEN"
```

The manifest is intentionally unversioned during early development. `project.targets` is a non-empty, duplicate-free persistent set. Target commands retain the existing value decoration so a trailing comment on `targets` survives mutation.

Each `instructions.sources` entry has non-empty project-relative `files`, optional `exclude` and `targets`, and exactly one scope form:

- `scope = "source-directory"` requires every matched source to be named `AGENTS.md` and derives each source's directory scope independently.
- `apply-to = [...]` declares exact repository-relative globs and is accepted only when every selected target can represent those globs without broadening.

Source `targets` default to the complete project target set and otherwise must be a duplicate-free subset. Discovery always excludes `.git/**` and `.aru/**`; onboarding also skips common build/vendor roots and writes the exact files it found. Unsafe/absolute patterns, parent traversal, duplicate matches, symlinks, non-regular files, non-UTF-8 paths/content, files larger than 1 MiB, generated output paths, and reserved aru marker text fail before writes.

Skill `version`, `branch`, and `rev` are mutually exclusive. `branch` stores moving user intent while the lock records its resolved commit; ordinary sync stays pinned and only `skill update` re-resolves it. `include` is either `['*']` or one or more validated names. `exclude` applies only to wildcard mode. `paths` is stable user intent, not transient resolution data. An MCP entry has exactly one of `server` (Registry) or `url` (direct remote). Secret-bearing fields contain environment names only.

`package-input-hash` canonicalizes credential-free skill requirements and MCP requirements but excludes targets and local instructions. Target and instruction changes therefore preserve package identity. The projection identity covers the complete package lock, normalized instruction-source records, sorted project targets, and adapter capability schema.

## Instruction projections

| Target | Directory-scoped AGENTS source | Explicit `apply-to` source |
| --- | --- | --- |
| Codex | Native source; no generated output | Rejected |
| pi | Native source; no generated output | Rejected |
| OpenCode | Native source; no generated output | Rejected |
| Claude Code | Sibling `CLAUDE.md` block containing `@AGENTS.md` | `.claude/rules/aru/<source-path>` with `paths` frontmatter |
| GitHub Copilot | Root block or `.github/instructions/aru/<source>.instructions.md` | `.github/instructions/aru/<source>.instructions.md` with `applyTo` frontmatter |

Shared Markdown files use a reversible percent-encoded source identity:

```md
<!-- aru:instruction:start AGENTS.md -->
@AGENTS.md
<!-- aru:instruction:end AGENTS.md -->
```

Every source owns only its marker block. Bytes outside blocks are unmanaged and survive merge, update, and cleanup. Existing unmanaged destinations collide by default. `--merge` authorizes adding blocks while preserving unmanaged Markdown; it is mutually exclusive with `--force`, which is explicit destructive takeover of an unmanaged destination, but never overrides drift in an already owned block.

Generated path-specific files are wholly aru-owned. Removing a source or target removes a file/block only when local state proves its current semantic digest equals the last applied digest. Missing state can adopt an exact committed baseline but never authorizes historical deletion. Unknown or drifted content is preserved and reported.

## `aru.lock`

`version = 1`. Each `instruction-source` locks a portable source path, normalized scope, sorted selected targets, and source SHA-256. `aru sync --locked` compares discovered sources exactly and rejects changed content, scope, targets, or adapter schema.

Each `skill-package` locks a normalized source, original requirement descriptor, selected SemVer/branch/revision label, full 40-hex commit, repository root name, and selected `{name,path,sha256}` entries. Branch requirements use `branch:<name>` while `revision` remains immutable. Each `mcp-server` locks exact normalized metadata and one concrete projection for each configured target with an implemented MCP adapter.

`projection-input-hash` covers complete lock identity, sorted project targets, and adapter capability schema. `projection-baseline` contains only currently desired semantic instruction, skill, and MCP entries. It can bootstrap ownership after state loss but cannot authorize historical deletion.

Codex skills project to `.agents/skills`. Claude skills project to `.claude/skills`: when Codex is also selected and project symlinks are supported, Claude entries link to `.agents`; otherwise they are copies. Instruction-only targets are filtered out before skill/MCP resolution. A declared skill or MCP requirement with no capable configured target fails explicitly.

The canonical skill digest byte stream is:

1. ASCII `aru-skill-digest-v1` plus NUL;
2. for each regular file sorted by portable `/` path: big-endian u64 path length, path bytes, one executable-marker byte, big-endian u64 content length, and raw content bytes.

Directories add no direct digest record. Symlinks and special files are rejected.

## `.aru/state.toml`

`version = 1`. Each `entry` has project-relative destination, kind/key, actual deployment mode (`copy`, `symlink`, `merge`, or `file`), last-applied semantic digest, and complete owning lock identity. Instruction `merge` entries track one source block; instruction `file` entries track a whole generated path. State proves local ownership but never replaces the committed baseline.

## `.aru/transaction.toml`

`version = 1`, phase (`prepared`, `applying`, or `committed`), and ordered entries. Each entry records project-relative destination/stage/backup paths, optional old/new physical digests, and whether journal persistence observed apply. Secret data and file bytes are never journaled.

On recovery, only old/new digest matches are actionable. Unknown content stops recovery and remains untouched. User-owned instruction sources are never transaction destinations.

Golden fixtures are under `tests/fixtures/contracts/` and `tests/fixtures/instructions/` and are parsed or rendered by the normal test suite.
