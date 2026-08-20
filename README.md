# aru

`aru` keeps coding-agent instructions, Agent Skills, and MCP servers consistent across a project.

> [!WARNING]
> **Work in progress:** `aru` is under active development. Features, behavior, and file formats may change without notice.

Declare what your project needs in `aru.toml`, pin the exact result in `aru.lock`, then use `aru sync` to safely reconcile each agent's project files.

[Read the documentation](https://narumiruna.github.io/aru/) · [View the source](https://github.com/narumiruna/aru)

## ✨ At a glance

| Resource | Source of truth | Target result |
| --- | --- | --- |
| Native packages | Git repositories with a package-mode `aru.toml` | Composed instructions, skills, trusted MCP, and package dependencies |
| Instructions | Existing `AGENTS.md` files or configured Markdown sources | Native use, Claude imports/rules, or Copilot instructions |
| Agent Skills | Git repositories | Native project skill directories for every supported target |
| MCP servers | Registry packages, HTTPS endpoints, or stdio argv | Codex, Claude, Copilot CLI, and OpenCode MCP configuration |

Aru is designed to be reproducible and fail closed: it validates the complete operation before writing, does not execute configured MCP commands, and preserves drifted or unowned content for review.

## 🧭 Contents

- [Install](#install)
- [Quick start](#quick-start)
- [Manage native packages](#manage-native-packages)
- [Manage instructions](#manage-instructions)
- [Manage skills](#manage-skills)
- [Manage MCP servers](#manage-mcp-servers)
- [Manage targets](#manage-targets)
- [Lock and sync](#lock-and-sync)
- [Inspect packages and metadata](#inspect-packages-and-metadata)
- [Audit project integrity](#audit-project-integrity)
- [Export locked inventory](#export-locked-inventory)
- [Project files](#project-files)
- [Target capabilities](#target-capabilities)
- [Safety and recovery](#safety-and-recovery)
- [Offline demo](#offline-demo)
- [Development](#development)

<a id="install"></a>

## 📦 Install

You need a system `git` executable to use aru. The standalone installers download a prebuilt, checksum-verified binary and do not require Rust.

On macOS or Linux, install the latest release into `~/.local/bin/aru` with either curl or wget:

```console
curl -LsSf https://raw.githubusercontent.com/narumiruna/aru/main/install.sh | sh
wget -qO- https://raw.githubusercontent.com/narumiruna/aru/main/install.sh | sh
```

On 64-bit Windows, install into `~/.local/bin/aru.exe` with Windows PowerShell:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://raw.githubusercontent.com/narumiruna/aru/main/install.ps1 | iex"
```

The installers currently support x86-64 Linux, Intel and Apple Silicon macOS, and x86-64 Windows. Set `ARU_VERSION` to install a specific stable version or `ARU_INSTALL_DIR` to choose another directory. If the destination is not already on `PATH`, the installer prints the path to add.

Alternatively, install the latest release from [crates.io](https://crates.io/crates/aru) with a Rust toolchain:

```console
cargo install aru
```

Or install from a source checkout:

```console
cargo install --path .
```

Verify the installation:

```console
aru --help
```

### Self-update

Binaries installed by the standalone curl, wget, or PowerShell installers can update themselves to the latest stable GitHub Release without an `aru.toml` project:

```console
aru self update
```

Download and checksum-verify the complete update without replacing the current executable:

```console
aru self update --dry-run
```

Self-update is disabled for Cargo-built binaries so aru does not overwrite package-manager-owned files. Update those installations with `cargo install aru --locked` instead. `--offline` and `--frozen` reject self-update before network access.

### Shell completions

Generate a completion script from the current command tree and install it in your shell's user completion directory:

**Bash** (requires `bash-completion`):

```console
mkdir -p ~/.local/share/bash-completion/completions
aru generate-shell-completion bash > ~/.local/share/bash-completion/completions/aru
```

**Zsh**:

```console
mkdir -p ~/.zfunc
aru generate-shell-completion zsh > ~/.zfunc/_aru
```

Add `fpath=(~/.zfunc $fpath)` to `.zshrc` before `autoload -Uz compinit && compinit`.

**Fish**:

```console
mkdir -p ~/.config/fish/completions
aru generate-shell-completion fish > ~/.config/fish/completions/aru.fish
```

The command writes only the script to stdout and does not require an `aru.toml` project.

Aru passes Git arguments directly and uses your existing SSH or credential-helper configuration. It never invokes a shell for Git operations.

<a id="quick-start"></a>

## 🚀 Quick start

### 1. Initialize a project

Initialize the current directory, or pass another existing directory as `PATH`:

```console
aru init --target codex --target claude --target copilot
aru init PATH --target codex
```

This creates the project manifest and configures the initial target set. Supported target names are `codex`, `claude`, `copilot`, `pi`, and `opencode`.

### 2. Adopt existing instructions

Preview discovery before changing project files:

```console
aru instruction add --discover --dry-run
```

Then apply it:

```console
aru instruction add --discover
```

> [!IMPORTANT]
> Before 1.0, `aru instruction init` was replaced directly by `aru instruction add --discover`; there is no compatibility alias.

If an unmanaged `CLAUDE.md` or `.github/copilot-instructions.md` already exists, aru reports a collision. Choose one explicit strategy:

```console
aru instruction add --discover --merge  # preserve Markdown and add owned blocks
aru instruction add --discover --force  # replace unmanaged instruction output
```

> [!CAUTION]
> `--force` is destructive takeover. Use it only when aru should replace the colliding unmanaged key or path. A later remove will not restore the old content.

### 3. Add a skill

```console
aru skill add narumiruna/skills
```

In an interactive terminal, aru opens a searchable multi-select menu:

| Input | Action |
| --- | --- |
| Type | Filter the list |
| <kbd>↑</kbd> / <kbd>↓</kbd> | Move through results |
| <kbd>Space</kbd> | Toggle an item |
| <kbd>→</kbd> / <kbd>←</kbd> | Select all / none |
| <kbd>Enter</kbd> | Confirm |
| <kbd>Esc</kbd> | Cancel without changing project files |

Verify that the committed lock can be replayed, then check local synchronization without writing:

```console
aru sync --locked
aru sync --check
```

You now have:

- **`aru.toml`** — human-maintained project intent
- **`aru.lock`** — exact, reproducible resolutions and projections
- **Target files** — reconciled instructions, skills, and MCP configuration

<a id="manage-native-packages"></a>

## 📦 Manage native packages

A native aru package is a Git repository with a root `aru.toml` containing `[package]` metadata. It can export embedded instructions, skills, trusted MCP declarations, and bounded transitive package dependencies.

```console
aru add owner/agent-kit
aru add owner/agent-kit --version '^1.2'
aru add ../local-agent-kit --target codex --target claude
aru add owner/mcp-kit --trust-mcp docs
```

`add` updates `aru.toml`, resolves the complete package graph into `aru.lock`, and synchronizes projections by default. Use `--no-sync` to defer target paths or `--dry-run` to use temporary storage and write nothing. Package instructions that need to join an existing unmanaged `AGENTS.md` require explicit `--merge`; `--force` remains destructive takeover.

Compatible lock nodes remain pinned during ordinary sync. Preview or apply updates with Cargo-style selection:

```console
aru update --dry-run
aru update
aru update owner/agent-kit
aru update owner/agent-kit --precise 1.2.3
aru remove owner/agent-kit
```

Package-provided MCP servers are denied by default. `--trust-mcp NAME` records a root, credential-free decision for that package source; it never bypasses transport, target, secret, or ownership validation. Transitive local paths, scripts, hooks, duplicate exports, cycles, conflicting source requirements, unsupported targets, and oversized graphs fail before project writes.

Package authors can validate inventory and build a deterministic archive from a clean package Git root:

```console
aru package --list
aru package
aru package --output dist/agent-kit.tar.gz
aru package --allow-dirty
```

`package` validates the current package-mode `aru.toml`, archive paths/content, and dependency graph before writing. It includes tracked and non-ignored files, rejects symlinks/special files/case collisions/hidden controls, and normalizes tar ordering, timestamps, ownership, and permissions. Dirty input requires explicit `--allow-dirty`; `.aru/`, ignored files, and `target/aru-package/` are excluded.

See the complete [`aru.toml` package, graph, trust, and archive contracts](docs/formats.md#native-aru-packages).

<a id="manage-instructions"></a>

## 📝 Manage instructions

Existing `AGENTS.md` files remain canonical and user-owned. Aru records their paths without moving, editing, or removing them.

Configure instruction sources in `aru.toml`:

```toml
[[instructions.sources]]
files = ["AGENTS.md", "src/**/AGENTS.md"]
exclude = ["target/**", "third_party/**"]
scope = "source-directory"

[[instructions.sources]]
files = ["docs/instructions/rust.md"]
apply-to = ["**/*.rs"]
targets = ["claude", "copilot"]
```

Each source uses exactly one scoping model:

- `scope = "source-directory"` requires matched files named `AGENTS.md`. Each file applies to its own directory tree.
- `apply-to` declares exact, repository-relative globs for targets that can preserve path-specific rules.

The optional `targets` list defaults to all project targets and must be a subset of them. All patterns are project-relative; `.git/**` and `.aru/**` are always excluded.

Aru projects instructions according to target capabilities:

- **Codex, pi, and OpenCode** consume directory-scoped `AGENTS.md` files directly, so aru does not duplicate them.
- **Claude Code** receives sibling `CLAUDE.md` import blocks and path rules under `.claude/rules/aru/`.
- **GitHub Copilot** receives a root `.github/copilot-instructions.md` block and path-specific files under `.github/instructions/aru/`.

Unsupported scope and target combinations fail before any write. Removing a source or target removes only digest-matching, aru-owned output; drifted or unowned content is preserved for review.

List declared file selectors or remove exact selectors without touching canonical source files:

```console
aru instruction list
aru instruction remove AGENTS.md --dry-run
aru instruction remove AGENTS.md
```

Use `--no-sync` on add or remove to update only `aru.toml` and `aru.lock`; aru reports that target paths remain pending.

<a id="manage-skills"></a>

## 🧩 Manage skills

### Select exports

Use one selection mode per `skill add` command:

```console
aru skill add owner/repository                    # interactive selection
aru skill add owner/repository --all              # every current and future export
aru skill add owner/repository -a                 # short form of --all
aru skill add owner/repository --skill review \
  --skill applying-tdd                            # explicit, repeatable names
aru skill add owner/repository --path extras/review # non-standard layout
aru skill add owner/repository --all --target codex  # narrow deployment
```

In a pipe, redirected shell, or CI runner, a bare `skill add` fails before fetching. Pass `--all`, `--skill`, or `--path` explicitly. At least one skill must be selected.

Selection behavior is deliberate:

- `--all` stores wildcard intent and tracks valid exports added by future versions.
- Interactive selection stores an explicit snapshot, even when every visible export is selected.
- Reopening a wildcard preserves it when all exports remain selected; deselecting one converts it to an explicit snapshot.
- Repeated `--skill` selections are additive.
- Removing the final explicit skill removes the package.
- A `--path` selection remains in `aru.toml` until explicitly removed.

### Pin a source revision

```console
aru skill add owner/repository --version 0.5.0
aru skill add owner/repository --version '=0.5.0'
aru skill add owner/repository --branch main
aru skill add ssh://git@example.com/team/skills.git \
  --rev 67cd354 --skill review
```

| Option | Resolution behavior |
| --- | --- |
| No reference option | Latest matching SemVer tag; falls back to `main` when none exists |
| `--version 0.5.0` | Cargo caret semantics: `^0.5.0` |
| `--version '=0.5.0'` | Exact tag |
| `--branch main` | Current branch head, pinned to an exact commit in `aru.lock` |
| `--rev <SHA>` | Immutable 7–40 character commit |

`--version`, `--branch`, and `--rev` are mutually exclusive.
Source arguments contain neither a path selector nor a reference, so SCP-like `git@host:path` sources remain unambiguous.
Equivalent canonical Git sources do not create duplicate packages.
The `main` fallback applies only when no reference option is provided; an explicit `--version` remains strict.
The fallback commit is pinned in `aru.lock`, and a later update prefers a matching SemVer tag if the repository adds one.

Use `--upgrade` / `-U` during add to re-resolve an existing source instead of reusing its lock:

```console
aru skill add owner/repository --skill review --upgrade
```

A new source installs normally. An existing SemVer source selects the latest matching tag, a branch selects its current head, and an exact revision remains fixed.

### Update or remove

```console
aru skill list
aru skill update --dry-run               # preview current and candidate revisions
aru skill update                         # update all eligible sources
aru skill update owner/repository        # update one source
aru skill remove owner/repository --skill review
aru skill remove owner/repository        # remove the source
```

`skill list` writes deterministic tab-separated `name`, locked version, and canonical source records to stdout. Repeat `--target` to narrow a skill source to configured, skill-capable targets. Omit it to use every compatible project target.

Skills project to each target's native project directory: `.agents/skills/` for Codex, `.claude/skills/` for Claude, `.github/skills/` for Copilot, `.pi/skills/` for pi, and `.opencode/skills/` for OpenCode. On Unix, when Codex receives the same skill, the other native paths link to the canonical `.agents` copy; platforms without project symlinks receive verified copies.

Ordinary `aru sync` and `aru sync --locked` keep a branch's locked commit. Run `skill update` or add with `--upgrade` to move it. Because force-pushes can make old commits unreachable, prefer immutable SemVer tags for published, long-lived configurations.

<a id="manage-mcp-servers"></a>

## 🔌 Manage MCP servers

Aru accepts three MCP source types.

### Registry package

```console
aru mcp add io.example/context \
  --name context \
  --transport stdio \
  --package-registry npm
```

Registry packages support npm records rendered through exact `npx` argv and PyPI records that explicitly declare the `uvx` runtime hint. Select PyPI with `--package-registry pypi`. A Registry candidate must be unique after transport, package-registry, and configured-target capability filtering; aru never chooses the first API result when multiple candidates remain. Cargo, OCI, NuGet, MCPB, unknown runtime hints, secret arguments, and unresolved required arguments remain fail-closed. Use a pinned direct stdio declaration when you need an argv shape the Registry cannot represent safely.

### Direct HTTPS endpoint

```console
aru mcp add \
  --url https://docs.example.com/mcp \
  --name docs \
  --bearer-token-env DOCS_MCP_TOKEN \
  --header-env X-Workspace=DOCS_MCP_WORKSPACE
```

Repeat `--header-env HEADER=ENV` for non-Authorization headers whose values come from the environment. Header names are case-insensitive and cannot collide; use `--bearer-token-env` rather than a second `Authorization` declaration. Aru stores only each environment variable name or target placeholder, never its value. Repeat `--target` to narrow any MCP declaration to configured MCP-capable targets:

```console
aru mcp add \
  --url https://docs.example.com/mcp \
  --name docs \
  --target claude
```

### Direct stdio command

```console
aru mcp add \
  --command uvx \
  --arg=--with \
  --arg 'mcp<2' \
  --arg yfmcp@0.12.2 \
  --env-var YFINANCE_API_KEY \
  --name yfinance
```

Repeat `--env-var NAME` to forward an existing environment variable without reading it. Aru stores the executable and each repeated `--arg` as an ordered argv array. Use `--arg=--flag` when an argument begins with `-`. Aru projects these commands but never executes them during add, lock, or sync.

Pin package versions explicitly in argv. The example constrains the unbounded MCP SDK dependency because `yfmcp` 0.12.2 uses the 1.x API.

Update or remove servers by project-local name:

```console
aru mcp list
aru mcp update --dry-run
aru mcp update context
aru mcp remove docs
```

`mcp list` writes deterministic tab-separated name, source type, and transport records to stdout. `mcp update [name]` unlocks only selected Registry packages. Direct URLs and direct stdio commands do not have Registry versions to upgrade.

Project MCP is supported for Codex, Claude, Copilot CLI, and OpenCode. pi intentionally has no built-in MCP and is rejected as an MCP dependency target. Copilot uses the shared repository file `.github/mcp.json`; aru does not emit VS Code's incompatible `.vscode/mcp.json` or modify GitHub.com repository settings. OpenCode's `opencode.json` is edited as JSONC so unrelated settings, comments, and formatting survive.

<a id="manage-targets"></a>

## 🎯 Manage targets

Targets are the coding-agent project layouts that aru reconciles.

```console
aru target list
aru target add claude
aru target remove codex
aru target set codex claude
```

- `add` performs a set union.
- `remove` performs a set subtraction.
- `set` replaces the complete set atomically.
- Repeating an already configured target is safe.
- Unknown targets and unconfigured removal arguments fail before writes.

At least one target must remain. To switch a Codex-only project to Claude, run `aru target set claude` instead of trying to remove the only target first.

Target mutations resolve the complete result before atomically updating `aru.toml`, `aru.lock`, ownership state, and owned project paths. They preserve locked package versions and support `--dry-run`. `target add` and `target set` also support `--merge` and `--force` for new-target instruction collisions.

Use `--no-sync` to update only manifest and lock intent, then run `aru sync` later. If local ownership state is missing and deferral would lose information required for a Claude copy/symlink conversion, aru rejects `--no-sync` and asks you to apply the change directly.

<a id="lock-and-sync"></a>

## 🔒 Lock and sync

| Command | Behavior |
| --- | --- |
| `aru lock` | Resolve instructions and packages; update `aru.lock` without changing target paths |
| `aru lock --check` | Check that the existing lock is complete and current without writing or using the network |
| `aru sync` | Reuse compatible locked packages, fill missing lock data, and reconcile target paths |
| `aru sync --locked` | Require a complete, current lock; never change it or advance a branch |
| `aru sync --check` | Check the lock and all target paths locally without writing |
| `aru sync --dry-run` | Print the deterministic plan without changing persistent project state |

`--dry-run` / `-n` may read Git or HTTP sources through temporary storage. It does not modify `aru.toml`, `aru.lock`, `.aru/`, or target paths.

`--no-sync` on add, remove, or update still resolves and transactionally updates `aru.toml` and `aru.lock`; it skips only target projections and prints the command needed to apply them later.

### Common uv/Cargo-style options

| Option | Behavior |
| --- | --- |
| `--project <PATH>` | Discover the aru project from a specific directory |
| `--locked` | Fail if the command would change `aru.lock` |
| `--offline` | Disable remote Git and Registry access; use local sources or cached locks |
| `--frozen` | Equivalent to `--locked --offline` |
| `-q`, `--quiet` | Suppress normal status output, but keep errors and actionable warnings |
| `-v`, `--verbose` | Show resolved revisions and digests; repeat for projection identity detail |
| `--color auto\|always\|never` | Control color in human-readable status output |
| `--no-progress` | Hide static TTY resolution progress |

Aru sends list data to stdout and human-readable status to stderr. Normal status uses Cargo-style verbs and omits full digests:

```text
      Locked skill review 1.2.0
     Created skill review (.agents/skills/review)
     Updated aru.toml
     Updated aru.lock
    Finished Project synchronized.
```

Use `-v` when exact revisions and digests are needed. Status meaning never depends on color.

Changing `project.targets` by hand does not unlock package versions, but it invalidates the projection hash. A locked sync will fail until a normal sync updates per-target projections. Prefer `aru target add`, `remove`, or `set` so all related state changes in one transaction.

<a id="inspect-packages-and-metadata"></a>

## 🌳 Inspect packages and metadata

Inspection commands read validated lock evidence and write no project state:

```console
aru tree
aru tree --depth 2 --target claude
aru tree --invert shared-rules
aru tree --format json
aru info agent-kit
aru metadata --format-version 1
aru metadata --format-version 1 --no-deps
```

`tree` renders a deterministic deduplicated package graph; `--invert` shows reverse dependencies. `info` uses exact locked evidence when installed and can inspect an undeclared Git package through bounded temporary storage. `metadata` requires an explicit contract version, emits JSON only to stdout, removes URL credentials, and never fetches; `--no-deps` limits package records to direct graph roots.

<a id="audit-project-integrity"></a>

## 🔍 Audit project integrity

Run a detailed, local integrity review without changing project or cache state:

```console
aru audit
aru audit --format json
aru audit --format json --output audit.json
```

Audit checks manifest and lock consistency, pending recovery, ownership references, target projection drift, deployed skill content, and hidden Unicode format controls in instructions and deployed skills. Ordinary multilingual text and emoji are accepted. Bidi controls, zero-width format controls, and unexpected byte-order marks are blocking findings with exact path, line, column, and code point.

`aru sync --check` remains the concise exact-state gate. `aru audit` provides versioned, sorted findings and remediation guidance. It is non-interactive, uses no network, never repairs state, and exits non-zero when blocking findings exist. JSON schema version 1 is written to stdout unless `--output` is explicit; human findings and status use stderr.

<a id="export-locked-inventory"></a>

## 📤 Export locked inventory

Export the existing lock as a deterministic CycloneDX 1.5 inventory:

```console
aru export --format cyclonedx1.5
aru export --format cyclonedx1.5 --output-file sbom.json
aru export --format cyclonedx1.5 --timestamp 2026-07-31T00:00:00Z
```

Export reads and validates `aru.lock` only. It performs no resolution, source fetch, source rehash, ownership update, or target write. Components and relationships are sorted, URL credentials are removed, and unknown or invalid URLs fail rather than produce a partial inventory. Omit `--timestamp` for timestamp-free deterministic output, or provide an RFC 3339 UTC value for byte-stable metadata.

The result is a dependency inventory, not a vulnerability, license, provenance, or security attestation. SPDX is not part of the first export contract.

<a id="project-files"></a>

## 📂 Project files

| Path | Commit? | Purpose |
| --- | --- | --- |
| `aru.toml` | Yes | Human-maintained sources, package requirements, selectors, and targets |
| `aru.lock` | Yes | Exact Git commits, MCP metadata, source digests, projections, and portable ownership baseline |
| `AGENTS.md`, `**/AGENTS.md` | Yes | User-owned, directory-scoped instruction sources |
| `CLAUDE.md`, `**/CLAUDE.md` | Optional | Claude imports managed as source-specific marker blocks |
| `.claude/rules/aru/**` | Optional | Aru-owned Claude path-specific instruction projections |
| `.github/copilot-instructions.md` | Optional | Copilot root instructions with source-specific marker blocks |
| `.github/instructions/aru/**` | Optional | Aru-owned Copilot path-specific instruction projections |
| `.agents/skills/<name>` | Optional | Codex project skill projection |
| `.claude/skills/<name>` | Optional | Claude skill link to `.agents`, when possible, or a verified copy |
| `.github/skills/<name>` | Optional | Copilot skill link to `.agents`, when possible, or a verified copy |
| `.pi/skills/<name>` | Optional | pi skill link to `.agents`, when possible, or a verified copy |
| `.opencode/skills/<name>` | Optional | OpenCode skill link to `.agents`, when possible, or a verified copy |
| `.codex/config.toml` | Optional | Codex project MCP entries |
| `.mcp.json` | Optional | Claude Code project MCP entries |
| `.github/mcp.json` | Optional | GitHub Copilot CLI project MCP entries |
| `opencode.json` | Optional | OpenCode project config with managed MCP entries |
| `.aru/cache/` | No | Immutable, content-addressed Git checkouts |
| `.aru/state.toml` | No | Local deployment mode and last-applied ownership digests |
| `.aru/transaction.toml` | No | Crash-recovery journal, present only during an interrupted operation |

`aru init` adds `.aru/` to `.gitignore`. It does not ignore generated target paths, because each team can decide whether to commit them. During early development, `aru.toml` is intentionally unversioned and has no schema field.

For byte-level details about persisted data, see [`docs/formats.md`](docs/formats.md).

<a id="target-capabilities"></a>

## 🤖 Target capabilities

| Capability | Codex | Claude Code | Copilot CLI | pi | OpenCode |
| --- | --- | --- | --- | --- | --- |
| Directory `AGENTS.md` | Native | Sibling import | Materialized root/path rule | Native | Native |
| Explicit instruction globs | Not supported | `.claude/rules/aru/**` | `.github/instructions/aru/**` | Not supported | Not supported |
| Project skills | `.agents/skills/` | `.claude/skills/` | `.github/skills/` | `.pi/skills/` | `.opencode/skills/` |
| stdio MCP | `command`, `args`, `env_vars` | `type: stdio`, `command`, `args`, `${ENV}` | `type: stdio`, `mcpServers`, `${ENV}` | Not built in | `type: local`, command array, `{env:ENV}` |
| Streamable HTTP MCP | `url` | `type: http`, `url` | `type: http`, `mcpServers` | Not built in | `type: remote`, `url` |
| Bearer environment reference | `bearer_token_env_var` | `Authorization: Bearer ${ENV}` | `Authorization: Bearer ${ENV}` | Not built in | `Authorization: Bearer {env:ENV}` |
| Environment-backed HTTP headers | `env_http_headers` | `${ENV}` header value | `${ENV}` header value | Not built in | `{env:ENV}` header value |

Aru rejects configurations that a selected target cannot represent rather than silently broadening or dropping behavior. Copilot MCP support is scoped to Copilot CLI's `.github/mcp.json`; VS Code's `.vscode/mcp.json` and GitHub.com MCP settings are separate contracts. See the [original Codex/Claude MCP evidence](docs/spikes/2026-07-30_mcp-registry-target-capabilities.md) and [additional target capability evidence](docs/spikes/2026-07-31_additional-target-capabilities.md).

<a id="safety-and-recovery"></a>

## 🛡️ Safety and recovery

### Bounded, portable inputs

Aru treats instruction content, skill content, and Registry metadata as untrusted.

- Conventional discovery scans only the source root and `skills/**/SKILL.md`, up to depth 6, 2,000 directories, and 20,000 entries.
- `SKILL.md` is limited to 1 MiB, each selected regular file to 10 MiB, and each skill tree to 100 MiB.
- Symlinks, devices, sockets, FIFOs, non-UTF-8 paths, case-folding collisions, Windows reserved names, and escaping selectors are rejected.
- Digests include a format version, portable path, executable marker, byte length, and raw bytes for every sorted regular file.
- Registry requests require HTTPS without URL userinfo. They use 10-second connect and 30-second total timeouts, at most three redirects, 10 MiB bodies, 100 pages, and 10,000 records.
- Malformed, oversized, truncated, cyclic, ambiguous, inactive, or unsupported metadata fails before writes.

### Ownership protection

- MCP commands and arguments remain argv arrays. Aru never shell-expands or executes direct stdio commands.
- Aru stores only secret environment variable names or placeholders. It never reads secret values.
- Unmanaged instruction destinations and skill or MCP entries collide by default.
- `--merge` preserves unmanaged Markdown only around aru-owned instruction blocks.
- Drifted owned entries are preserved and reported instead of overwritten.
- Only aru-owned server keys are merged or removed. Unrelated TOML comments, TOML keys, JSON entries, and OpenCode JSONC comments survive.
- Invalid existing configuration fails closed.

### Atomic transactions

Every mutating command:

1. takes `.aru/operation.lock`;
2. rereads project inputs and validates the complete operation;
3. stages each destination beside its final path;
4. writes a durable journal;
5. performs fixed-order atomic replacements with sibling backups.

A normal apply error immediately rolls back.

After a process kill or power loss, run any mutating command again:

```console
aru sync
```

Before doing new work, aru reads `.aru/transaction.toml` and digest-gates a deterministic rollback to the complete old state. A dry run refuses to proceed while recovery is pending.

If a destination or backup contains unknown manual changes, recovery stops and preserves both the content and journal. Copy the affected project file and `.aru/transaction.toml` before manual repair. Do not delete backups until you understand whether each digest represents the old or new state.

If `.aru/state.toml` is lost, `aru sync --locked` adopts only entries whose semantic digests exactly match the committed `projection-baseline`. A different same-name entry remains an unmanaged collision. Baselines never authorize deletion of unknown historical orphans.

<a id="offline-demo"></a>

## 🧪 Offline demo

This example exercises initialization, add, locked replay, update, and remove without network access:

```console
mkdir -p /tmp/aru-demo-source/skills/demo /tmp/aru-demo-project
printf '%s\n' '---' 'name: demo' 'description: Demo skill' '---' '# Demo' \
  > /tmp/aru-demo-source/skills/demo/SKILL.md
git -C /tmp/aru-demo-source init -q
git -C /tmp/aru-demo-source config user.email demo@example.com
git -C /tmp/aru-demo-source config user.name Demo
git -C /tmp/aru-demo-source add skills/demo/SKILL.md
git -C /tmp/aru-demo-source commit -qm initial
git -C /tmp/aru-demo-source tag 1.0.0

cd /tmp/aru-demo-project
aru init --target codex --target claude
aru skill add /tmp/aru-demo-source --skill demo
aru --offline sync --locked
aru skill update /tmp/aru-demo-source
aru skill remove /tmp/aru-demo-source
```

Representative output is deterministic:

```text
     Created skill demo (.agents/skills/demo)
     Created skill demo (.claude/skills/demo)
      Locked skill demo 1.0.0
     Updated aru.lock
    Finished Project synchronized.
```

<a id="development"></a>

## 🛠️ Development

Serve or build the documentation with its locked toolchain:

```console
just docs-serve
just docs-build
```

Run the CI-equivalent Rust checks:

```console
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Tests use temporary Git repositories, local Registry fixtures, and Unix PTYs (`expectrl`) for the real `inquire` keyboard flow. Live network access is reserved for an explicit, ignored public Git smoke test.

Release maintainers should follow [`docs/releasing.md`](docs/releasing.md). Aru is available under the [MIT License](LICENSE).
