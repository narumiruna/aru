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

`aru init` adds `.aru/` to `.gitignore`. Generated target projections are not ignored automatically because each team decides whether to commit them.

For byte-level persisted-format details, see [`docs/formats.md` on GitHub](https://github.com/narumiruna/aru/blob/main/docs/formats.md).

## Managed projections

Depending on configured resources and targets, aru may reconcile:

- `CLAUDE.md` and `.claude/rules/aru/**`;
- `.github/copilot-instructions.md` and `.github/instructions/aru/**`;
- `.agents/skills/**`, `.claude/skills/**`, `.github/skills/**`, `.pi/skills/**`, and `.opencode/skills/**`;
- `.codex/config.toml`, `.mcp.json`, `.github/mcp.json`, and `opencode.json`.

Aru owns only the entries or marker blocks it created. Unrelated configuration and unmanaged content remain outside its authority.

## Target capabilities

| Capability | Codex | Claude Code | Copilot CLI | pi | OpenCode |
| --- | --- | --- | --- | --- | --- |
| Directory `AGENTS.md` | Native | Sibling import | Root/path projection | Native | Native |
| Explicit instruction globs | No | Path rules | Path rules | No | No |
| Project skills | `.agents/skills/` | `.claude/skills/` | `.github/skills/` | `.pi/skills/` | `.opencode/skills/` |
| stdio MCP | Yes | Yes | Yes | No | Yes |
| Streamable HTTP MCP | Yes | Yes | Yes | No | Yes |
| Environment-backed HTTP auth/headers | Yes | Yes | Yes | No | Yes |

Aru rejects configurations that a selected target cannot represent rather than silently broadening or dropping behavior.

## Change targets

```console
aru target list
aru target add claude
aru target remove codex
aru target set codex claude
```

- `add` performs a set union.
- `remove` performs a set subtraction.
- `set` replaces the complete set atomically.
- At least one target must remain.

Target mutations resolve the complete result before atomically updating manifest, lock, ownership state, and owned paths. They preserve compatible locked package versions and support `--dry-run`.
