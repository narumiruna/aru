# aru

**Keep coding-agent instructions, skills, and MCP servers in sync across your project.**

Aru gives your team one project manifest, one reproducible lockfile, and safe projections for every supported coding agent.

[Documentation](https://narumiruna.github.io/aru/) · [Getting started](https://narumiruna.github.io/aru/getting-started/) · [Command reference](https://narumiruna.github.io/aru/reference/commands/) · [Source](https://github.com/narumiruna/aru)

> [!WARNING]
> Aru is under active development.
> Features, behavior, and file formats may change before 1.0.

## Why aru?

Projects often repeat the same setup for Codex, Claude Code, GitHub Copilot, pi, OpenCode, and other agents.

Aru lets you declare that setup once:

- Keep existing `AGENTS.md` files as the canonical instructions.
- Install Agent Skills from Git repositories.
- Configure MCP servers without storing secret values.
- Reuse native aru packages that bundle instructions, skills, and trusted MCP declarations.
- Pin exact revisions and projections in `aru.lock`.
- Detect drift and unmanaged content before replacing anything.

Aru supports these project targets:

| Target name | Coding-agent layout |
| --- | --- |
| `agents` | Generic Agents-compatible project files |
| `codex` | OpenAI Codex |
| `claude` | Claude Code |
| `copilot` | GitHub Copilot CLI |
| `pi` | pi |
| `opencode` | OpenCode |

## Install

Aru requires a system `git` executable.

### macOS and Linux

The recommended installer downloads a prebuilt, checksum-verified binary to `~/.local/bin` and does not require Rust:

```console
curl -LsSf https://raw.githubusercontent.com/narumiruna/aru/main/scripts/install.sh | sh
```

You can use `wget` instead:

```console
wget -qO- https://raw.githubusercontent.com/narumiruna/aru/main/scripts/install.sh | sh
```

### Windows PowerShell

The Windows installer places `aru.exe` in `~/.local/bin`:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://raw.githubusercontent.com/narumiruna/aru/main/scripts/install.ps1 | iex"
```

The standalone installers support x86-64 Linux, Intel and Apple Silicon macOS, and x86-64 Windows.

### uv

The PyPI distribution is named `arust` because `aru` was already taken, but it installs the `aru` command:

```console
uv tool install arust
```

Prebuilt wheels support x86-64 glibc Linux and Apple Silicon macOS.

### Cargo

Install from [crates.io](https://crates.io/crates/aru) with a Rust toolchain:

```console
cargo install aru --locked
```

Verify the installation:

```console
aru --version
aru --help
```

## Quick start

The following workflow initializes a project for Codex and Claude Code, adopts existing instructions, installs a skill, and verifies the result.

### 1. Initialize your project

Run this command from the project root:

```console
aru init --target codex --target claude
```

Initialization creates:

- `aru.toml`, which contains the setup your team maintains;
- `aru.lock`, which pins the exact resolved result;
- `.aru/`, which contains local cache, ownership, and recovery state.

Aru also adds `.aru/` to the project's `.gitignore`.

To initialize another existing directory, pass its path:

```console
aru init ../my-project --target codex
```

### 2. Adopt existing instructions

If your project already has one or more `AGENTS.md` files, preview their discovery:

```console
aru instruction add --discover --dry-run
```

Apply the result after reviewing the plan:

```console
aru instruction add --discover
```

Aru keeps each `AGENTS.md` file in place and creates only the files required by your selected targets.

If a destination such as `CLAUDE.md` already contains unmanaged content, aru stops instead of overwriting it.

Use `--merge` only after reviewing the collision:

```console
aru instruction add --discover --merge
```

> [!CAUTION]
> `--force` destructively takes over colliding unmanaged content.
> A later removal cannot restore that content.

### 3. Add an Agent Skill

Choose a skill interactively:

```console
aru skill add narumiruna/skills
```

For scripts and CI, select exports explicitly:

```console
aru skill add narumiruna/skills --skill writing-plans
```

Aru resolves the Git revision, records it in `aru.lock`, and projects the skill to each compatible target.

### 4. Verify the project

Replay the committed lock without changing its resolutions:

```console
aru sync --locked
```

Check that the lock and all managed target files are current without writing:

```console
aru sync --check
```

### 5. Commit the reproducible state

Commit `aru.toml`, `aru.lock`, and the `.gitignore` change.

Your team may also commit generated target files if that matches the repository's policy.

Do not commit `.aru/`.

## The everyday workflow

Most aru commands follow the same pattern:

1. Preview risky or unfamiliar changes with `--dry-run`.
2. Apply the command.
3. Review `aru.toml`, `aru.lock`, and projected target files.
4. Run `aru sync --check`.
5. Commit the intended files.

Add, remove, update, and target commands normally update the manifest, lockfile, and target projections together.

Use `--no-sync` when you intentionally want to update only `aru.toml` and `aru.lock`, then run `aru sync` later.

## Common tasks

### Manage instructions

Discover conventional root and nested `AGENTS.md` files:

```console
aru instruction add --discover --dry-run
aru instruction add --discover
```

List configured instruction selectors:

```console
aru instruction list
```

Remove a selector without deleting the canonical instruction file:

```console
aru instruction remove AGENTS.md --dry-run
aru instruction remove AGENTS.md
```

For custom paths, globs, and target-specific rules, see the [instructions guide](https://narumiruna.github.io/aru/instructions/).

### Manage Agent Skills

```console
aru skill list
aru skill add owner/repository --skill review
aru skill add owner/repository --all
aru skill update --dry-run
aru skill update
aru skill remove owner/repository --skill review
aru skill remove owner/repository
```

A bare `skill add` opens an interactive selector.

Non-interactive environments must use `--skill`, `--all`, or `--path`.

Aru discovers skills from `SKILL.md` files at the repository root or in nested directories within its discovery limits.

Each skill's `name` must match the directory containing its `SKILL.md`, or the repository name for a root skill.

See the [Agent Skills guide](https://narumiruna.github.io/aru/skills/) for revision pinning, target selection, and non-standard layouts.

### Manage MCP servers

Add an HTTPS MCP endpoint while storing only the environment variable name for its token:

```console
aru mcp add \
  --url https://docs.example.com/mcp \
  --name docs \
  --bearer-token-env DOCS_MCP_TOKEN
```

List, update, or remove MCP declarations:

```console
aru mcp list
aru mcp update --dry-run
aru mcp update context
aru mcp remove docs
```

Aru also supports Registry packages and direct stdio argv.

It validates direct commands but never executes them during add, lock, or sync.

Project MCP is supported for Codex, Claude Code, GitHub Copilot CLI, and OpenCode.

See the [MCP guide](https://narumiruna.github.io/aru/mcp/) for all source types and target capabilities.

### Manage native aru packages

A native package can bundle reusable instructions, skills, trusted MCP declarations, and package dependencies.

```console
aru add owner/agent-kit
aru add owner/agent-kit --version '^1.2'
aru update --dry-run
aru update
aru remove owner/agent-kit
```

Package-provided MCP servers are denied by default and require an explicit trust decision.

See the [native packages guide](https://narumiruna.github.io/aru/packages/) for package authoring, trust, and dependency behavior.

### Change project targets

```console
aru target list
aru target add copilot
aru target remove claude
aru target set codex claude
```

At least one target must remain.

Use `target set` when replacing the only configured target.

## Understand locking and synchronization

`aru.toml` describes what the project wants.

`aru.lock` records the exact Git revisions, metadata, content digests, and target projections needed to reproduce it.

| Command | Use it when |
| --- | --- |
| `aru lock` | You want to update `aru.lock` without changing target files |
| `aru lock --check` | You want to verify the lock without writing or using the network |
| `aru sync` | You want to resolve missing lock data and reconcile target files |
| `aru sync --locked` | You want to reproduce the existing lock without changing it |
| `aru sync --check` | You want a local, read-only exact-state check |
| `aru sync --dry-run` | You want to preview the synchronization plan |

Use `--offline` to disable remote Git and Registry access.

Use `--frozen` for the equivalent of `--locked --offline`.

Read the [lock and sync guide](https://narumiruna.github.io/aru/sync/) for detailed behavior.

## Safety model

Aru is designed to fail closed.

Before writing, it validates the complete operation and rejects unsupported, ambiguous, or unsafe inputs.

It also:

- preserves drifted or unowned content for review;
- keeps Git and MCP commands as argument arrays instead of shell-expanding them;
- never executes configured direct MCP commands;
- stores secret environment variable names or placeholders, never secret values;
- applies multi-file changes through atomic transactions;
- records durable recovery information for interrupted operations.

If an operation is interrupted, run a mutating aru command such as `aru sync` again.

Aru will attempt digest-gated recovery before starting new work.

Read the [safety and recovery guide](https://narumiruna.github.io/aru/safety/) before using destructive takeover or repairing interrupted transactions manually.

## Useful inspection commands

These commands inspect the project without changing managed state:

```console
aru sync --check
aru audit
aru tree
aru info PACKAGE
aru metadata --format-version 1
```

Run `aru COMMAND --help` for command-specific options.

The complete [command reference](https://narumiruna.github.io/aru/reference/commands/) lists every command and flag.

## Updating aru

Standalone installations can update themselves:

```console
aru self update
```

Update package-manager installations through the same package manager:

```console
uv tool upgrade arust
cargo install aru --locked
```

## Development

Install from a source checkout:

```console
cargo install --path .
```

Run the CI-equivalent checks:

```console
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Serve or build the documentation with its locked toolchain:

```console
just docs-serve
just docs-build
```

Release maintainers should follow [`docs/releasing.md`](docs/releasing.md).

Aru is available under the [MIT License](LICENSE).
