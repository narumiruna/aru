# Getting started

Install aru, initialize a project, and replay its locked state.

## Install

Aru requires a system `git` executable.

=== "uv"

    Install the PyPI distribution named `arust`:

    ```console
    uv tool install arust
    ```

    The distribution name differs because `aru` was already taken on PyPI, but the installed command remains `aru`.
    Prebuilt wheels support x86-64 glibc Linux and Apple Silicon macOS.

=== "macOS and Linux"

    The standalone installer downloads a prebuilt, checksum-verified binary and does not require Rust.

    ```console
    curl -LsSf https://raw.githubusercontent.com/narumiruna/aru/main/scripts/install.sh | sh
    ```

    You can use `wget` instead:

    ```console
    wget -qO- https://raw.githubusercontent.com/narumiruna/aru/main/scripts/install.sh | sh
    ```

=== "Windows PowerShell"

    ```powershell
    powershell -ExecutionPolicy ByPass -c "irm https://raw.githubusercontent.com/narumiruna/aru/main/scripts/install.ps1 | iex"
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

Package-manager installations should be updated through the same package manager instead:

=== "uv"

    ```console
    uv tool upgrade arust
    ```

=== "Cargo"

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

The instruction-capable targets are `agents`, `codex`, `claude`, `copilot`, `pi`, and `opencode`.
Additional canonical targets provide project-scoped Agent Skills only.
Run `aru target list --available` for the complete registry and aliases such as `claude-code`, `gemini-cli`, and `kiro-cli`.
Initialization creates:

- `aru.toml` for human-maintained intent;
- `aru.lock` for exact, reproducible resolution and projection data;
- `.aru/` for ignored local cache, ownership, and transaction state.

## Adopt existing instructions

Specify each existing `AGENTS.md` file explicitly and preview without changing project files:

```console
aru instruction add AGENTS.md src/api/AGENTS.md --dry-run
```

Apply the result:

```console
aru instruction add AGENTS.md src/api/AGENTS.md
```

If an unmanaged destination such as `CLAUDE.md` already exists, aru reports a collision.
Choose an explicit policy only after reviewing the file:

```console
aru instruction add AGENTS.md src/api/AGENTS.md --merge  # preserve Markdown around owned blocks
aru instruction add AGENTS.md src/api/AGENTS.md --force  # destructively take over the destination
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

Require the committed lock, then check local synchronization without changing project or target files (private per-user lock metadata may be created outside the project):

```console
aru sync --locked
aru sync --check
```

Next, learn how to [lock and synchronize project state](sync.md) or configure a specific resource:

- [Instructions](instructions.md)
- [Agent Skills](skills.md)
- [MCP servers](mcp.md)
- [Native packages](packages.md)
