# Getting started

Install aru, initialize a project, and replay its locked state.

## Install

Aru requires a system `git` executable. The standalone installers download a prebuilt, checksum-verified binary and do not require Rust.

=== "macOS and Linux"

    ```console
    curl -LsSf https://raw.githubusercontent.com/narumiruna/aru/main/install.sh | sh
    ```

    You can use `wget` instead:

    ```console
    wget -qO- https://raw.githubusercontent.com/narumiruna/aru/main/install.sh | sh
    ```

=== "Windows PowerShell"

    ```powershell
    powershell -ExecutionPolicy ByPass -c "irm https://raw.githubusercontent.com/narumiruna/aru/main/install.ps1 | iex"
    ```

=== "Cargo"

    ```console
    cargo install aru
    ```

Verify the installation:

```console
aru --version
aru --help
```

Standalone installations can update themselves:

```console
aru self update
```

Cargo-built binaries should be updated through Cargo instead:

```console
cargo install aru --locked
```

## Initialize a project

Run `init` from an existing project directory and name every coding agent the project should support:

```console
aru init --target codex --target claude --target copilot
```

Or initialize another directory:

```console
aru init ../my-project --target codex
```

Supported target names are `agents`, `codex`, `claude`, `copilot`, `pi`, and `opencode`.
Initialization creates:

- `aru.toml` for human-maintained intent;
- `aru.lock` for exact, reproducible resolution and projection data;
- `.aru/` for ignored local cache, ownership, and transaction state.

## Adopt existing instructions

Preview conventional `AGENTS.md` discovery without writing:

```console
aru instruction add --discover --dry-run
```

Apply the result:

```console
aru instruction add --discover
```

If an unmanaged destination such as `CLAUDE.md` already exists, aru reports a collision. Choose an explicit policy only after reviewing the file:

```console
aru instruction add --discover --merge  # preserve Markdown around owned blocks
aru instruction add --discover --force  # destructively take over the destination
```

!!! warning
    `--force` is destructive takeover. A later removal will not restore the old unmanaged content.

## Add a skill

Choose exports explicitly for reproducible scripts and CI:

```console
aru skill add narumiruna/skills --skill writing-plans
```

In an interactive terminal, omit selectors to open the searchable multi-select interface:

```console
aru skill add narumiruna/skills
```

## Verify replay

Require the committed lock, then check local synchronization without writing:

```console
aru sync --locked
aru sync --check
```

Next, learn how to [lock and synchronize project state](sync.md) or configure a specific resource:

- [Instructions](instructions.md)
- [Agent Skills](skills.md)
- [MCP servers](mcp.md)
- [Native packages](packages.md)
