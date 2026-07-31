# APM-Inspired, uv/Cargo-Shaped CLI UX Spike

Date: 2026-07-31

Status: proposed product direction. This document records a CLI design, not implemented behavior. Existing commands remain authoritative until an approved implementation changes them.

## Scope

This spike identifies useful APM capabilities that aru could adopt without becoming an APM compatibility mode. It reshapes those capabilities around aru's existing uv/Cargo-inspired lifecycle and fail-closed safety model.

The proposal assumes aru remains native to `aru.toml` and `aru.lock`. A future aru package may bundle instructions, skills, and MCP declarations and may declare transitive aru package dependencies. The package format itself is not defined here.

Evidence was taken from the checked-out references:

- APM at `634f7b603a8c827ab5c2a7c776ba2e470b1303eb`, including `install`, `audit`, `deps`, `view`, `outdated`, `lock export`, and `pack` documentation.
- uv at `f45f3a12549122455e8f622a3da0f1eef666c7d6`, especially its `add`, `remove`, `sync`, `lock`, `tree`, `audit`, and `export` command contracts.
- Cargo at `0d83fa61d55f3d2c5c1db906a1ea57308edbf5dd`, especially `update`, `tree`, `info`, `metadata`, and `package`.

These snapshots are implementation research, not aru's canonical contract.

## Current baseline

aru already has several capabilities that overlap APM:

- `init`, `lock`, and exact target reconciliation through `sync`.
- Cargo-style `--locked`, `--offline`, and `--frozen`, where frozen means locked plus offline.
- Read-only `--check` and no-write `--dry-run` paths.
- Root `SKILL.md`, conventional `skills/**/SKILL.md`, explicit selectors, and interactive skill subsets.
- Immutable Git lock revisions, bounded discovery, canonical content digests, and offline cache replay.
- Per-source instruction targets.
- Ownership-aware cleanup, drift preservation, atomic multi-file transactions, and digest-gated recovery.
- Registry, direct remote, and direct stdio MCP declarations without executing configured MCP commands or reading secret values.

The most useful APM ideas therefore are not another manifest mode. They are stronger project inspection, security reporting, reusable package composition, target-scoped dependencies, and standards-based export.

## Design principles

- Separate intent mutation from reconciliation: `add` and `remove` change intent; `sync` applies the complete project.
- Keep project lifecycle verbs at the root, as uv does, and use resource namespaces only where instructions, skills, and MCP genuinely differ.
- Prefer Cargo inspection vocabulary: `tree`, `info`, `metadata`, and `package`.
- Preserve the current meaning of `--locked`, `--offline`, `--frozen`, `--check`, `--dry-run`, and `--no-sync`.
- Do not add aliases that duplicate one operation under several names before 1.0.
- Keep every audit and inspection command non-interactive and safe for CI.
- Reject unsupported or lossy target behavior before writes.
- Never execute package lifecycle scripts, configured MCP commands, or package-supplied shell text.
- Never read or persist secret values; retain only environment-variable names or placeholders.

## Command mapping

| User goal | APM | Proposed aru | Primary convention |
| --- | --- | --- | --- |
| Add a reusable package | `apm install PACKAGE` | `aru add PACKAGE` | uv/Cargo `add` |
| Apply declared intent | `apm install` | `aru sync` | uv `sync` |
| Remove a package | `apm uninstall PACKAGE` | `aru remove PACKAGE` | uv/Cargo `remove` |
| Update locked packages | `apm update` | `aru update [PACKAGE]` | Cargo `update` |
| Preview available updates | `apm outdated` | `aru update --dry-run` | Cargo dry-run planning |
| Display the dependency graph | `apm deps tree` | `aru tree` | uv/Cargo `tree` |
| Explain a transitive dependency | `apm deps why PACKAGE` | `aru tree --invert PACKAGE` | Cargo inverted tree |
| Inspect package metadata | `apm view PACKAGE` | `aru info PACKAGE` | Cargo `info` |
| Check security and integrity | `apm audit` | `aru audit` | uv `audit` |
| Emit a stable machine graph | no direct equivalent | `aru metadata` | Cargo `metadata` |
| Export an SBOM | `apm lock export` | `aru export` | uv `export` |
| Build a distributable archive | `apm pack` | `aru package` | Cargo `package` |

## Proposed information architecture

The root help should group commands by user goal rather than display one undifferentiated list.

### Project lifecycle

- `init`
- `add`
- `remove`
- `lock`
- `sync`
- `update`

### Inspection and assurance

- `tree`
- `info`
- `metadata`
- `audit`

### Distribution and interoperability

- `package`
- `export`

