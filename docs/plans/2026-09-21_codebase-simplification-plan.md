# Codebase Simplification Plan

## Goal

Consolidate five confirmed duplicate implementations without changing aru's CLI output, public Rust API, persisted formats, target projections, fail-closed validation, or transaction behavior.

## Context

The review found repeated implementations in five bounded areas:

1. Claude and Copilot use nearly identical object-backed MCP configuration adapters in `src/target/claude.rs` and `src/target/copilot.rs`, while their entry renderers duplicate most transport fields in `src/target/mod.rs`.
2. App commands independently translate `--no-sync`, `--merge`, and `--force` into `ProjectionPolicy` in `src/app.rs` and `src/app/`.
3. Ownership identity `(kind, key, destination)` is reconstructed in `src/ownership.rs`, `src/sync.rs`, and `src/instruction/sync.rs`.
4. Skill, package, and package-trust commands independently match declared Git sources by canonical identity in `src/resolver/skill.rs` and `src/app/package_dependency.rs`.
5. `src/app.rs` repeats explicit-path canonicalization and nearest-ancestor `aru.toml` discovery for managed commands, standalone adds, and package archives.

The existing behavioral baseline passes:

- `cargo clippy --locked --all-targets --all-features -- -D warnings`
- `cargo test --locked --all-targets --all-features` (all enabled tests passed; one public-network smoke test remained ignored)

## Non-Goals

- Do not alter target capabilities, configuration paths, MCP schemas, or target-specific JSON fields.
- Do not change manifest, lockfile, ownership-state, transaction-journal, or fixture formats.
- Do not combine Codex or Opencode MCP adapters with the Claude/Copilot JSON adapter; their storage and entry formats differ materially.
- Do not generalize unrelated filesystem, resolver, or command orchestration code.
- Do not rename or remove the public `ClaudeConfig` or `CopilotConfig` types or their existing methods.
- Do not change existing CLI messages, flag precedence, collision policy, project discovery semantics, or network behavior.

## Architecture

Keep ownership with the current domain modules:

- Shared Claude/Copilot JSON storage remains under `src/target/`; public target wrappers retain their current API and paths.
- Projection-policy construction remains in `src/app.rs` beside `ExecutionPolicy` and `ProjectionPolicy`.
- Ownership identity remains in `src/ownership.rs` on `StateEntry`.
- Canonical declared-source matching remains in `src/source/git.rs` or the narrowest existing Git-source helper module.
- Project-root search remains private to `src/app.rs`.

Do not introduce traits, dependency injection, or a generic target-adapter framework for these changes.

## Plan

### 1. Lock down the duplicated behavior before consolidation

- [x] Characterize both public JSON adapters and exact transport fields in `tests/json_mcp_api.rs`. Evidence: all three tests pass against the original implementation, including serialization bytes, errors, and public cross-target set behavior.
- [x] Characterize Claude/Copilot environment placeholders, bearer precedence, omitted optional fields, unsupported transport errors, and Copilot tools. Evidence: `claude_and_copilot_transport_fields_remain_exact` passes.
- [x] Characterize projection-policy precedence. Evidence: `projection_flags_preserve_no_sync_precedence` passes against the original package policy function.
- [x] Characterize source matching and project discovery. Evidence: new lookup unit tests and `project_discovery_cli` plus `package_archive_cli` pass. Initial fixture assumptions were corrected: `./repo` is currently parsed as GitHub shorthand, so local aliases use `repo` and `./repo/.`; the nested package repository is excluded from the outer fixture's Git status.

### 2. Centralize ownership-state identity

- [x] Centralize borrowed identity and its owned conversion on `StateEntry`; use them for normalization and indexing. Evidence: four ownership tests pass, including ordering, deduplication, and lookup equivalence.
- [x] Replace both sync identity helpers with `StateEntry::owned_identity`. Evidence: `cli`, `instruction_cli`, and `target_cli` suites pass (50 tests).
- [x] Verify state bytes. Evidence: v1 ownership and skill-metadata golden tests pass; both contract fixtures have no diff.

### 3. Centralize canonical declared-source matching

- [x] Add `git::find_declared_source_key` for canonical scanning without changing exact-key shortcuts or trust scan order. Evidence: characterization tests for aliases, exact matches, sorted selection, and missing-source errors pass.
- [x] Replace three independent loops with the Git-source helper. Skill/package wrappers retain only exact-key precedence; trust delegates without a shortcut.
- [x] Verify lifecycle and resolution behavior. Evidence: resolver skill tests (3), package app tests (2), `cli` (19), and `package_cli` (15) pass; no fixture changes.

### 4. Centralize projection-policy construction

- [x] Add `ProjectionPolicy::from_flags` with existing no-sync precedence. Evidence: the characterization test now targets the common constructor and passes with the original conflict error.
- [x] Replace all resource and target-change policy branches; delete the three local helpers. Evidence: source search finds no duplicate helper or inline no-sync projection branch.
- [x] Verify cross-resource behavior. Evidence: `cli_policy`, `instruction_cli`, `mcp_cli`, `package_cli`, `plugin_cli`, and `target_cli` all pass (73 tests).

