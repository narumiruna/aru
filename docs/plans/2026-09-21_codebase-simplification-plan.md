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

- [ ] Add one `ProjectionPolicy` constructor in `src/app.rs` for `(no_sync, merge, force)`. Preserve the existing precedence: `no_sync` returns `LockOnly` before projection collision flags are interpreted; projected `merge && force` remains an error. Acceptance: focused policy tests pass with existing error messages.
- [ ] Replace policy branches in `src/app/instruction.rs`, `src/app/mcp.rs`, `src/app/package_dependency.rs`, `src/app/plugin.rs`, `src/app/skill.rs`, and target-change handling in `src/app.rs`. Commands that do not support merge pass `false` explicitly. Acceptance: `skill_projection`, `package_projection`, plugin `projection`, and equivalent inline branches are removed.
- [ ] Verify every resource mutation preserves dry-run, no-sync, merge, force, and collision behavior. Acceptance: `tests/cli_policy.rs`, `tests/instruction_cli.rs`, `tests/mcp_cli.rs`, `tests/package_cli.rs`, `tests/plugin_cli.rs`, and `tests/target_cli.rs` pass.

### 5. Consolidate project-root traversal

- [ ] Extract one private helper in `src/app.rs` that canonicalizes an optional explicit root or finds the nearest ancestor containing `aru.toml`. Return enough information for callers to retain their current command-specific outcomes; do not embed standalone fallback or user-facing error policy in the helper. Acceptance: only one ancestor traversal remains.
- [ ] Rebuild `discover_add_root`, `discover_project`, and `package_for_archive` around the helper while preserving their distinct standalone fallback and exact error messages. Acceptance: explicit `--project`, nearest initialized ancestor, missing manifest, and package-root behavior remain unchanged.
- [ ] Verify project discovery through `tests/standalone_skill_cli.rs`, `tests/standalone_mcp_cli.rs`, `tests/package_archive_cli.rs`, and relevant `tests/cli_policy.rs` cases. Acceptance: tests pass and captured output is unchanged.

### 6. Share Claude/Copilot MCP JSON implementation

- [ ] Introduce one private object-backed MCP JSON implementation under `src/target/` for loading a configured path, validating the root and `mcpServers`, computing entry digests, setting/removing entries, and serializing pretty JSON with a final newline. Keep path-specific error context injectable and explicit. Acceptance: the implementation contains no target capability branching and all common storage behavior has one implementation.
- [ ] Refactor `ClaudeConfig` and `CopilotConfig` to delegate to the shared implementation while retaining their public names, method signatures, configuration paths, and error strings. Acceptance: downstream code using either public type continues to compile without source changes.
- [ ] Extract focused shared rendering for the common Claude/Copilot stdio fields and streamable-HTTP fields in `src/target/mod.rs`; apply Copilot's `tools: ["*"]` addition explicitly after common rendering. Acceptance: each normalized entry is byte-for-byte JSON-equivalent after canonical serialization.
- [ ] Keep `McpConfig` dispatch in `src/target/mcp.rs` explicit. Acceptance: Codex, Claude, Copilot, and Opencode remain visibly distinct variants and mismatched target/config combinations still fail closed.
- [ ] Verify managed and standalone MCP merging and replay. Acceptance: target adapter unit tests, `tests/mcp_cli.rs`, and `tests/standalone_mcp_cli.rs` pass; generated `.mcp.json` and `.github/mcp.json` fixtures from representative commands match pre-refactor bytes.

### 7. Final validation and review

- [ ] Run `cargo fmt --all -- --check`. Acceptance: exits successfully.
- [ ] Run `cargo clippy --locked --all-targets --all-features -- -D warnings`. Acceptance: exits successfully with no warnings.
- [ ] Run `cargo test --locked --all-targets --all-features`. Acceptance: all enabled tests pass; the explicit public-network smoke test may remain ignored.
- [ ] Inspect `git diff --check` and the complete diff. Acceptance: no whitespace errors, persisted fixture changes, public API removals, unrelated edits, or new generic framework are present.
- [ ] Confirm `cargo run --locked --quiet -- sync --locked --dry-run` is unnecessary because no manifest, lock identity, capability schema, instruction source, or projection format changed; if implementation changes any of those unexpectedly, run the repository-required sync and investigate rather than accepting incidental generated changes. Acceptance: the final diff demonstrates that no regeneration trigger occurred, or the required commands pass with intentional reviewed outputs.

## Risks

- Sharing the JSON adapter could accidentally normalize existing user JSON differently. Mitigation: retain `serde_json::Map` storage and current pretty serializer, and compare exact generated bytes.
- A projection-policy constructor could change the current `no_sync` precedence over merge/force validation. Mitigation: encode and test that precedence before replacing call sites.
- Canonical source lookup could return a different declaration or hide an error when equivalent or invalid keys exist. Canonical duplicate rejection occurs later in resolution, not in manifest parsing. Preserve skill/package exact-key shortcuts and package-trust's unconditional canonical scan, including sorted scan order and errors; characterize both before consolidation.
- A root-discovery helper could erase command-specific errors or standalone behavior. Mitigation: share only path discovery; leave outcomes and messages in existing wrappers.
- Ownership identity changes affect safety-sensitive reconciliation. Mitigation: preserve tuple ordering and golden state bytes, and run the complete transaction and CLI suite.

## Rollback / Recovery

Each simplification is independently revertible and should be implemented as a separate coherent commit. No data migration or recovery operation is expected because persisted formats must not change. If a behavior or byte-equivalence check fails, revert only the corresponding consolidation and retain any characterization tests that accurately document existing behavior. Do not recover by weakening validation, changing fixtures, or broadening force behavior.

## Completion Checklist

- [ ] All five duplicate implementations are consolidated at their current domain boundaries.
- [ ] Public `ClaudeConfig` and `CopilotConfig` APIs remain available and source-compatible.
- [ ] CLI text, flag precedence, project discovery, target paths, and projection bytes remain unchanged.
- [ ] Manifest, lockfile, ownership-state, journal, capability schema, and contract fixtures remain unchanged.
- [ ] No new traits, generic utility framework, or speculative extension points were introduced.
- [ ] Formatting, Clippy, and the full enabled test suite pass.
- [ ] The final diff contains only scoped source and test changes required by this plan.
