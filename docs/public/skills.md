# Agent Skills

Aru resolves skills from Git repositories and installs each selected skill into compatible targets' native project directories.

In an initialized aru project, `skill add` records reproducible intent in `aru.toml` and `aru.lock` and manages projections through ownership state.
In a directory without an `aru.toml` in it or an ancestor, the same command performs a one-time standalone installation without creating project state.

## Standalone installation

Pass one or more targets to install without running `aru init`:

```console
aru skill add --target codex owner/repository --all
aru skill add --target claude --target kiro owner/repository --skill review
aru skill add --global --target codex owner/repository --skill review
```

Use `-g` or `--global` to write to each target's user-level skill directory instead of the current directory.
Global mode is intentionally standalone: aru rejects it when the discovered root contains `aru.toml`.
For example, Codex receives `~/.codex/skills/<name>`, Claude receives `~/.claude/skills/<name>`, pi receives `~/.pi/agent/skills/<name>`, and OpenCode receives `${XDG_CONFIG_HOME:-~/.config}/opencode/skills/<name>`.
`CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `VIBE_HOME`, `HERMES_HOME`, `AUTOHAND_HOME`, and `GROK_HOME` override their corresponding target roots and must be absolute paths.
Eve and PromptScript do not support global installation.

If `--target` is omitted in an interactive terminal, aru first opens a searchable multi-select target menu.
It then applies the normal skill selection rules, so a bare source opens the skill menu while `--all`, `--skill`, or `--path` skips it.
Non-interactive standalone commands must provide both `--target` and an explicit skill selector.

Standalone installation writes a complete independent copy to each unique target path and never creates cross-target symlinks.
This applies to both project-directory and global standalone installs.
It rejects an existing same-name destination before any write unless `--force` is passed.
It does not create `aru.toml`, `aru.lock`, `.aru/`, ownership state, or a project cache, and the installed copies are not managed by `skill update`, `skill remove`, or `sync`.
Mutating standalone and managed installs coordinate through aru's durable OS-user state directory, independent of `XDG_STATE_HOME`; the recovery journal is removed after a completed transaction.
`--dry-run` validates and previews the complete installation without writing.
`--no-sync`, `--locked`, and `--frozen` require an initialized project and are rejected in standalone mode.

## Select exports

Use one selection mode per `skill add` command:

```console
aru skill add owner/repository                         # interactive
aru skill add owner/repository --all                   # all current and future exports
aru skill add owner/repository --skill review          # explicit export
aru skill add owner/repository --path extras/review    # non-standard layout
aru skill add owner/repository --all --target codex    # target subset
```

A bare command requires an interactive terminal. Scripts, pipes, redirected shells, and CI must pass `--all`, `--skill`, or `--path` explicitly.

Aru discovers a skill at the repository root (`./SKILL.md`) or in any nested directory (`**/SKILL.md`) within the discovery limits.
When the source repository contains a valid `aru.lock`, automatic discovery ignores a locked skill only when its content digest still matches and it is under a hidden projection directory corresponding to that skill's locked targets, such as `.agents/skills/` or `.pi/skills/`.
This prevents unchanged installed copies and their nested contents from affecting discovery while preserving drifted content, untracked skills, and skills under unrelated target directories; use `--path` to select a projection directory explicitly.
A skill's `name` must match the directory that directly contains its `SKILL.md`; a root skill must match the repository name.

Selection intent is preserved:

- `--all` stores wildcard intent and includes valid exports added by future versions.
- Interactive selection stores an explicit snapshot, even when every visible export is selected.
- Repeated `--skill` options are additive.
- Removing the final explicitly selected skill removes its source.

## Pin a revision

```console
aru skill add owner/repository --version 0.5.0 --skill review
aru skill add owner/repository --version '=0.5.0' --skill review
aru skill add owner/repository --branch main --skill review
aru skill add owner/repository --rev 67cd354 --skill review
```

| Option | Resolution behavior |
| --- | --- |
| No reference | Latest matching SemVer tag, falling back to `main` when none exists |
| `--version 0.5.0` | Cargo caret semantics: `^0.5.0` |
| `--version '=0.5.0'` | Exact tag |
| `--branch main` | Current branch head pinned to an exact commit in `aru.lock` |
| `--rev 67cd354` | Exact immutable commit |

The `main` fallback applies only when no reference option is provided.
An explicit `--version` remains strict and never falls back to a branch.
The fallback commit is pinned in `aru.lock`; a later update prefers a matching SemVer tag if the repository adds one.

Use `--upgrade` during `add` to re-resolve an existing source instead of reusing its compatible lock.

## Update and remove

```console
aru skill list
aru skill update --dry-run
aru skill update
aru skill update owner/repository
aru skill remove owner/repository --skill review
aru skill remove owner/repository
```

Ordinary `sync` and `sync --locked` retain a branch's locked commit. Run `skill update` or add with `--upgrade` to move it.

## Projection paths

Aru supports full adapters and skill-only targets.
Run `aru target list --available` or read the [skill target registry](reference/skill-targets.md) for every canonical name, alias, capability, and exact project directory.

| Full or native target | Project path |
| --- | --- |
| Agents | `.agents/skills/<name>` |
| Codex | `.agents/skills/<name>` |
| Claude Code | `.claude/skills/<name>` |
| GitHub Copilot | `.github/skills/<name>` |
| pi | `.pi/skills/<name>` |
| OpenCode | `.opencode/skills/<name>` |

Skill-only targets include direct paths such as `.kiro/skills/<name>` and `.windsurf/skills/<name>`, shared `.agents/skills/<name>` projections, and explicit exceptions such as `.factory/skills/<name>` for `droid`.
Aliases such as `claude-code`, `kiro-cli`, and `hermes-agent` normalize to canonical names and are never persisted.

In initialized projects, targets that share one destination create one owned projection while retaining their complete canonical target reach in `aru.lock`.
On Unix, other native paths link to a selected `.agents` copy when possible.
Platforms without project symlinks receive verified copies.
Standalone installation always uses independent copies instead.
With `--global`, standalone destinations use each target's user-level path rather than the project paths in this table.
