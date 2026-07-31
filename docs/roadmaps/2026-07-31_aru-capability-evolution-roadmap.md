# aru Capability Evolution Roadmap

- Audience: aru maintainers and contributors
- Scope: APM-inspired capabilities reshaped around aru's uv/Cargo-style CLI and native file contracts
- Planning horizon: sequential outcome phases without committed dates
- Design source: [`../spikes/2026-07-31_apm-inspired-uv-cargo-cli-ux.md`](../spikes/2026-07-31_apm-inspired-uv-cargo-cli-ux.md)

## Vision

Make aru a trustworthy, inspectable, and composable project manager for coding-agent instructions, skills, and MCP servers, with uv/Cargo-style workflows and aru-native safety guarantees rather than APM compatibility.

## Objectives

- Give users one read-only command that explains lock, projection, ownership, recovery, and content-security findings without changing project state.
- Let projects narrow direct skills and MCP servers to explicit target subsets while preserving complete fail-closed validation.
- Provide deterministic update previews and standards-based lock inventory for local review and CI integration.
- Establish a native aru package model only after the current resource lifecycle is diagnosable and target-precise.
- Make any future package graph inspectable and reproducible through stable human and machine interfaces.

No delivery dates, maintainer capacity, adoption targets, or performance budgets were supplied. This roadmap therefore sequences outcomes but does not promise a calendar.

## Current State

### Verified capabilities

- The current root lifecycle is `init`, `lock`, and `sync`; direct resources are managed through `instruction`, `skill`, `mcp`, and `target` namespaces.
- `lock --check`, `sync --check`, `--dry-run`, `--locked`, `--offline`, and Cargo-style `--frozen` are implemented.
- aru discovers root and nested Agent Skills, resolves Git references to exact commits, supports interactive and explicit subsets, and replays immutable cached content offline.
- Instructions may already restrict their selected targets. Direct skills and MCP declarations currently apply to every capable configured target.
- Registry, direct HTTPS, and direct stdio MCP sources are supported without executing configured commands or reading secret values.
- Ownership baselines, local state, operation locking, durable journals, sibling staging and backups, rollback, and digest-gated recovery protect target files.
- Output is deterministic, uses stdout for list data and stderr for human status, and supports quiet, verbose, color, and no-progress controls.

### Verified constraints

- There is no native aru package composition model or transitive dependency graph.
- There are no root `audit`, `export`, `tree`, `info`, `metadata`, `package`, `add`, `remove`, or `update` commands.
- Existing content validation is bounded and portable, but there is no explicit hidden-Unicode audit report.
- Existing lock data can identify resolved skill and MCP inputs, but no SBOM export contract exists.
- `src/app.rs` is 970 lines and `src/transaction.rs` is 938 lines. New command orchestration must follow existing `src/app/` decomposition rather than push either file beyond the repository's 1,000-line boundary without justification.

### Proposed direction

The design spike proposes borrowing APM's audit, package composition, dependency inspection, inventory export, and target-scoping ideas while using uv/Cargo vocabulary:

- `audit` instead of a separate APM compatibility or CI mode;
- update preview through `update --dry-run`, not `outdated`;
- `tree --invert`, not a separate `deps why`;
- `info`, `metadata`, `export`, and `package` as root commands; and
- `add` and `remove` for future native aru packages while `sync` remains reconciliation.

No implementation phase is an approved delivery commitment merely because it appears in this roadmap. Each phase still requires an approved implementation plan.

## Guiding Principles

