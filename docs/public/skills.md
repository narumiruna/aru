# Agent Skills

Aru resolves skills from Git repositories and installs each selected skill into compatible targets' native skill directories.

In Project scope, `skill add` records reproducible intent in `aru.toml` and `aru.lock` and manages projections through ownership state when an initialized aru project is found.
Without an `aru.toml` in the current directory or an ancestor, Project scope performs a one-time standalone installation without creating project state.

In a terminal, omitting both `--scope` and `--global` opens an Installation scope menu, with Project first and Global second.
Pass `--scope project` to select Project explicitly; `--scope global`, `--global`, and `-g` select Global.
`--global` and `--scope` cannot be combined. The existing `--project PATH` option chooses a directory, not an installation scope.
Without a terminal, or with `--no-interactive`, omitted scope defaults to Project.

## Standalone installation

Pass one or more targets to install without running `aru init`:

```console
aru skill add --target codex owner/repository --all
aru skill add --target claude --target kiro owner/repository --skill review
aru skill add --global --target codex owner/repository --skill review
```

Use `-g` or `--global` to write to each target's user-level skill directory instead of the current directory.
Global mode always selects standalone installation, even when the current directory or an ancestor contains `aru.toml`. It does not use or update the project's manifest, lockfile, or configured targets. Relative sources resolve from the current directory, or from the directory passed with `--project`, without searching for a project root.
Global destinations inside managed projects, including through symlinks, remain rejected to protect managed content; overlapping pending managed recovery also blocks installation.
For example, Codex receives `~/.codex/skills/<name>`, Claude receives `~/.claude/skills/<name>`, pi receives `~/.pi/agent/skills/<name>`, and OpenCode receives `${XDG_CONFIG_HOME:-~/.config}/opencode/skills/<name>`.
Global mode preserves aliases whose user-level paths differ from their canonical target: `universal` uses `${XDG_CONFIG_HOME:-~/.config}/agents/skills/<name>`, `antigravity-cli` uses `~/.gemini/antigravity-cli/skills/<name>`, `qoder-cn` uses `~/.qoder-cn/skills/<name>`, and `trae-cn` uses `~/.trae-cn/skills/<name>`.
`CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `VIBE_HOME`, `HERMES_HOME`, `AUTOHAND_HOME`, and `GROK_HOME` override their corresponding target roots and must be absolute paths.
For home-based paths, Windows prefers `USERPROFILE` over `HOME`; other platforms prefer `HOME`.
Eve and PromptScript do not support global installation.

After scope selection, if `--target` is omitted in an interactive terminal, aru opens a searchable multi-select target menu.
In managed Project scope, it offers configured skill-capable targets, initially checked. Standalone Project scope offers all skill targets; Global scope offers only targets with user-level skill directories.
Global paths and environment overrides are validated only for the selected targets, after the menu completes.
It then applies the normal skill selection rules, so a bare source opens the skill menu while `--all`, `--skill`, or `--path` skips it.
Non-interactive standalone commands must provide both `--target` and an explicit skill selector.

Standalone installation writes a complete independent copy to each unique target path and never creates cross-target symlinks.
This applies to both project-directory and global standalone installs.
It rejects an existing same-name destination before writing target content unless `--force` is passed.
Non-force skill installs also require atomic no-replace rename support from the platform and filesystem, so content created concurrently is preserved; unsupported backends fail rather than falling back to an overwriting rename.
If preparation fails before a journal is published, aru removes its stages and newly created empty destination parents, but preserves pre-existing directories and concurrent content. Cleanup failures are reported for review. After journal publication, the existing journal-driven recovery rules apply.
Transaction plans also reject duplicate or nested paths after conservative Unicode case and normalization comparison, even on case-sensitive filesystems; use unambiguous destination names instead.
It does not create `aru.toml`, `aru.lock`, `.aru/`, ownership state, or a project cache, and the installed copies are not managed by `skill update`, `skill remove`, or `sync`.
Standalone and managed commands, including previews, coordinate through the same persistent per-user `operation.lock`, independent of `XDG_STATE_HOME`. On Unix, a private `/var/tmp/aru-standalone-scope-<uid>/` directory (`/private/var/tmp/` on macOS) holds `scope.lock` and a versioned `scope.toml` that pins the selected control directory and its filesystem identity before journal writes. Mutations and both standalone and managed previews (including `--check`) retain both locks through pending-recovery checks, project reads, and plan/apply output. Managed interactive skill selection releases its snapshot guard while waiting for input, then reacquires the locks and revalidates the snapshot before planning or writing. Aru never locks the shared temporary directory itself. The recovery journal is removed after a completed transaction, but the scope marker remains.
Control paths with symlink ancestors are rejected rather than following a potentially retargeted lock or journal. Restore the original directory layout before retrying if existing recovery state is affected.
Before a scope is pinned, an established Unix fallback directory is preferred only when it is owned by the effective UID and is private or contains existing lock/recovery state. An unused home-based state path is not inspected when this fallback is selected. After pinning, aru never switches scopes when the home mount disappears, the account home changes, or another project is selected; it rejects an unavailable or replaced control directory until the original filesystem is restored. Without a marker or established fallback, an unavailable account home is ambiguous and is rejected rather than hiding a possible older journal.

Coordination metadata must remain outside the project being operated on. On first use, if the home-backed path is inside that project and contains no existing lock or journal, aru selects the UID fallback. An already established or pinned scope that overlaps the project is rejected, not silently moved to a second lock. The anchor directory itself must also be outside the project.

Unix control directories writable by another user are rejected without permission repair; review their contents manually. Existing journal, temporary journal, lock and scope files must be regular, owned by the effective UID, singly linked, and not writable by other users. Validation precedes permission repair, reads do not follow symlinks, and temporary writes use the same checks. Mutations may repair non-private read/execute permissions only after these checks; previews never repair them. State reads and writes are bounded to 16 MiB.
Both managed and standalone mutations recover an existing legacy project-scoped standalone journal before new writes; dry runs reject pending legacy recovery without changing it.
`--dry-run` validates and previews without changing project files or installing skills. It may create private coordination directories and `operation.lock` outside the project, including on first use; on Unix these are created with modes `0700` and `0600`. It does not recover or modify pending journals. Standalone project roots are canonicalized before validation and journaling, so relative roots work and retargeting a root symlink does not redirect recovery.
`--no-sync`, `--locked`, and `--frozen` require managed project installation and are rejected in standalone mode, including with `--global` inside an initialized project.

## Select exports

Use one selection mode per `skill add` command:

```console
aru skill add owner/repository                         # interactive
aru skill add owner/repository --all                   # all current and future exports
aru skill add owner/repository --skill review          # explicit export
aru skill add owner/repository --path extras/review    # non-standard layout
aru skill add owner/repository --all --target codex    # target subset
```

A bare command requires both stdin and stderr to be terminals. Scripts, pipes, redirected shells, and CI must pass `--all`, `--skill`, or `--path` explicitly.
These flags skip only the skill menu; scope and target menus still appear for omitted selections in a terminal.
Use `--no-interactive` to disable every prompt, including when running a script in a terminal.

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
| No reference (standalone or global) | Source's current default-branch `HEAD`, queried on every invocation |
| No reference (managed project) | Reuse a compatible lock; otherwise select the latest matching SemVer tag, falling back to `main` when none exists |
| `--version 0.5.0` | Cargo caret semantics: `^0.5.0` |
| `--version '=0.5.0'` | Exact tag |
| `--branch main` | Current branch head; managed projects pin the exact commit in `aru.lock` |
| `--rev 67cd354` | Exact immutable commit |

Standalone and global installs without a reference query the source's advertised `HEAD` before skill selection, including during `--dry-run`, then fetch that exact commit. Release tags do not override this default, and the default branch need not be named `main`. A missing or invalid `HEAD`, or a failed remote query, fails instead of falling back to a tag or stale content. Existing destinations remain protected by collision checks; refreshing the source does not authorize replacement.

`--offline` does not query remote sources and therefore rejects standalone remote installation. Local Git sources remain usable offline. Explicit `--version`, `--branch`, and `--rev` retain their selection rules.

The `main` fallback applies only to managed projects when no reference option is provided.
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

In a terminal, `aru skill update` offers declared sources with all checked, and `aru skill remove` offers a single source to remove.
Explicit sources skip these menus. Without prompts, bare `skill update` still updates all sources, while `skill remove` requires a source.
These commands manage initialized project intent only; they do not update or remove standalone/global installations.

Ordinary `sync` and `sync --locked` retain a branch's locked commit. Run `skill update` or add with `--upgrade` to move it.

## Local metadata overrides

For managed project skills, edit the installed `SKILL.md` frontmatter and run `aru sync`. Aru accepts changes to top-level fields other than `name` and `description`, records them in local ownership state, and keeps your values when upstream changes the same fields. Untouched fields follow upstream updates.

For example, add this top-level field inside the existing YAML frontmatter to hide a skill from pi's system prompt while retaining manual `/skill:name` invocation:

```yaml
disable-model-invocation: true
```

```console
aru sync
aru sync --locked --check
```

- Added or changed fields are local overrides. Deleting a field records a persistent deletion, not a request to restore the upstream default. An overridden field stays local even if upstream temporarily publishes the same value.
- Nested mappings and lists are replaced as a whole top-level value; aru does not merge individual keys inside `metadata`.
- Changes to the parsed `name` or `description`, Markdown body bytes, other files, or executable markers still fail with drift. Malformed YAML, duplicate keys, non-string top-level keys, custom tags, and YAML merge keys are rejected. Parsing bounds expanded YAML to 32 levels, 20,000 nodes, and 1 MiB of scalar string content; `SKILL.md` remains limited to 1 MiB.
- Ordinary sync preserves your header bytes when the effective fields do not change. An upstream metadata update may reformat the frontmatter; upstream body and asset updates still apply.
- Overrides are local to the projection and `.aru/state.toml`; they do not change `aru.lock` source digests or travel to a fresh checkout. Source integrity checks still cover the complete original tree. `sync --check` reports pending ownership-state changes until an ordinary sync records the edit.
- Targets linked to the same `.agents` copy share metadata. Independent copies remain independent. A target change that would combine different overrides fails instead of discarding them.
- Removing a projection with recorded overrides fails for review. Preserve the customized skill outside managed paths and manually remove the projection before retrying removal. A missing projection is recreated with its recorded overrides by `sync`.

Existing ownership entries without metadata snapshots can migrate using the unchanged source or a verified cached previous source. If neither matches the last-applied digest, aru preserves the edited skill and reports that it cannot verify the edit. Missing ownership state never authorizes adoption of a modified skill.

After upgrading from the previous adapter schema, run an ordinary `aru sync` once to refresh the lock before using `--locked`.

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
Aliases such as `claude-code`, `kiro-cli`, and `hermes-agent` normalize to canonical names and are never persisted in initialized projects.
Standalone global installation retains the requested spelling only when it selects a distinct documented user-level path.

In initialized projects, targets that share one destination create one owned projection while retaining their complete canonical target reach in `aru.lock`.
On Unix, other native paths link to a selected `.agents` copy when possible.
Platforms without project symlinks receive verified copies.
Standalone installation always uses independent copies instead.
With `--global`, standalone destinations use each target's user-level path rather than the project paths in this table.