### Direct resource management

- `instruction`
- `skill`
- `mcp`
- `target`

The existing resource commands remain useful. A native aru package is a higher-level composition unit, not a replacement name for an arbitrary skill repository or MCP server.

## Native aru package workflow

### Add

```console
aru add owner/agent-kit
aru add owner/agent-kit --version 1.2.0
aru add owner/agent-kit --branch main
aru add owner/agent-kit --rev 67cd354
aru add ../local-agent-kit
```

By default, `add` should:

1. update `aru.toml`;
2. resolve and update `aru.lock`; and
3. synchronize target paths.

Like `uv add`, callers can defer the deployed environment while retaining resolved intent:

```console
aru add owner/agent-kit --no-sync
```

A package can be restricted to a subset of project targets:

```console
aru add owner/agent-kit \
  --target codex \
  --target claude
```

`--target` is repeatable. aru should not add comma-delimited target parsing when repetition is already established.

Direct resources retain their explicit namespaces:

```console
aru skill add owner/skills
aru mcp add --url https://docs.example.com/mcp --name docs
```

### Remove

```console
aru remove owner/agent-kit
aru remove owner/agent-kit --no-sync
aru remove owner/agent-kit --dry-run
```

Removal must delete only digest-matching, aru-owned outputs. Drifted or unowned target content must remain in place with actionable diagnostics.

### Empty and error states

- `aru add` without a package is a usage error.
- Adding an already-declared equivalent canonical source is idempotent when no options change.
- Conflicting source identities, missing package metadata, graph cycles, duplicate exported names, unsupported target capabilities, and untrusted transitive MCP declarations fail before any write.
- Cancellation of any future interactive package selection has no side effects.

## Lock, sync, and update

### Lock

```console
aru lock
aru lock --check
aru lock --dry-run
```

- `lock` resolves intent and updates only `aru.lock`.
- `lock --check` asserts that the lock would remain unchanged.
- `lock --dry-run` resolves and reports the prospective lock change without writing.

### Sync

```console
aru sync
aru sync --locked
aru sync --offline
aru sync --frozen
aru sync --check
aru sync --dry-run
```

The established Cargo-style policy remains:

- `--locked`: fail if `aru.lock` would change.
- `--offline`: perform no remote Git or Registry access.
- `--frozen`: equivalent to `--locked --offline`.
- `--check`: assert that the lock and all target paths are synchronized.
- `--dry-run`: resolve and print the complete deterministic plan without persistent writes.

This intentionally does not adopt uv's different frozen-lock semantics.

### Update

```console
aru update
aru update owner/agent-kit
aru update owner/agent-kit --precise 1.4.2
aru update --dry-run
```

`aru update --dry-run` replaces a separate `outdated` command. It should perform the same resolution as an update and report current and candidate values without changing the manifest, lock, cache, local state, or target paths.

```text
    Updating package index
       Would update owner/agent-kit 1.2.0 -> 1.4.1
       Would update owner/shared     a721bd0 -> e938f21
    Finished Dry run complete; no files were changed.
```

Finding available updates is informational and exits successfully. Resolution or authentication failure is still an error.

Existing direct-resource update paths remain:

```console
aru skill update --dry-run
aru skill update owner/skills
aru mcp update --dry-run
aru mcp update docs
```

A package update may advance its transitive package graph within declared constraints. Direct skill and MCP declarations are updated only through their resource commands unless a later design establishes an unambiguous unified selector model.

## Dependency inspection

These commands become useful only after aru has a transitive package graph.

### Tree

```console
aru tree
aru tree --depth 2
aru tree --target claude
aru tree --format json
```

Example:

```text
project
├── owner/agent-kit v1.4.1
│   ├── owner/review-skills v2.0.0
│   └── owner/shared-rules v1.1.0
└── owner/docs-kit v0.8.0
    └── owner/shared-rules v1.1.0 (*)
```

Reverse dependency inspection uses Cargo's inverted-tree model rather than a separate `why` command:

```console
aru tree --invert owner/shared-rules
aru tree -i owner/shared-rules
```

Additional behavior:

- Default output is deterministic UTF-8 text; an ASCII fallback should be available when terminal capability requires it.
- Repeated shared dependencies are deduplicated and marked unless an explicit no-dedupe option is later justified.
- Cycles are resolver errors and must never be presented as a valid locked graph.
- `--target` filters effective deployment reach; it does not mutate configured targets.
- JSON output goes to stdout with status and errors on stderr.

### Info

```console
aru info owner/agent-kit
aru info owner/agent-kit --offline
```

