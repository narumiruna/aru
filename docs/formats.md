# aru File Contracts

All files are UTF-8.
`aru.toml` is edited with `toml_edit` so unrelated comments and keys survive.
Generated lock/state/journal maps and entries use lexical key order; instruction sources, packages, plugins, skills, targets, baselines, and state entries are explicitly sorted.

## `aru.toml`

```toml
[project]
targets = ["agents", "codex", "claude", "copilot", "opencode", "pi", "kiro"]

[[instructions.sources]]
files = ["AGENTS.md", "src/**/AGENTS.md"]
exclude = ["target/**", "third_party/**"]
scope = "source-directory"

[[instructions.sources]]
files = ["docs/instructions/rust.md"]
apply-to = ["**/*.rs"]
targets = ["claude", "copilot"]

[skills]
"owner/repository" = { version = "0.5.0", include = ["writing-plans"], exclude = [], paths = { writing-plans = "skills/writing-plans" }, targets = ["codex"] }
"owner/development" = { branch = "main", include = ["reviewing-code"], exclude = [] }

[mcp.docs]
transport = "streamable-http"
url = "https://docs.example.com/mcp"
bearer-token-env = "DOCS_TOKEN"
targets = ["claude"]

[mcp.docs.env-http-headers]
X-Workspace = "DOCS_WORKSPACE"

[mcp.yfinance]
command = "uvx"
args = ["--with", "mcp<2", "yfmcp@0.12.2"]
env-vars = ["YFINANCE_API_KEY"]
```

The manifest is intentionally unversioned during early development.
`project.targets` is a non-empty, duplicate-free persistent set of canonical target identifiers.
CLI aliases normalize before mutation and are not valid serialized manifest or lock identifiers.
Target commands retain the existing value decoration so a trailing comment on `targets` survives mutation.
The complete version-specific canonical registry is emitted by `aru target list --available` and documented in `docs/public/reference/skill-targets.md`.

Each `instructions.sources` entry has non-empty project-relative `files`, optional `exclude` and `targets`, and exactly one scope form:

- `scope = "source-directory"` requires every matched source to be named `AGENTS.md` and derives each source's directory scope independently.
- `apply-to = [...]` declares exact repository-relative globs and is accepted only when every selected target can represent those globs without broadening.

Source `targets` default to the instruction-capable intersection of the project target set and otherwise must be a duplicate-free, instruction-capable subset.
A declaration with no capable effective target fails before writes.
Projects without instruction declarations skip source resolution.
Otherwise, source resolution traverses only the fixed roots implied by configured `files` selectors; a selector beginning with a wildcard necessarily starts at the project root.
Source resolution always excludes `.git/**` and `.aru/**`.
Unsafe/absolute patterns, parent traversal, duplicate matches, symlinks, non-regular files, non-UTF-8 paths/content, files larger than 1 MiB, generated output paths, and reserved aru marker text fail before writes.

Skill `version`, `branch`, and `rev` are mutually exclusive. `branch` stores moving user intent while the lock records its resolved commit; ordinary sync stays pinned and only `skill update` re-resolves it. `include` is either `['*']` or one or more validated names. `exclude` applies only to wildcard mode. `paths` is stable user intent, not transient resolution data. Optional skill `targets` must be a non-empty, duplicate-free subset of `project.targets` whose adapters support skills; omission means every compatible project target. All current targets support skills through their native project directories.