- Prefer aru-native contracts over compatibility modes; interoperability is an explicit import or export boundary, not hidden runtime dispatch.
- Separate intent changes from reconciliation: `add` and `remove` change project intent, while `sync` applies the complete desired state.
- Preserve the current meanings of locked, offline, frozen, check, dry-run, and no-sync across every new command.
- Put consequential evidence before automation: users must be able to explain drift, ownership, and dependency reach before aru adds a transitive package graph.
- Narrow capability explicitly. A dependency target list may only reduce the configured target set and may never activate another target or permit lossy conversion.
- Treat package content and metadata as untrusted. Bound discovery, fail before writes, and never execute package scripts, configured MCP commands, or shell-expanded metadata.
- Keep secrets outside aru. Persist only environment-variable names or placeholders and never resolve secret values during add, lock, sync, audit, package, or export.
- Add one canonical command for each user goal; do not introduce `install`, `outdated`, `view`, `deps why`, `pack`, or `lock export` aliases.
- Make machine interfaces versioned, deterministic, and stdout-clean before asking external tools to depend on them.

## Roadmap Themes

### Explainable trust

Turn aru's existing lock, digest, ownership, and recovery evidence into actionable local and CI diagnostics, including content-security findings that hashes alone cannot explain.

### Precise deployment

Let users express which configured targets should receive each dependency while retaining capability checks, deterministic projection identity, and safe contraction.

### Portable interoperability

Expose locked inventory in standard and versioned machine formats without making exports a second source of truth or implying unsupported attestations.

### Composable packages

Add reusable aru-native packages and transitive resolution only after trust, target reach, and update behavior are observable and stable.

## Phases and Milestones

**Delivery status (2026-07-31): complete.** All four phases are implemented with versioned fixtures, public CLI coverage, offline/no-write checks, and CI-equivalent Rust gates. Detailed execution evidence is preserved in the archived plans under `docs/plans/archived/`.

### Phase 1: Explain project integrity without mutation — Complete

**Milestones:**

- `aru audit` reports manifest/lock consistency, projection drift, ownership-reference failures, pending recovery, deployed-content drift, and a bounded hidden-Unicode classification over current instructions and skills.
- Audit is non-interactive, local and read-only by default, emits deterministic text plus one explicitly versioned machine format, and provides one recovery action when known.
- `skill update --dry-run` and `mcp update --dry-run` report current and candidate resolutions while leaving manifest, lock, cache, state, and target paths unchanged.
- `sync --check` remains the concise exact-state gate; audit remains the detailed explanation surface, with tests proving they do not silently heal one another's findings.

**Outcome:** Users and CI can understand whether a current aru project is trustworthy and what action is required before any new package abstraction expands the state space.

### Phase 2: Narrow deployment and export locked inventory — Complete

**Milestones:**

- Direct skill and MCP requirements accept non-empty, duplicate-free target subsets that can only narrow `project.targets`.
- Resolution computes effective reach as configured targets intersected with dependency targets and target capabilities, rejecting empty or lossy results before writes.
- Target contraction updates manifest, lock identity, ownership baselines, state, and owned target paths atomically without unlocking unrelated package versions.
- `aru export` emits a deterministic CycloneDX inventory from the existing lock without network access, source re-resolution, or source rehashing; the first release explicitly decides whether SPDX ships concurrently or later.
- Audit includes target-reach and exported-inventory consistency checks appropriate to the shipped contracts.

**Outcome:** Projects can minimize where each resource is deployed and can hand a deterministic locked inventory to external tooling without adopting another manifest format.

### Phase 3: Compose reproducible aru packages — Complete

**Milestones:**

- A package contract decision fixes the manifest filename, package identity, supported primitive layout, dependency requirement syntax, and extension/versioning policy before implementation begins.
- A native package can bundle supported instructions, skills, and MCP declarations without weakening existing per-resource validation or target capability rules.
- The resolver produces a bounded, deterministic direct and transitive graph; cycles, ambiguous identities, duplicate exports, depth/entry limit overflow, and unresolvable requirements fail before writes.
- Transitive MCP servers are denied by default unless the root project provides an explicit trust decision that is credential-free, reviewable, and locked.
- `aru add`, `aru remove`, and `aru update [PACKAGE]` mutate package intent through the existing transaction boundary, support no-sync and dry-run behavior, and preserve compatible locked versions by default.
- `aru sync --locked --offline` replays the complete package graph and target projections from committed lock data and verified cache content.

