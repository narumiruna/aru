# Agent Skills

Aru resolves skills from Git repositories and projects each selected skill into every compatible target's native project directory.

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

Targets that share one destination create one owned projection while retaining their complete canonical target reach in `aru.lock`.
On Unix, other native paths link to a selected `.agents` copy when possible.
Platforms without project symlinks receive verified copies.
Aru does not install skills into global home-directory paths.