An MCP entry has exactly one of `server` (Registry), `url` (direct remote), or `command` (direct stdio). Registry stdio candidates support npm with an absent or `npx` runtime hint and PyPI with an explicit `uvx` runtime hint; exact command/argv and `{registry, identifier, version}` package identity are locked. Other package types or hints, secret arguments, and unresolved required arguments fail closed. Direct stdio `args` preserve ordered argv, default to `transport = "stdio"`, and cannot combine with Registry selectors, `version`, or `bearer-token-env`; optional `env-vars` is a non-empty-name, duplicate-free list that is valid only with `command`. Direct remote `env-http-headers` maps validated, case-insensitively unique HTTP field names to environment names and is valid only with `url`; it cannot define `Authorization` when `bearer-token-env` is also present. Environment names use uppercase letters, digits, and underscores and begin with an uppercase letter or underscore. Optional MCP `targets` obey the same subset rule and may contain only MCP-capable targets: Codex, Claude, Copilot, or OpenCode.
Agents, pi, and every skill-only target have no built-in MCP and are rejected.
aru records and projects direct commands but never executes them, and secret-bearing fields contain environment names only.
Codex retains names, Claude and Copilot emit `${ENV}` placeholders, and OpenCode emits `{env:ENV}` placeholders.

`package-input-hash` canonicalizes credential-free skill, MCP, native-package, and plugin requirements but excludes project targets, dependency targets, and local instructions.
Target and instruction changes therefore preserve package identity.
The projection identity covers the complete package lock, normalized instruction-source records, sorted project targets, and adapter capability schema.

## Plugin dependencies

A project declares plugin dependencies by stable plugin name:

```toml
[plugins.review-tools]
source = "owner/monorepo"
format = "openai"
subdir = "plugins/review"
version = "^1.2"
components = ["skills"]
mcp = ["docs"]
targets = ["codex", "claude"]

[plugin-trust.review-tools]
mcp = ["docs"]
```

`format` is `agent-plugins`, `openai`, or `gemini` and is always persisted after CLI detection.
`subdir` is a contained repository-relative plugin root.
At most one of `version`, `branch`, or `rev` is allowed.
Absence of `components`, `skills`, and `mcp` means whole-plugin intent.
A wildcard component and named selections of the same type are mutually exclusive.
Every selected MCP name requires a matching `plugin-trust` entry.

Agent Plugins 1.0 uses the canonical root `plugin.json`, immediate `skills/*/SKILL.md`, and root `mcp.json` locations.
OpenAI supports legacy `.codex-plugin/plugin.json` paths and an Agent Plugins base with a `com.openai` overlay.
Gemini uses root `gemini-extension.json`, root `skills/`, and inline `mcpServers`.
Detection checks only the selected plugin root and rejects independent multi-format ambiguity.
The selected format and all contributing manifest digests are locked, so replay never redetects format.

Selected remote MCP accepts only lossless Streamable HTTP with an absolute HTTPS URL and no literal headers, authentication, or variable expansion.
Selected stdio MCP accepts only one bare executable token plus opaque ordered argv, with no `cwd`, configured environment values, plugin placeholders, absolute paths, or explicit relative path arguments.
Apps, hooks, commands, OAuth, SSE, bundled executables, plugin data directories, disabled servers, and unknown transports are not projected.
Aru never executes plugin code or MCP commands.

## Native aru packages

A project declares reusable Git packages in `aru.toml`:

```toml
[packages]
"owner/agent-kit" = { version = "^1.2", targets = ["codex", "claude"] }

[package-trust."owner/agent-kit"]
mcp = ["docs"]
```

Package requirements support one of `version`, `branch`, or `rev`, plus an optional non-empty target subset. Omitted targets inherit the complete parent target set. Root package targets must be a subset of `project.targets`; transitive targets must be a subset of their parent's effective targets. Reaching the same canonical source through multiple parents unions compatible target sets, but conflicting requirement descriptors fail instead of invoking an implicit solver.

A package is a Git repository with a package-mode root `aru.toml`:

```toml
# aru.toml
[package]
name = "agent-kit"
version = "1.2.0"

[[instructions.sources]]
files = ["AGENTS.md"]
scope = "source-directory"

[skills]
review = "skills/review"

[mcp.docs]
url = "https://docs.example.com/mcp"

[dependencies]
"owner/shared-agent-rules" = { version = "^1.0" }
```