**Outcome:** Teams can reuse and compose aru-native agent configuration while retaining reproducibility, least deployment reach, and aru's fail-closed ownership model.

### Phase 4: Make the package ecosystem inspectable and distributable — Complete

**Milestones:**

- `aru tree` displays the deterministic package graph and supports bounded depth, target filtering, JSON output, and Cargo-style `--invert` reverse-dependency queries.
- `aru info PACKAGE` presents declared intent, exact resolution, exported primitives, dependency count, and effective target reach, failing on ambiguous selectors rather than choosing one.
- `aru metadata --format-version 1` exposes a documented, stable JSON graph without credentials; `--no-deps` avoids transitive fetching and resolution.
- `aru package --list` produces the exact deterministic archive inventory, and `aru package` assembles a validated archive with fixed path, timestamp, ordering, size, and executable-bit rules.
- Package assembly has no validation bypass and rejects dirty input unless `--allow-dirty` is explicitly supplied and visibly reported.

**Outcome:** Humans and tools can explain, integrate, and distribute aru packages through stable interfaces without parsing internal lock or state files directly.

## Technical Health

- Decompose new root command orchestration into focused `src/app/` modules before `src/app.rs` exceeds 1,000 lines. Keep audit policy, export serialization, package resolution, and archive assembly with their owning domains rather than in one facade.
- Keep `transaction.rs` focused on generic atomic application and recovery. New package or audit policy must produce operations or findings without embedding domain-specific decisions in transaction code.
- Version every persisted lock, capability, package, metadata, audit, export, and archive contract that external state or tooling depends on. Update golden fixtures whenever identity or projection semantics change.
- Keep hidden-Unicode scanning pure, bounded, encoding-aware, and independently testable. Do not let remediation mutate user-owned sources.
- Preserve deterministic collection types and explicit sorting for graph traversal, diagnostics, exports, package inventories, and plans.
- Exercise package resolution through local temporary Git repositories and fixtures. Routine tests must not require live Git hosts, APM services, MCP Registry access, or vulnerability APIs.
- Measure audit, graph, and package operations before setting performance budgets. Current latency and scale baselines are unavailable.
- Run focused tests during each milestone and the repository's fmt, clippy, and all-target/all-feature test gates for every Rust or persisted-format change.

## Risks and Dependencies

| Risk or dependency | Consequence | Mitigation or decision gate |
| --- | --- | --- |
| A native package model expands scope into an APM reimplementation | aru becomes a follower and weakens its product boundary | Phase 3 starts only after a package-contract decision confirms aru-native semantics and preserves every guiding principle |
| `aru add` may be confused with `aru skill add` | Users cannot predict whether a repository is a package or raw skill source | Require an unambiguous package marker and produce a teaching error that points raw skill repositories to `aru skill add` |
| Target restrictions alter persisted projection identity | Locked replay or cleanup may preserve stale paths incorrectly | Update capability schema and golden fixtures; test add, removal, contraction, state loss, drift, and rollback before release |
| Hidden-Unicode scanning can flag legitimate content | Multilingual or emoji-rich instructions become noisy or blocked | Define an evidence-backed severity table, keep ordinary Unicode valid, separate informational findings from blocking controls, and validate against multilingual fixtures |
| Transitive MCP introduces executable and network trust | A dependency could broaden a project's runtime attack surface | Deny transitive MCP by default and require root-level, credential-free approval before any projection |
| SBOM fields may be incomplete in the current lock | Export could imply provenance or license certainty that aru does not possess | Omit unknown fields, label output as inventory, and defer richer claims until the lock records verified evidence |
| Machine formats become accidental public APIs | Early schema mistakes constrain later development | Require explicit format versions, fixtures, deterministic output, and documented compatibility policy before release |
| Archive semantics vary by platform | Package bytes or integrity differ across systems | Decide canonical paths, timestamps, ordering, line-ending treatment, permissions, and case-folding rules before `aru package` ships |
| Maintainer capacity and user demand are unknown | Later phases may consume effort without validated value | Treat each phase boundary as a continuation decision and collect usage/issues from the preceding outcome before approving the next plan |

