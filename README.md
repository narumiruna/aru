# aru

> [!WARNING]
> **Work in progress (WIP):** `aru` is under active development. Features, behavior, and file formats may change without notice.

`aru` is a project-scoped manager for coding-agent instructions, skills, and MCP servers. `aru.toml` records intent, `aru.lock` pins exact source content and per-target projections, and `aru sync` safely reconciles Codex, Claude Code, GitHub Copilot, OpenCode, and pi project paths according to each target's implemented capabilities.

## Install

```console
cargo install --path .
aru --help
```

A system `git` executable is required. Git subprocesses use argument arrays and existing SSH or credential-helper configuration; aru never invokes a shell.

## Quick start

```console
aru init --target codex --target claude --target copilot
aru instruction init --dry-run
aru instruction init --merge
aru skill add narumiruna/skills
# Use the searchable menu to select writing-plans, then lock replay is non-interactive.
aru sync --locked
aru skill update narumiruna/skills
aru skill remove narumiruna/skills --skill writing-plans
```

`aru instruction init` discovers existing root and nested `AGENTS.md` files, records their exact paths without moving or editing them, and projects configured non-native targets. Use `--dry-run` to inspect the complete transaction. An existing unmanaged `CLAUDE.md` or `.github/copilot-instructions.md` collides by default; `--merge` preserves its content and adds source-specific aru blocks, while `--force` explicitly replaces unmanaged destination content; the two flags are mutually exclusive. Once established, ordinary `aru sync` changes only owned blocks or files.

In an interactive terminal, omitting selectors opens a searchable multi-select menu. Type to filter, use ↑/↓ to move, Space to toggle, →/← to select all/none, Enter to confirm, or Esc to cancel without changing project files. At least one skill must be selected.

Use `--all` / `-a` to track every current and future export without a prompt. Use repeatable `--skill` for explicit automation, or `--path` only for a non-standard repository layout:

```console
aru skill add owner/repository                 # interactive menu
aru skill add owner/repository --all           # non-interactive wildcard
aru skill add owner/repository -a              # short form
aru skill add owner/repository --skill review --skill applying-tdd
aru skill add owner/repository --skill review --upgrade
aru skill add owner/repository --path extras/review
aru skill add owner/repository --branch main       # opt in to a moving branch
aru skill add ssh://git@example.com/team/skills.git --rev 67cd354 --skill review
```

`--version 0.5.0` has Cargo caret semantics (`^0.5.0`); use `--version =0.5.0` for an exact tag. Use `--branch main` to explicitly track a moving branch, or `--rev <SHA>` for an immutable commit. `--version`, `--branch`, and `--rev` are mutually exclusive; omitting all three continues to select the latest matching SemVer tag, never the default branch. `--upgrade` / `-U` makes add re-resolve this source instead of reusing its lock: a new source installs normally, an existing SemVer source selects the latest matching tag, a branch selects its current head, and an exact revision remains fixed. Source positionals never contain a path selector or reference, so SCP-like `git@host:path` remains unambiguous. In a pipe, redirected shell, or CI runner, bare add fails before fetching; pass `--all`, `--skill`, or `--path` explicitly.

Add a Registry package or a direct remote MCP server:

```console
aru mcp add io.example/context --name context --transport stdio --package-registry npm
aru mcp add --url https://docs.example.com/mcp --name docs \
  --bearer-token-env DOCS_MCP_TOKEN
aru mcp update context
aru mcp remove docs
```

A Registry candidate must be unique after transport, package-registry, and filtering against every configured target with an implemented MCP adapter. aru never chooses the first API array item.

## Offline local example

