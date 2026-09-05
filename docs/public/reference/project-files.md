# Project files and targets

## Source of truth

| Path | Commit? | Purpose |
| --- | --- | --- |
| `aru.toml` | Yes | Human-maintained sources, package requirements, selectors, and targets |
| `aru.lock` | Yes | Exact Git commits, MCP metadata, source digests, projections, and portable ownership baseline |
| `AGENTS.md`, `**/AGENTS.md` | Yes | User-owned, directory-scoped instruction sources |
| `.aru/cache/` | No | Immutable, content-addressed Git checkouts |
| `.aru/state.toml` | No | Local deployment mode and last-applied ownership digests |
| `.aru/transaction.toml` | No | Crash-recovery journal, present only during interrupted operations |

Aru does not create or modify `.gitignore`. Add `.aru/` to your ignore rules yourself. Each team decides whether to ignore or commit generated target projections.

For byte-level persisted-format details, see [`docs/formats.md` on GitHub](https://github.com/narumiruna/aru/blob/main/docs/formats.md).

## Managed projections

Depending on configured resources and targets, aru may reconcile:

- `CLAUDE.md` and `.claude/rules/aru/**`;
- `.github/copilot-instructions.md` and `.github/instructions/aru/**`;
- target skill directories listed in the [skill target registry](skill-targets.md), including `.agents/skills/**`, `.claude/skills/**`, `.github/skills/**`, `.pi/skills/**`, and `.opencode/skills/**`;
- `.codex/config.toml`, `.mcp.json`, `.github/mcp.json`, and `opencode.json`.

Aru owns only the entries or marker blocks it created. Unrelated configuration and unmanaged content remain outside its authority.

## Target capabilities

| Capability | Agents | Codex | Claude Code | Copilot CLI | pi | OpenCode | Skill-only targets |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Directory `AGENTS.md` | Native | Native | Sibling import | Root/path projection | Native | Native | No |
| Explicit instruction globs | No | No | Path rules | Path rules | No | No | No |
| Project skills | `.agents/skills/` | `.agents/skills/` | `.claude/skills/` | `.github/skills/` | `.pi/skills/` | `.opencode/skills/` | Registry-defined |
| stdio MCP | No | Yes | Yes | Yes | No | Yes | No |
| Streamable HTTP MCP | No | Yes | Yes | Yes | No | Yes | No |
| Environment-backed HTTP auth/headers | No | Yes | Yes | Yes | No | Yes | No |

Aru rejects configurations that a selected target cannot represent rather than silently broadening or dropping behavior.

## Change targets

The six instruction-capable targets are `agents`, `codex`, `claude`, `copilot`, `pi`, and `opencode`.
Additional canonical targets support project skills only.
Use the available-target listing for the complete version-specific registry.

```console
aru target list
aru target list --available
aru target add claude
aru target add kiro-cli
aru target remove codex
aru target set codex claude
```

CLI aliases normalize to canonical names before `aru.toml` and `aru.lock` are written.
Global home-directory skill installation is not supported.

- `add` performs a set union.
- `remove` performs a set subtraction.
- `set` replaces the complete set atomically.
- At least one target must remain.

Target mutations resolve the complete result before atomically updating manifest, lock, ownership state, and owned paths. They preserve compatible locked package versions and support `--dry-run`.