## Success Metrics

Adoption, repository count, audit latency, and package-graph scale have no measured baseline. Targets for those outcomes must be set only after instrumentation or user evidence exists.

| Outcome indicator | Baseline | Target | Horizon | Measurement source |
| --- | --- | --- | --- | --- |
| Read-only integrity diagnosis | No `aru audit` command | Every committed audit fixture category is reported deterministically and no audit path changes persistent project bytes | Phase 1 | CLI integration tests and before/after filesystem snapshots |
| Update visibility | Dry-run exists but no dedicated current-to-candidate reporting contract | Skill and MCP update previews identify unchanged and candidate resolutions without writes | Phase 1 | Public CLI tests with local Git and Registry fixtures |
| Dependency target precision | Instructions can be target-scoped; skills and MCP cannot | Every direct skill/MCP target subset is validated, locked, projected, contracted, and replayed across supported adapters | Phase 2 | Manifest/lock fixtures and target CLI tests |
| Inventory reproducibility | No export command | Identical lock plus pinned timestamp produces byte-identical supported export output | Phase 2 | Golden export fixtures |
| Package reproducibility | No native package graph | A locked package graph replays successfully with `sync --locked --offline` from verified cache content | Phase 3 | Offline graph integration tests |
| Graph explainability | No package graph inspection | Every locked direct/transitive node is reachable through tree, inverse-tree, info, or versioned metadata output | Phase 4 | Graph contract tests and metadata fixtures |
| Safe distribution | No aru package archive | Package inventory and archive fixtures are byte-stable and reject every defined unsafe path/content class | Phase 4 | Cross-platform package fixture tests |

## Non-Goals

- Supporting `apm.yml`, `apm.lock.yaml`, or an APM mode.
- Claiming OpenAPM or APM CLI compatibility.
- Adding `aru install`; package intent uses `add` and project reconciliation uses `sync`.
- Adding standalone `outdated`, `view`, `deps why`, `pack`, or `lock export` aliases.
- Auto-detecting targets from local harness directories.
- Executing package lifecycle scripts, hooks, configured MCP commands, or package-supplied shell text.
- Reading, resolving, or persisting secret values.
- Providing global package installation, marketplace, publish, or self-update workflows in this roadmap.
- Adding a force or no-verify switch that bypasses security, ownership, graph, or package validation.
- Committing dates, staffing, registry services, or package adoption targets without evidence.

## Decisions and Changes

- **2026-07-31 — Keep aru native rather than add APM mode.** Rationale: APM compatibility would add a second evolving contract and conflict with aru's focused fail-closed model. Impact: APM is an implementation research source only; interoperability must be explicit.
- **2026-07-31 — Use uv/Cargo command vocabulary where semantics align.** Rationale: separating add/remove from sync and using tree/info/metadata/package reduces command overlap and follows the repository's established CLI preference. Impact: proposed APM names such as install, outdated, view, deps why, pack, and lock export are excluded.
- **2026-07-31 — Sequence evidence and target precision before package composition.** Rationale: transitive packages multiply ownership, trust, and projection states. Impact: audit, update preview, target restrictions, and export precede the native package graph.
- **2026-07-31 — Complete all four roadmap phases.** Rationale: the audit, target/export, native package, inspection, metadata, and deterministic archive contracts now have implementation and verification evidence. Impact: the roadmap remains as delivered product history; future capability work should begin in a new roadmap rather than extending these phases silently.
- **2026-07-31 — Preserve Cargo-style frozen semantics.** Rationale: aru already defines frozen as locked plus offline. Impact: future commands retain that behavior rather than adopting uv's different frozen-lock interpretation.
- No prior roadmap artifact was supplied. The linked spike is the originating design record; future revisions should append significant changes here rather than rewrite this history.