The project (`[project]`) and package (`[package]`) schemas are distinct `aru.toml` modes; one file does not serve both roles simultaneously, and the legacy `aru-package.toml` filename is not recognized. `package.name` uses aru's lowercase portable name grammar and `package.version` is exact SemVer. A SemVer Git tag must agree with the package version. Unknown top-level or nested fields fail. The file has no scripts, hooks, commands to execute, secret values, target discovery, or compatibility dispatch.

Instruction declarations use the ordinary instruction source schema relative to the package checkout. Their stable lock identity is namespaced by package source while directory scope remains project-relative. Package instructions for native AGENTS targets are managed blocks at the corresponding project `AGENTS.md`; a caller must use `--merge` to preserve an existing unmanaged document. Package skill exports map a globally unique skill name to one portable package-relative directory. Package MCP names are also project-global.

Every package-provided MCP server is denied unless the root project has a `package-trust` entry for the package's canonical source identity or an equivalent declared source that explicitly lists the MCP name. Trust entries contain names only, are credential-free, and do not bypass transport or target capability validation.

Root package sources may be local or remote Git repositories. Transitive local/file dependencies are rejected because their meaning would depend on a parent checkout location. Resolution is bounded to depth 16, 128 package nodes, 512 edges, 100,000 package-tree entries, and 256 MiB of regular-file content. Each package tree is additionally bounded to depth 32 and 20,000 entries. Symlinks, special files, non-portable paths, case-folding collisions, unsafe hidden Unicode, cycles, ambiguous canonical identities, and duplicate exports fail before project writes.

`aru.lock` records exact package source, requirement descriptor, selected version/revision, package metadata, effective targets, dependency edges, and exported instruction/skill/MCP identities. Compatible locked nodes remain pinned unless selected by `aru update`; locked offline sync verifies cached package manifests and exports before projection.

`aru package` builds `<name>-<version>.aru-package.tar.gz` under `target/aru-package/` unless `--output` is explicit. It requires a Git repository root and clean status unless `--allow-dirty` is explicit. Inventory comes from tracked and non-ignored files; ignored files, `.aru/`, and `target/aru-package/` are excluded. The command snapshots and validates the exact inventory through the ordinary package parser and bounded dependency resolver before writing.

Archive entries are sorted portable paths with raw file bytes, uid/gid and names cleared, mtime zero, and mode normalized from the Git index to `0644` or `0755`. Directories are implicit. Gzip has timestamp zero and no filename. Symlinks, special files, path traversal, hidden controls, case-folding collisions, oversized files/trees, unsafe dependencies, and dirty input without acknowledgement fail before archive creation. `--list` performs the same validation and writes no archive. The byte-stable contract fixture is `tests/fixtures/contracts/agent-kit-1.2.0.aru-package.tar.gz`.

## Instruction projections

| Target | Directory-scoped AGENTS source | Explicit `apply-to` source |
| --- | --- | --- |
| Agents | Native source; no generated output | Rejected |
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

## Skill and MCP projections

| Target | Project skill destination | Project MCP destination |
| --- | --- | --- |
| Agents | `.agents/skills/<name>` | Rejected; no generic MCP format |
| Codex | `.agents/skills/<name>` | `.codex/config.toml` |
| Claude Code | `.claude/skills/<name>` | `.mcp.json` |
| GitHub Copilot CLI | `.github/skills/<name>` | `.github/mcp.json` |
| pi | `.pi/skills/<name>` | Rejected; no built-in MCP |
| OpenCode | `.opencode/skills/<name>` | `opencode.json` |
| Skill-only registry entry | Explicit registered project path | Rejected |

The skill target registry defines every skill-only destination and alias; no destination is derived from a target string at runtime.
Targets with the same skill destination share one independently owned projection while retaining complete canonical target reach in the lock.
When a selected target uses `.agents/skills/<name>` and another target has a distinct path on a platform with project symlink support, the other path links to the canonical `.agents` copy.
The link target is relative to the actual destination depth.
Otherwise each destination is a verified copy with the same semantic digest and an independent ownership entry.