Installed packages should show both declared intent and locked resolution. Uninstalled package inspection may query a remote source; the global `--offline` policy forbids that unless sufficient cache data exists.

```text
name:         agent-kit
source:       https://github.com/owner/agent-kit
requirement:  ^1.2.0
locked:       1.4.1
revision:     e938f21…
targets:      claude, codex
instructions: 2
skills:       4
mcp:          1
dependencies: 2
```

Ambiguous short names fail with the matching canonical package identities instead of selecting the first result.

## Audit

```console
aru audit
aru audit --format json
aru audit --format sarif
aru audit --format sarif --output audit.sarif
```

`audit` is a detailed read-only assurance command. It should check:

- `aru.toml` and `aru.lock` consistency;
- source identity and package graph integrity;
- lock completeness and exact resolved revisions;
- projection baselines and target capability schema;
- target files that are missing, drifted, or no longer desired;
- ownership-state references and pending recovery journals;
- deployed content hashes;
- bounded hidden-Unicode findings in instructions and skills;
- transitive MCP trust decisions; and
- whether each selected target can represent every projected capability without loss.

`sync --check` and `audit` remain distinct:

- `aru sync --check` is the concise exact-synchronization gate.
- `aru audit` explains integrity and security findings in detail.

### Audit behavior

- Audit is always read-only, non-interactive, and local by default.
- Audit never repairs the lock, state, sources, or target files.
- A separate `--ci` mode is unnecessary because the default command is already CI-safe.
- Text is the default human format; JSON and SARIF have stable schemas.
- Machine output goes to stdout or the explicit output path. Diagnostics go to stderr.
- Critical content, lock inconsistency, invalid ownership, or target drift exits non-zero.
- Informational findings do not fail the command.
- If no package dependencies exist, audit still checks project instructions, direct skills, direct MCP declarations, lock state, ownership, and projections.

APM's `--strip` behavior is intentionally excluded. aru must not automatically rewrite user-owned instruction or skill source files.

## Machine-readable metadata

```console
aru metadata --format-version 1
aru metadata --format-version 1 --no-deps
```

Following Cargo, callers must select a supported format version explicitly. The JSON document should include:

- project root and manifest path;
- declared targets;
- direct and transitive packages;
- instructions, skills, and MCP resources;
- declared requirements and exact resolutions;
- graph edges and parent relationships;
- effective per-target projections; and
- portable ownership identities, but no local secret values.

`--no-deps` returns the project and direct declarations without resolving or fetching transitive package metadata. Output ordering and schema-version behavior must be deterministic and documented.

## Export

```console
aru export --format cyclonedx1.5
aru export --format spdx2.3
aru export --format cyclonedx1.5 --output-file sbom.json
aru export --format spdx2.3 --output-file sbom.spdx.json
```

Like uv, export is a root command rather than a `lock export` subcommand.

Export should:

- read the existing `aru.lock` only;
- perform no resolution, source download, or source-tree rehash;
- write stdout by default and support `--output-file`;
- sort components and relationships deterministically;
- support a pinned timestamp for byte-stable output;
- scrub credentials from recorded URLs; and
- identify itself as a dependency inventory, not a security attestation.

A missing or invalid lock is an error. Export must never silently produce a partial inventory.

## Package assembly

```console
aru package
aru package --list
aru package --allow-dirty
```

Following Cargo:

- `aru package` assembles a distributable aru package archive.
- `--list` prints the exact included paths without producing an archive.
- `--allow-dirty` permits packaging from a dirty Git worktree while retaining an explicit warning in human output.

Packaging validates by default:

- package metadata and dependency constraints;
- portable paths, case-folding collisions, symlinks, and special files;
- per-file and total size ceilings;
- package graph identity and lock completeness;
- primitive discovery and duplicate exports;
- target capability declarations;
- hidden Unicode; and
- deterministic archive inventory.

A `--no-verify` bypass is intentionally omitted because it conflicts with aru's fail-closed package boundary.

## Per-dependency target restrictions

This capability can be implemented before transitive aru packages because the instruction model already establishes target subsets.

```console
aru skill add owner/skills \
  --skill review \
  --target codex \
  --target claude

aru mcp add \
  --url https://docs.example.com/mcp \
  --name docs \
  --target codex
```

Illustrative manifest shape:

```toml
[skills]
"owner/skills" = { include = ["review"], exclude = [], targets = ["claude", "codex"] }

[mcp.docs]
url = "https://docs.example.com/mcp"
targets = ["codex"]
```

The effective target set is:

```text
project targets ∩ dependency targets ∩ target capabilities
```

Rules:

