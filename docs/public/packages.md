# Native packages

A native aru package is a Git repository with a root `aru.toml` containing `[package]` metadata. A package can export instructions, Agent Skills, trusted MCP declarations, and bounded package dependencies.

## Add a package

```console
aru add owner/agent-kit
aru add owner/agent-kit --version '^1.2'
aru add ../local-agent-kit --target codex --target claude
```

By default, `add` updates `aru.toml`, resolves the complete graph into `aru.lock`, and synchronizes target paths. Use `--no-sync` to defer projection or `--dry-run` to preview without persistent writes.

Package-provided MCP servers are denied by default. Trust each required export explicitly:

```console
aru add owner/mcp-kit --trust-mcp docs
```

Trust is scoped to the root package source and MCP name. It does not bypass transport, target, secret, collision, or ownership validation.

## Pin a source

Choose at most one reference mode:

```console
aru add owner/agent-kit --version '^1.2'
aru add owner/agent-kit --branch main
aru add owner/agent-kit --rev 67cd354
```

- **Version requirements** resolve SemVer tags and provide stable published inputs.
- **Branches** express moving intent, but ordinary sync retains the locked commit.
- **Revisions** pin an exact 7–40 character Git commit.

Prefer immutable SemVer tags for published, long-lived configurations.

## Update and remove

Compatible lock nodes remain pinned during ordinary synchronization. Preview updates before applying them:

```console
aru update --dry-run
aru update
aru update owner/agent-kit
aru update owner/agent-kit --precise 1.2.3
```

Remove a direct package requirement with:

```console
aru remove owner/agent-kit
```

## Author and validate a package

A minimal package manifest looks like this:

```toml
[package]
name = "agent-kit"
version = "1.2.0"

[[instructions.sources]]
files = ["AGENTS.md"]
scope = "source-directory"
```

From a clean package Git root, inspect archive inventory or produce a deterministic archive:

```console
aru package --list
aru package
aru package --output dist/agent-kit.tar.gz
```

Dirty input requires explicit `--allow-dirty`. Package creation rejects unsafe paths, symlinks, special files, case collisions, hidden controls, cycles, conflicting requirements, and oversized graphs before writing.

For byte-level package contracts, see [`docs/formats.md` on GitHub](https://github.com/narumiruna/aru/blob/main/docs/formats.md#native-aru-packages).