### 5. Consolidate project-root traversal

- [x] Add private `find_manifest_root` with explicit outcomes for found, missing explicit root, and missing ancestor. Evidence: one ancestor traversal remains; only explicit paths and found roots are canonicalized inside the helper.
- [x] Keep managed/archive errors and standalone fallback in their command wrappers. Evidence: exact-error, explicit-root, nearest-manifest, and Unix symlink characterization tests pass.
- [x] Verify discovery and standalone behavior. Evidence: `project_discovery_cli`, `standalone_skill_cli`, `standalone_mcp_cli`, `package_archive_cli`, and `cli_policy` pass (47 tests).

### 6. Share Claude/Copilot MCP JSON implementation

- [x] Add private `src/target/json_mcp.rs` with shared map operations and path-specific errors; no new state, trait, or capability branching.
- [x] Delegate both public adapters without changing their root fields, derived Debug/Clone representation, paths, or signatures. Evidence: `json_mcp_api` passes through both concrete APIs, including cross-target set compatibility.
- [x] Combine the Claude/Copilot transport match arms and keep Copilot tools explicit. Evidence: exact transport values and optional-field behavior pass.
- [x] Preserve explicit `McpConfig` dispatch. Evidence: a new test rejects every mismatched configured/requested target pair without changing serialized config.
- [x] Verify MCP behavior. Evidence: target tests (25), `json_mcp_api` (3), `mcp_cli` (9), and `standalone_mcp_cli` (9) pass. A temporary pre/post-binary comparison of managed and standalone stdio/HTTP installs confirms identical JSON bytes, CLI output, manifest, lockfile, and ownership state; temporary files were removed.

### 7. Final validation and review

- [x] `cargo fmt --all -- --check` passed.
- [x] `cargo clippy --locked --all-targets --all-features -- -D warnings` passed without warnings.
- [x] `cargo test --locked --all-targets --all-features --quiet` passed: 451 tests, one intentionally ignored public-network smoke test.
- [x] Review the complete source/test diff and run `git diff --check origin/main...HEAD`. Evidence: no whitespace errors, public API removals, unrelated changes, or generic framework. Locking, recovery, journaling, transaction application, and secret handling are untouched; new tests cover existing lookup/error precedence, target mismatch rejection, and state identity semantics.
- [x] No sync regeneration trigger occurred. Evidence: manifest, lockfile, dependency files, contract fixtures, capability schema, target paths, and instruction sources are unchanged; MCP projection bytes and ownership bytes were checked against the pre-refactor binary. Repository-local sync was not needed.
- [ ] Push the signed focused branch and open the requested pull request with verification evidence and platform limitations. After handoff, remove this completed plan and push that cleanup.

## Risks

- Sharing the JSON adapter could accidentally normalize existing user JSON differently. Mitigation: retain `serde_json::Map` storage and current pretty serializer, and compare exact generated bytes.
- A projection-policy constructor could change the current `no_sync` precedence over merge/force validation. Mitigation: encode and test that precedence before replacing call sites.
- Canonical source lookup could return a different declaration or hide an error when equivalent or invalid keys exist. Canonical duplicate rejection occurs later in resolution, not in manifest parsing. Preserve skill/package exact-key shortcuts and package-trust's unconditional canonical scan, including sorted scan order and errors; characterize both before consolidation.
- A root-discovery helper could erase command-specific errors or standalone behavior. Mitigation: share only path discovery; leave outcomes and messages in existing wrappers.
- Ownership identity changes affect safety-sensitive reconciliation. Mitigation: preserve tuple ordering and golden state bytes, and run the complete transaction and CLI suite.

## Rollback / Recovery

Each simplification is independently revertible and should be implemented as a separate coherent commit. No data migration or recovery operation is expected because persisted formats must not change. If a behavior or byte-equivalence check fails, revert only the corresponding consolidation and retain any characterization tests that accurately document existing behavior. Do not recover by weakening validation, changing fixtures, or broadening force behavior.

## Completion Checklist

- [x] All five duplicate implementations are consolidated at their current domain boundaries.
- [x] Public `ClaudeConfig` and `CopilotConfig` APIs remain available and source-compatible; concrete API integration tests pass.
- [x] CLI text, flag precedence, project discovery, target paths, and projection bytes remain unchanged, backed by characterization tests and binary comparison.
- [x] Manifest, lockfile, ownership-state, journal, capability schema, and contract fixtures remain unchanged.
- [x] No new traits, generic utility framework, or speculative extension points were introduced.
- [x] Formatting, Clippy, and the full enabled test suite pass.
- [x] The reviewed diff contains only scoped source/test changes and this plan's lifecycle updates.
- [ ] Signed changes are pushed and the pull request is open; report its URL and remove the completed plan.

## Verification Limitations

Validation ran on Linux. Windows and macOS execution, installer scripts, wheel builds, and the intentionally ignored public-network smoke test were not run; platform-specific and distribution code is unchanged. No unresolved implementation or acceptance failure remains.