Copilot MCP uses the `mcpServers` project format, `stdio` / `http` transport names, `${ENV}` references, and `tools = ["*"]`. This contract targets Copilot CLI; `.vscode/mcp.json` and GitHub.com repository MCP settings are not projections. OpenCode MCP uses `mcp` entries with `local` command arrays or `remote` URLs, `{env:ENV}` references, and `enabled = true`. Header-authenticated OpenCode remotes set `oauth = false` to avoid an unintended OAuth flow. `opencode.json` is parsed and edited as JSONC; unrelated keys, MCP entries, comments, trailing commas, and surrounding formatting survive.

## `aru.lock`

`version = 4`.
Version 4 adds plugin packages and explicit plugin origins on selected skill and MCP records.
A valid version 3 lock remains readable for inspection and upgrades deterministically during an unlocked `aru lock` or `aru sync`.
Check, locked, and frozen modes do not rewrite version 3 and report the required unlocked upgrade command.
Each `instruction-source` locks a portable source path, normalized scope, sorted selected targets, source SHA-256, and whether aru must project package-owned native content.
`aru sync --locked` compares discovered sources exactly and rejects changed content, scope, targets, or adapter schema.

Each `aru-package` locks its canonical source, requirement descriptor, selected Git version and full revision, declared name/version, manifest and complete package-tree digests, effective targets, dependency source identities, and exported instruction/skill/MCP records.

Each `plugin-package` locks its stable name, canonical Git source, requirement, selected source version and revision, declared plugin version, format, adapter version, subdirectory, complete plugin-tree digest, contributing manifest paths and digests, selection intent, targets, selected resources, and unsupported capability inventory.
Plugin-derived locked skills and MCP servers carry `{kind = "plugin", name, source}` origin identity.
Cycles, missing nodes, incomplete flattened exports, duplicate package names, and graph limit overflow invalidate the lock.

Each `skill-package` locks a normalized source, original requirement descriptor, selected SemVer/branch/revision label, full 40-hex commit, repository root name, sorted effective targets, and selected `{name,path,sha256}` entries. Branch requirements use `branch:<name>` while `revision` remains immutable. Each `mcp-server` locks exact normalized metadata and one concrete projection for each effective dependency target. Registry package projections lock npm/`npx` or approved PyPI/`uvx` command arrays plus exact package registry, identifier, and version. Direct stdio entries lock their exact command and ordered argv with `version = "direct"`; they contain no resolved package metadata.

`projection-input-hash` covers complete lock identity, sorted project targets, and adapter capability schema. `projection-baseline` contains only currently desired semantic instruction, skill, and MCP entries. It can bootstrap ownership after state loss but cannot authorize historical deletion.

Skill and MCP destinations follow the table above. Codex and OpenCode MCP projections explicitly set `enabled = true` so a declared project server overrides a disabled same-name higher-precedence entry. Targets without a requested capability are filtered out before dependency resolution; a declared requirement with no capable effective target fails explicitly.

The canonical skill digest byte stream is:

1. ASCII `aru-skill-digest-v1` plus NUL;
2. for each regular file sorted by portable `/` path: big-endian u64 path length, path bytes, one executable-marker byte, big-endian u64 content length, and raw content bytes.

Directories add no direct digest record. Symlinks and special files are rejected.

## `.aru/state.toml`

`version = 1`. Each `entry` has project-relative destination, kind/key, actual deployment mode (`copy`, `symlink`, `merge`, or `file`), last-applied semantic digest, and complete owning lock identity. Instruction `merge` entries track one source block; instruction `file` entries track a whole generated path. State proves local ownership but never replaces the committed baseline.

## CycloneDX 1.5 inventory

`aru export --format cyclonedx1.5` serializes the existing validated lock to JSON. It does not read `aru.toml`, resolve or fetch sources, rehash source trees, or claim that the lock is current for uncommitted manifest edits.