- Omitted dependency targets mean every compatible project target.
- An explicit target list must be non-empty, duplicate-free, and a subset of `project.targets`.
- A target restriction narrows deployment and never activates an unconfigured target.
- An empty effective set is an error for a declared resource.
- Unsupported or lossy capability combinations fail before manifest, lock, state, or target writes.
- Target changes update capability and projection identity without unlocking unrelated package versions.

## Loading, success, and failure feedback

- Read-only commands show no progress unless they perform bounded remote queries.
- Remote resolution uses existing static Cargo-style progress on stderr and honors `--no-progress`.
- Empty reports state what was checked; they do not print an unexplained blank table.
- Dry runs prefix prospective actions with `Would` and always end with an explicit no-write completion message.
- Successful mutation output is emitted only after the transaction commits.
- Errors identify the failed package or resource, the violated invariant, and one recovery action when known.
- Meaning never depends on color. JSON, SARIF, metadata, list, and export data remain clean on stdout.
- `--quiet` suppresses normal progress and completion but not errors or required safety warnings.
- Repeated `--verbose` exposes exact revisions, digests, and projection identities without changing behavior.

## Acceptance criteria

### CLI contract

- Root help groups lifecycle, inspection, distribution, and direct-resource commands.
- `install`, `outdated`, `view`, `deps why`, `pack`, and `lock export` are not introduced as duplicate aliases.
- Existing shared global options retain their current meaning and conflict rules.
- Every new mutating command supports `--dry-run`; intent mutations that normally deploy support `--no-sync`.

### Safety and compatibility

- Existing `aru.toml`, `aru.lock`, ownership state, and target projections remain readable unless an approved migration explicitly changes their format.
- Complete graph, capability, collision, drift, and ownership validation occurs before writes.
- Multi-file mutations use existing operation locking, durable journals, sibling staging/backups, atomic replacement, and digest-gated recovery.
- Unknown, drifted, or unowned content is preserved for review.
- No command executes package scripts or configured MCP commands, shell-expands metadata, reads secret values, or persists credentials.

### Determinism and automation

- Plans, graphs, metadata, findings, package inventories, and exports use stable ordering.
- Machine formats have explicit schema or format versions and send only data to stdout.
- Dry run, check, audit, metadata, tree, info in offline mode, and export do not persist project or cache state.
- CI behavior is non-interactive and uses documented exit statuses.

### Tests and documentation

- Public CLI tests cover help, conflicts, no-write behavior, output routing, empty states, failure remediation, and exit statuses.
- Graph tests cover transitive depth, shared dependencies, inverse queries, ambiguity, cycles, and target filtering.
- Audit tests cover lock drift, projection drift, ownership corruption, pending recovery, hidden Unicode, and machine formats.
- Package and export tests use byte-stable fixtures and reject partial or unsafe output.
- User-facing documentation distinguishes current behavior from proposed or preview behavior until each phase ships.

## Priorities

1. Add `aru audit` on top of existing lock, ownership, transaction, and projection evidence.
2. Add target restrictions to direct skills and MCP servers.
3. Improve `skill update --dry-run` and `mcp update --dry-run` so they clearly report current and candidate resolutions.
4. Add deterministic `aru export` from the existing lock.
5. Define the native aru package contract, then add `add`, `remove`, `update`, `tree`, and `info`.
6. Add versioned `metadata` after the package graph stabilizes.
7. Add `package` after package validation and archive contracts are stable.

## Explicit non-goals

- No `apm.yml` mode or APM CLI compatibility claim.
- No `aru install`; `add` and `sync` remain separate user goals.
- No standalone `aru outdated`; update preview uses `update --dry-run`.
- No `aru deps` namespace or separate `why`; use `tree` and `tree --invert`.
- No `view`, `pack`, or `lock export` aliases.
- No automatic target detection from local harness directories.
- No package lifecycle scripts or package-supplied command execution.
- No install-time resolution of secret values.
- No global package installation, marketplace, publish, or self-update work in this proposal.
- No security bypass that weakens validation or ownership checks.

## Resolved implementation decisions

- Native packages use root `aru-package.toml`; package and lock contracts are documented in `docs/formats.md`.
- Package identity is the canonical credential-free Git repository source in this release; no registry identity is inferred.
- Hidden format controls are blocking audit errors, while warnings and informational findings do not make audit fail.
- Audit JSON, tree JSON, and metadata each begin at contract version 1. SARIF is not part of the initial contract.
- CycloneDX 1.5 is the initial inventory format; SPDX is deferred.
- Package archives use `.aru-package.tar.gz`, sorted portable paths, zero timestamps and ownership, and Git-index-derived `0644`/`0755` modes.
