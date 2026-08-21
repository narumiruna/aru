# Instructions

Existing `AGENTS.md` files remain canonical and user-owned. Aru records their paths without moving, editing, or removing them, then creates only the projections required by configured targets.

## Add canonical sources

Specify exact root or nested `AGENTS.md` paths when previewing or applying:

```console
aru instruction add AGENTS.md src/api/AGENTS.md --dry-run
aru instruction add AGENTS.md src/api/AGENTS.md
```

The command does not search the project for instruction files.
Configure glob selectors directly in `aru.toml` when needed.

List declared selectors or remove exact selectors:

```console
aru instruction list
aru instruction remove AGENTS.md --dry-run
aru instruction remove AGENTS.md
```

Removing a selector does not remove the canonical source file.

## Configure sources

Declare source paths in `aru.toml`:

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

- `scope = "source-directory"` requires matched files named `AGENTS.md`; each file applies to its own directory tree.
- `apply-to` declares exact repository-relative globs for targets that can preserve path-specific rules.

The optional `targets` list defaults to every configured project target and must be a subset of them.
Patterns are project-relative; `.git/**` and `.aru/**` are always excluded.
Projects without instruction declarations skip source resolution.
Otherwise, aru limits traversal to the fixed roots in `files`; selectors that begin with a wildcard necessarily start at the project root.

## Target projection

| Target | Instruction behavior |
| --- | --- |
| Agents | Consumes directory-scoped `AGENTS.md` directly |
| Codex | Consumes directory-scoped `AGENTS.md` directly |
| pi | Consumes directory-scoped `AGENTS.md` directly |
| OpenCode | Consumes directory-scoped `AGENTS.md` directly |
| Claude Code | Receives sibling `CLAUDE.md` imports and `.claude/rules/aru/` path rules |
| GitHub Copilot | Receives a root instruction block and `.github/instructions/aru/` path rules |

Unsupported scope and target combinations fail before any write. Removing a source or target removes only digest-matching aru-owned output; drifted or unowned content is preserved for review.

## Handle collisions deliberately

When a destination already contains unmanaged content, choose one policy:

- `--merge` preserves Markdown around aru-owned marker blocks.
- `--force` replaces the colliding unmanaged destination.

!!! danger
    `--force` is a destructive takeover, not a conflict-resolution shortcut. Back up and review the destination before using it.
