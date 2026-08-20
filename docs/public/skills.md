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

| Target | Project path |
| --- | --- |
| Codex | `.agents/skills/<name>` |
| Claude Code | `.claude/skills/<name>` |
| GitHub Copilot | `.github/skills/<name>` |
| pi | `.pi/skills/<name>` |
| OpenCode | `.opencode/skills/<name>` |

On Unix, when Codex receives the same skill, other native paths link to the canonical `.agents` copy when possible. Platforms without project symlinks receive verified copies.
