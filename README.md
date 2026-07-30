# aru

`aru` is a project-scoped package manager for coding-agent skills and MCP servers. `aru.toml` records intent, `aru.lock` pins exact source content and per-agent projections, and `aru sync` safely reconciles Codex and Claude Code project paths.

## Install

```console
cargo install --path .
aru --help
```

A system `git` executable is required. Git subprocesses use argument arrays and existing SSH or credential-helper configuration; aru never invokes a shell.

## Quick start

```console
aru init --agent codex --agent claude-code
aru skill add narumiruna/skills --skill writing-plans --version 0.5.0
aru sync --locked
aru skill update narumiruna/skills
aru skill remove narumiruna/skills --skill writing-plans
```

Add every valid export from a collection by omitting selectors. Use repeatable `--skill` for stable names, or `--path` only for a non-standard repository layout:

```console
aru skill add owner/repository
aru skill add owner/repository --skill review --skill applying-tdd
aru skill add owner/repository --path extras/review
aru skill add ssh://git@example.com/team/skills.git --rev 67cd354 --skill review
```

`--version 0.5.0` has Cargo caret semantics (`^0.5.0`); use `--version =0.5.0` for an exact tag. `--version` and `--rev` are mutually exclusive. Source positionals never contain a path selector or version, so SCP-like `git@host:path` remains unambiguous.

Add a Registry package or a direct remote MCP server:

```console
aru mcp add io.example/context --name context --transport stdio --package-registry npm
aru mcp add --url https://docs.example.com/mcp --name docs \
  --bearer-token-env DOCS_MCP_TOKEN
aru mcp update context
aru mcp remove docs
```

A Registry candidate must be unique after transport, package-registry, and all-agent capability filtering. aru never chooses the first API array item.

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
aru init --agent codex --agent claude-code
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

## Project files

| Path | Commit? | Purpose |
| --- | --- | --- |
| `aru.toml` | Yes | Human-maintained requirements, selectors, and agent list |
| `aru.lock` | Yes | Exact Git commits, content digests, MCP metadata/candidates, per-agent targets, and portable ownership baseline |
| `.agents/skills/<name>` | Optional | Canonical installed skill bytes; Codex reads this directly |
| `.claude/skills/<name>` | Optional | Relative link to the canonical skill, or a verified copy where links are unavailable |
| `.codex/config.toml` | Optional | Codex project MCP entries |
| `.mcp.json` | Optional | Claude Code project MCP entries |
| `.aru/cache/` | No | Immutable, content-addressed Git checkouts |
| `.aru/state.toml` | No | Local deployment mode and last-applied ownership digests |
| `.aru/transaction.toml` | No | Crash-recovery journal, present only during an interrupted operation |

`aru init` adds `.aru/` to `.gitignore`. It does not ignore generated agent paths because teams may choose to commit them.

### Manifest selection semantics

- `include = ["*"]` tracks every valid export; `exclude` records per-skill removals.
- Explicit `include` lists are additive. Removing the final explicit name removes the package.
- A `--path` selection always persists `paths.<name>` in `aru.toml` until explicitly removed.
- Equivalent canonical Git sources do not create duplicate packages.
- Registry and Git versions stay locked during ordinary sync. `skill update [source]` and `mcp update [name]` unlock only selected packages.
- Direct MCP URLs have no upgradable version.

### Lock and sync modes

- `aru lock` resolves and writes the lock without changing agent project paths.
- `aru sync` reuses every compatible locked package, fills missing lock/projection data, and reconciles project paths.
- `aru sync --locked` rejects a missing or stale lock, including incomplete per-agent MCP targets. It never changes the lock.
- `--no-sync` on add/remove/update still resolves and transactionally updates `aru.toml` plus `aru.lock`; it only skips projections.
- `--dry-run` may read Git or HTTP sources through a temporary cache, but does not modify `aru.toml`, `aru.lock`, `.aru/`, or agent paths.
- `--force` is destructive takeover of a colliding unmanaged key/path. The operation plan says `force replace`; a later remove does not restore the previous unmanaged value.

Changing only `project.agents` does not unlock package versions. It does invalidate the projection hash, so `--locked` fails until a normal sync adds or removes per-agent target selections.

## Agent capability matrix

| Capability | Codex | Claude Code | aru v1 |
| --- | --- | --- | --- |
| Project skills | `.agents/skills/` | `.claude/skills/` | Yes |
| stdio MCP | `command`, `args`, `env_vars` | `type: stdio`, `command`, `args`, `${ENV}` | Yes |
| Streamable HTTP | `url` | `type: http`, `url` | Yes |
| Bearer environment reference | `bearer_token_env_var` | `Authorization: Bearer ${ENV}` | Yes |
| Environment-backed HTTP headers | `env_http_headers` | `${ENV}` header value | Yes |
| SSE | No distinct current transport | Deprecated support | No |
| Inline secret value | Representable in some host fields | Representable | Rejected |
| OAuth credential storage | Host-managed | Host-managed | Not managed |

See [`docs/spikes/2026-07-30_mcp-registry-agent-capabilities.md`](docs/spikes/2026-07-30_mcp-registry-agent-capabilities.md) for evidence and fail-closed decisions.

## Safety model and limits

aru treats downloaded skills and Registry metadata as untrusted:

- conventional discovery scans only the source root and `skills/**/SKILL.md`, with maximum depth 6, 2,000 directories, and 20,000 entries;
- `SKILL.md` is limited to 1 MiB; each selected regular file to 10 MiB; each skill tree to 100 MiB;
- source symlinks, devices, sockets, FIFOs, non-UTF-8 paths, case-folding collisions, Windows reserved names, and escaping selectors are rejected;
- the canonical digest includes a format version, delimited portable path, executable marker, length, and raw bytes for every sorted regular file;
- Registry requests use HTTPS without URL userinfo, 10-second connect/30-second total timeouts, at most three redirects, 10 MiB bodies, 100 pages, and 10,000 records;
- malformed, oversized, truncated, cyclic, ambiguous, inactive, or unsupported metadata fails before writes;
- MCP command and argument data stays in arrays and is never shell-expanded;
- aru stores only secret environment variable names/placeholders. It does not read secret values;
- existing unmanaged skills/MCP entries collide by default. Owned entries that differ from their last-applied semantic digest report drift and are preserved;
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

Tests use temporary Git repositories and local Registry fixtures. Live network is reserved for an explicit public Git smoke test.
