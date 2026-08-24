# Standalone MCP Add Plan

## Goal

Allow `aru mcp add` to install one MCP entry into one or more target-native project configuration files without requiring `aru init`.
Standalone installation must preserve unrelated configuration, reject ambiguous or unsafe changes before writes, and leave no `aru.toml`, `aru.lock`, `.aru/`, ownership state, or project cache.

## Context

Managed `mcp add` currently requires `aru.toml`, records intent in the manifest and lock, and reconciles target entries through ownership state.
The existing MCP resolver already converts Registry, direct HTTPS, and direct stdio requirements into target-specific `McpTarget` records for Codex, Claude Code, Copilot CLI, and OpenCode.
The existing target adapters already load, validate, merge, digest, and serialize `.codex/config.toml`, `.mcp.json`, `.github/mcp.json`, and `opencode.json` while preserving unrelated entries.
Standalone MCP differs from standalone Skill installation because the destination configuration file normally already exists and must be read, merged, and replaced atomically rather than treated as a whole-file collision.

## Architecture

- Generalize add-command root discovery into a shared `Managed(PathBuf)` or `Standalone(PathBuf)` result used by both `skill add` and `mcp add`; all list, update, remove, lock, and sync commands continue to require a managed project.
- Generalize interactive target choices so each caller supplies canonical targets and destination labels; Skill choices show skill roots, while MCP choices include only MCP-capable targets and show their configuration files.
- Extract MCP argument validation and normalization from manifest mutation so both modes build the same validated `McpRequirement` without reading environment values or executing direct commands.
- Expose a crate-internal single-requirement resolver that computes the normalized requirement digest and calls the existing Registry/direct candidate and `McpTarget` conversion path for an explicit target set.
- Resolve Registry metadata before taking the project mutation lock, then acquire the standalone lock before reading target configuration files so concurrent standalone operations cannot overwrite each other's entries.
- Replace the current whole-destination standalone apply API with a prepare-under-lock transaction API: it acquires the external path-keyed lock, performs digest-gated recovery, rechecks that no `aru.toml` appeared, runs a caller closure that loads and validates current files and returns operations plus plan data, and applies those operations through the existing journal, sibling stage, backup, rollback, and atomic replacement machinery while still holding the lock.
- Refactor standalone Skill apply to use the same prepare-under-lock API and retain its existing destination-level collision behavior and tests.
- For MCP, treat only the selected server name as the collision boundary: an existing configuration file is expected, an absent named entry is merged, an existing named entry is rejected by default, and `--force` replaces only that entry while preserving unrelated content.

## Non-Goals

- Do not add standalone `mcp list`, `mcp update`, or `mcp remove`; standalone entries have no aru ownership or dependency intent after installation.
- Do not install into user home-directory or global MCP configuration paths.
- Do not add targets, transports, Registry runtimes, OAuth flows, working directories, timeout fields, or command execution.
- Do not persist environment values, bearer tokens, static authorization values, or Registry credentials.
- Do not change managed-project manifest, lockfile, ownership, replay, or target-default behavior.

## Assumptions

- Without `--project`, an `aru.toml` in the current directory or an ancestor selects managed behavior; otherwise the canonical current directory is the standalone root.
- With `--project`, an existing directory containing `aru.toml` selects managed behavior and an existing directory without it selects standalone behavior.
- Standalone MCP supports only Codex, Claude Code, Copilot CLI, and OpenCode because they are the current MCP-capable project targets.
- Repeated `--target` remains supported, aliases normalize through Clap, and duplicate or unsupported targets fail before Registry access or project writes.
- Omitting `--target` in a terminal opens a searchable multi-select before Registry access; non-terminal use requires at least one explicit `--target`.
- Direct HTTPS and direct stdio declarations remain usable with `--offline`; Registry declarations fail before HTTP resolution when offline.
- `--dry-run` may perform Registry reads and config parsing but writes no project or persistent cache data.
- `--no-sync`, `--locked`, and `--frozen` are rejected in standalone mode because there is no deferred intent or lockfile to replay.
- Rerunning standalone add for an existing same-name entry requires `--force`, even when the rendered entry is identical.

## Plan