The document has `bomFormat = "CycloneDX"`, `specVersion = "1.5"`, BOM `version = 1`, an `aru:root` application component, sorted components, and a root dependency relationship to every component.
Native aru packages, plugin packages, and skill packages are `library` components, instruction sources are `data` components, and MCP servers are `application` components.
Package dependency edges preserve parent relationships, and plugin edges identify selected skill and MCP resources.
Stable `aru:*` properties carry only lock evidence such as kind, exact revision, requirement descriptor, exports, transports, and effective targets.

The metadata property `aru:document-purpose = "inventory"` distinguishes the output from an attestation. Unknown license, vulnerability, provenance, and registry facts are omitted rather than inferred. External URL credentials are removed. An invalid URL, malformed lock, or missing lock fails the complete export.

Timestamp is omitted by default. `--timestamp` accepts exactly an RFC 3339 UTC value such as `2026-07-31T00:00:00Z`; identical lock and timestamp inputs are byte-stable. `--output-file` is the only export option that writes a file.

The byte-stable contract fixture is `tests/fixtures/contracts/cyclonedx-1.5.json`. SPDX is not part of this initial export contract.

## Package graph and metadata JSON

`aru tree --format json` emits graph `version = 1` with sorted `roots`, package `nodes`, and `{from,to}` dependency `edges`. `--depth` and `--target` filter both nodes and edges. Package source credentials are removed; malformed source URLs fail the complete output.

`aru metadata --format-version 1` emits `format_version = 1`, the portable project root, declared targets, lock version, sorted package roots/nodes/edges, flattened instruction/skill/MCP inventory, and committed projection ownership identities.
Its JSON shape remains unchanged when plugin-derived resources fit those existing fields.
`aru metadata --format-version 2` adds sorted plugin records and explicit plugin origins on skill and MCP resources.
Metadata validates and reads `aru.toml` and `aru.lock`: no source resolution, fetch, rehash, ownership operation, recovery, or target write occurs.
URL credentials are removed and no secret values are present.
`--no-deps` retains package graph roots and direct resources while omitting transitive native-package nodes and edges.
Unsupported or omitted format versions fail.

The normalized byte-stable fixture is `tests/fixtures/contracts/metadata-v1.json`, with its environment-specific `project_root` replaced by `<PROJECT>`.

## Audit report JSON

`aru audit --format json` emits schema `version = 1` as a JSON number and a top-level `status` of `passed` or `failed`. `findings` are sorted by severity, code, path, line, column, and message. Each finding contains:

- a stable dotted `code`;
- `severity` (`error`, `warning`, or `info`);
- a human-readable `message`;
- optional portable `path`, one-based `line`, and one-based `column`; and
- optional remediation `help`.

Errors are blocking and make the command exit non-zero. Warnings and informational findings do not. JSON is written only to stdout or an explicit `--output` path; status remains on stderr. Audit performs no network access, creates no operation lock, does not recover pending transactions, and never changes manifest, lock, cache, ownership state, or target paths.
It verifies locked plugin cache, tree, and contributing manifest identity directly and reports missing or altered immutable shards.

Content audit treats ordinary Unicode, multilingual text, and emoji as valid. It reports bidi embedding/override/isolate controls, direction marks, zero-width format controls, invisible mathematical operators, and U+FEFF. Scanning is bounded to 100,000 deployed skill files, 64 MiB total, and 4 MiB per candidate text file. Instruction discovery retains its stricter source limits.

Skill and MCP `update --dry-run` output includes a sorted `Resolved` record for every selected dependency. The record shows the current and candidate version/revision or explicitly marks the candidate unchanged. These records are human status, not a persisted format.

## `.aru/transaction.toml`

`version = 1`, phase (`prepared`, `applying`, or `committed`), and ordered entries. Each entry records project-relative destination/stage/backup paths, optional old/new physical digests, and whether journal persistence observed apply. Secret data and file bytes are never journaled.

On recovery, only old/new digest matches are actionable. Unknown content stops recovery and remains untouched. User-owned instruction sources are never transaction destinations.

Golden fixtures are under `tests/fixtures/contracts/` and `tests/fixtures/instructions/` and are parsed or rendered by the normal test suite.