The following example exercises init, add, lock replay, update, and remove without an external network:

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
aru sync --locked
aru skill update /tmp/aru-demo-source
aru skill remove /tmp/aru-demo-source
```

Representative output contains deterministic operations such as:

```text
create skill demo (.agents/skills/demo)
create skill demo (.claude/skills/demo)
lock skill demo 1.0.0 sha256:…
write lockfile
```

## Manage instructions

Existing `AGENTS.md` files are canonical, user-owned sources:

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

`scope = "source-directory"` requires matched files to be named `AGENTS.md`; each file applies to its own directory tree. `apply-to` declares exact repository-relative globs and must target only adapters that can preserve them. Source `targets` default to the complete project target set and, when specified, must be a subset of it. Patterns are project-relative; `.git/**` and `.aru/**` are always excluded.

Codex, pi, and OpenCode consume directory-scoped `AGENTS.md` directly, so aru writes no duplicate file for them. Claude receives sibling `CLAUDE.md` import blocks and `.claude/rules/aru/**` path rules. Copilot receives a root `.github/copilot-instructions.md` block and path-specific `.github/instructions/aru/**. Unsupported scope/target combinations fail before any write rather than broadening scope.

`aru lock` records source and projection digests without writing target paths. `aru sync --locked` rejects changed source content or stale target projections. Removing a source or target removes only digest-matching aru-owned blocks/files; drifted or unowned output is preserved for review.

## Manage targets

Targets are the persistent set of coding-agent project layouts that aru reconciles. List the set declared in `aru.toml`, extend or reduce it, or replace it exactly:

```console
aru target list
aru target add claude
aru target remove codex
aru target set codex claude
```

`add` performs a set union, `remove` performs a set subtraction, and `set` replaces the complete set atomically. At least one target must remain, so use `aru target set claude` to switch a Codex-only project without an intermediate two-target sync. Repeating an already configured target is safe. Unknown or unconfigured removal arguments fail before writes.

Target mutations resolve the complete result before atomically updating `aru.toml`, `aru.lock`, local ownership state, and owned project paths. They preserve locked package versions and accept `--dry-run`; `add` and `set` accept `--merge` for instruction blocks and `--force` for explicit takeover of new-target collisions. `--no-sync` updates only manifest and lock intent, prints that target paths are pending, and requires a later `aru sync`. If local ownership state is missing and deferral would lose the information needed for a Claude copy/symlink conversion, aru rejects `--no-sync` and asks you to apply the target change directly.

## Project files

| Path | Commit? | Purpose |
| --- | --- | --- |
| `aru.toml` | Yes | Human-maintained instruction sources, package requirements, selectors, and target list |
| `aru.lock` | Yes | Instruction source digests, exact Git commits, MCP metadata/candidates, per-target projections, and portable ownership baseline |
| `AGENTS.md`, `**/AGENTS.md` | Yes | User-owned canonical directory-scoped instruction sources; aru never edits or removes them |
| `CLAUDE.md`, `**/CLAUDE.md` | Optional | Claude imports managed as source-specific marker blocks |
| `.claude/rules/aru/**` | Optional | Aru-owned Claude path-specific instruction projections |
| `.github/copilot-instructions.md` | Optional | Copilot root instructions with source-specific marker blocks |
| `.github/instructions/aru/**` | Optional | Aru-owned Copilot path-specific instruction projections |
| `.agents/skills/<name>` | Optional | Codex project skill projection |
| `.claude/skills/<name>` | Optional | Claude project skill projection: a relative link to `.agents` when both targets are selected and links are available, otherwise a verified copy |
| `.codex/config.toml` | Optional | Codex project MCP entries |
| `.mcp.json` | Optional | Claude Code project MCP entries |
| `.aru/cache/` | No | Immutable, content-addressed Git checkouts |
| `.aru/state.toml` | No | Local deployment mode and last-applied ownership digests |
| `.aru/transaction.toml` | No | Crash-recovery journal, present only during an interrupted operation |

`aru init` adds `.aru/` to `.gitignore`. It does not ignore generated target paths because teams may choose to commit them. During early development, `aru.toml` is intentionally unversioned and contains no schema field.

### Manifest selection semantics

- Each `instructions.sources` declaration has non-empty `files`, optional `exclude` and `targets`, and exactly one of `scope = "source-directory"` or `apply-to`.
- Discovery is deterministic and bounded. Unsafe, escaping, symlinked, non-UTF-8, oversized, duplicate, generated-output, or reserved-marker sources fail before writes.
- `include = ["*"]` tracks every valid current and future export; only `--all` / `-a` creates new wildcard intent. `exclude` records per-skill removals.
- Interactive selection writes an explicit snapshot, even when every visible item is selected. Reopening the menu preselects current explicit entries and replaces that source's complete explicit selection on confirmation.
- Reopening an unchanged wildcard preselects all exports and preserves wildcard intent if all remain selected; deselecting any export converts it to an explicit snapshot.
- Explicit `--skill` additions remain additive. Removing the final explicit name removes the package.
- A `--path` selection always persists `paths.<name>` in `aru.toml` until explicitly removed.
- Equivalent canonical Git sources do not create duplicate packages.
- Registry versions, Git tags, branches, and revisions stay locked to exact identities during ordinary sync. `skill update [source]` re-resolves only selected tag/branch sources; `mcp update [name]` unlocks only selected MCP packages.
- Direct MCP URLs have no upgradable version.

### Lock and sync modes

- `aru lock` resolves instructions and packages and writes the lock without changing target project paths.
- `aru sync` reuses every compatible locked package, fills missing lock/projection data, and reconciles project paths.
- `aru sync --locked` rejects a missing or stale lock, including changed instruction sources and incomplete per-target projections. It never changes the lock or advances a branch.
- `--no-sync` on add/remove/update still resolves and transactionally updates `aru.toml` plus `aru.lock`; it only skips projections.
- `--dry-run` may read Git or HTTP sources through a temporary cache, but does not modify `aru.toml`, `aru.lock`, `.aru/`, or target paths.
- `--force` is destructive takeover of a colliding unmanaged key/path. The operation plan says `force replace`; a later remove does not restore the previous unmanaged value.

Changing only `project.targets` does not unlock package versions. It does invalidate the projection hash, so `--locked` fails until a normal sync adds or removes per-target projections. Prefer `aru target add`, `remove`, or `set` so the manifest, lock, ownership state, and target paths change in one transaction.

### Branch sources

Branch tracking is an explicit development-mode opt-in:

```console
aru skill add narumiruna/skills --branch main
aru skill add narumiruna/skills -U  # select again and move to the current head
aru skill update narumiruna/skills  # move the lock without changing selection
```

The manifest records `branch = "main"`, while `aru.lock` records `requirement = "branch:main"` and the exact 40-hex commit shown in the selection preview. Normal `aru sync` and `aru sync --locked` keep that SHA. A force-push can make an older locked commit unreachable from a clean checkout; use immutable SemVer tags for published, long-lived, reproducible installations.

## Target capability matrix

| Capability | Codex | Claude Code | Copilot | pi | OpenCode |
| --- | --- | --- | --- | --- | --- |
| Directory `AGENTS.md` | Native | Sibling import | Materialized root/path rule | Native | Native |
| Explicit instruction globs | No | `.claude/rules/aru/**` | `.github/instructions/aru/**` | No | No |
| Project skills | `.agents/skills/` | `.claude/skills/` | Not implemented | Not implemented | Not implemented |
| stdio MCP | `command`, `args`, `env_vars` | `type: stdio`, `command`, `args`, `${ENV}` | Not implemented | Not implemented | Not implemented |
| Streamable HTTP MCP | `url` | `type: http`, `url` | Not implemented | Not implemented | Not implemented |
| Bearer environment reference | `bearer_token_env_var` | `Authorization: Bearer ${ENV}` | Not implemented | Not implemented | Not implemented |
| Environment-backed HTTP headers | `env_http_headers` | `${ENV}` header value | Not implemented | Not implemented | Not implemented |

See [`docs/spikes/2026-07-30_mcp-registry-target-capabilities.md`](docs/spikes/2026-07-30_mcp-registry-target-capabilities.md) for evidence and fail-closed decisions.

## Safety model and limits

aru treats discovered instruction/skill content and Registry metadata as untrusted:

- conventional discovery scans only the source root and `skills/**/SKILL.md`, with maximum depth 6, 2,000 directories, and 20,000 entries;
- `SKILL.md` is limited to 1 MiB; each selected regular file to 10 MiB; each skill tree to 100 MiB;
- source symlinks, devices, sockets, FIFOs, non-UTF-8 paths, case-folding collisions, Windows reserved names, and escaping selectors are rejected;
- the canonical digest includes a format version, delimited portable path, executable marker, length, and raw bytes for every sorted regular file;
- Registry requests use HTTPS without URL userinfo, 10-second connect/30-second total timeouts, at most three redirects, 10 MiB bodies, 100 pages, and 10,000 records;
- malformed, oversized, truncated, cyclic, ambiguous, inactive, or unsupported metadata fails before writes;
- MCP command and argument data stays in arrays and is never shell-expanded;
- aru stores only secret environment variable names/placeholders. It does not read secret values;
- existing unmanaged instruction destinations and skill/MCP entries collide by default. `--merge` is limited to preserving unmanaged Markdown around instruction blocks; owned entries that differ from their last-applied semantic digest report drift and are preserved;
- only aru-owned server keys are merged or removed. Unrelated TOML comments/keys and JSON entries survive. Invalid existing config fails closed.

## Transactions, ownership, and recovery

Every mutating command takes `.aru/operation.lock`, rereads project inputs, validates the complete operation, stages each destination beside its final path, and writes a durable journal before fixed-order atomic replacements. Existing destinations are sibling backups. A normal apply error immediately rolls back.

After a process kill or power loss, run any mutating aru command again:

```console
aru sync
```

Before doing new work, aru checks `.aru/transaction.toml` and digest-gates a deterministic rollback to the complete old state. A dry run refuses to proceed while recovery is pending. If a destination or backup has unknown/manual content, recovery stops, preserves that content and the journal, and reports the path rather than guessing. Copy the affected project file and `.aru/transaction.toml` before manual repair; do not delete backups until their old/new digest role is understood.

If `.aru/state.toml` is lost, `aru sync --locked` adopts only current entries whose semantic digests exactly match the committed `projection-baseline`. A different same-name entry is an unmanaged collision. Baselines never authorize deletion of unknown historical orphans.

## Development

```console
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Tests use temporary Git repositories, local Registry fixtures, and Unix PTYs (`expectrl`) for the real `inquire` keyboard flow. Live network is reserved for an explicit public Git smoke test.