- [x] Add focused CLI and interactive tests for standalone `mcp add` root discovery, explicit and selected targets, direct URL, direct stdio, Registry/offline policy, cancellation, non-terminal errors, and managed-project regression; verified `cargo test --locked --test standalone_mcp_cli` passes 9 tests and `cargo test --locked --test interactive_cli standalone_` passes 4 tests.
- [x] Refactor `src/app.rs` project discovery and `src/interactive.rs` target choice input so Skill and MCP add share root-mode selection and deterministic capability-specific labels; verified standalone Skill CLI tests, 4 interactive chooser unit tests, and 4 standalone PTY tests pass.
- [x] Refactor `src/app/mcp.rs` to construct and validate one normalized `McpRequirement` independently of `ManifestDocument`, then reuse that helper in managed add; verified all 9 existing `mcp_cli` tests and the MCP add help contract pass.
- [x] Extract a crate-internal single MCP resolution function in `src/resolver.rs` that accepts name, requirement, explicit targets, and offline policy and returns the existing `McpServer`/`McpTarget` model; verified the focused direct candidate/target test passes, standalone direct URL and stdio tests pass, offline Registry rejection is write-free, and existing Registry fixture tests remain in the full suite.
- [x] Refactor `src/transaction/standalone.rs` into a prepare-under-lock transaction boundary and migrate standalone Skill installation to it; verified all 11 transaction tests, including failure rollback and interrupted standalone recovery, and all 6 standalone Skill CLI tests pass without project state.
- [x] Implement standalone MCP preparation in `src/app/mcp.rs` by loading every selected `McpConfig` under the standalone lock, validating all files and same-name collisions before mutation, applying target-specific entries, and emitting one atomic file operation per changed target; verified malformed and collision multi-target tests produce zero partial writes and `--force` preserves unrelated entries.
- [x] Add adapter-focused standalone tests proving Codex TOML and OpenCode JSONC comments survive, Claude/Copilot unrelated JSON keys and servers survive, malformed containers fail closed, environment output contains placeholders but not marker secret values, and direct stdio commands are never executed; verified the multi-target and malformed-config standalone tests pass alongside existing duplicate OpenCode adapter tests.
- [x] Cover standalone policy and output behavior for `--dry-run`, `--quiet`, target cancellation, `--no-sync`, `--locked`/`--frozen`, unsupported targets, duplicate targets, and same-name collisions; verified focused CLI/PTY tests pass with zero-write assertions and deterministic config destinations.
- [x] Update `README.md`, `docs/public/mcp.md`, `docs/public/safety.md`, and `docs/public/reference/commands.md` with mode selection, supported target paths, entry-level collision semantics, no-state/no-management behavior, offline limits, and the no-secret/no-execution boundary; verified every referenced path exists, help exposes the documented apply options, and `git diff --check` passes.
- [x] Run `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked --all-targets --all-features`, and `git diff --check`; all gates pass with 144 library tests and every deterministic CLI suite passing, while the one explicit public-network interactive smoke remains intentionally ignored.

## Risks

- Reading a config before acquiring the standalone lock can lose a concurrent user's update; the prepare-under-lock transaction boundary is required rather than optional cleanup.
- Treating an existing config file as the collision prevents normal merge behavior, while blindly replacing it can destroy unrelated settings; collision checks must operate on the named entry and serialization must start from the adapter-loaded document.
- A Registry candidate can be valid for one selected target but not the complete target set; resolution must continue requiring one candidate representable by every selected target.
- JSON adapters preserve semantic keys but may reformat strict JSON; tests should assert semantic preservation rather than byte stability where comments are not supported.
- A test or diagnostic could accidentally read or print an environment value; use marker values in tests and assert they are absent from output, config, and transaction data.
- Standalone entries are intentionally unmanaged after success, so users must edit native files or use `--force` to replace them; documentation must not imply update or removal support.

## Rollback / Recovery

- Before commit, every target configuration is validated and staged beside its destination, and no project file changes if preparation fails.
- A normal apply error restores all sibling backups through the existing transaction rollback path.
- A process interruption leaves only external standalone control state and sibling transaction artifacts; the next standalone mutation for the same canonical project root performs digest-gated recovery before loading or applying new configuration.
- Reverting the feature requires no data migration because standalone installations create only target-native entries and no persisted aru schema; existing entries remain user-managed native configuration.

## Completion Checklist

- [x] `aru mcp add --target <target> ...` works without `aru init` for Registry, direct HTTPS, and direct stdio sources and creates no aru project state; focused direct tests pass, Registry/offline dispatch reaches the shared Registry resolver, and existing local Registry fixture tests pass.
- [x] Omitting targets in a terminal offers only Codex, Claude Code, Copilot CLI, and OpenCode with exact config destinations, while non-terminal omission fails before Registry access or writes; PTY and non-terminal assertions pass.
- [x] Existing native config files are merged safely, unrelated entries and supported comments survive, same-name entries require `--force`, and malformed input blocks the complete multi-target transaction; focused four-adapter, collision, force, and malformed tests pass.
- [x] Environment references remain placeholders, direct commands are never executed, offline policy is enforced, and no secret value is read or persisted; marker command and secret assertions pass.
- [x] Managed MCP add, update, remove, lock, sync, ownership, and locked replay behavior remains unchanged across all existing tests; the complete `mcp_cli`, policy, target, and full suites pass.
- [x] Standalone Skill behavior remains unchanged after transaction and chooser generalization; all standalone Skill and interactive regression tests pass.
- [x] Documentation, CLI help, deterministic plans, focused tests, formatting, Clippy, the complete Rust test suite, and `git diff --check` all agree with the delivered behavior.
