# Repository Guidance

## Scope and authority

This file applies to the entire repository. Add a nested `AGENTS.md` only when a subtree has materially different rules.

For explanation, review, diagnosis, or planning requests, inspect and report without changing files. For requested implementation or documentation work, make bounded local changes and run proportionate non-destructive checks. Do not push, open or merge pull requests, create tags, dispatch workflows, publish releases, use aru's destructive `--force` takeover, or materially expand scope unless explicitly requested.

## Project map

- `src/cli.rs` defines the Clap command contract; `src/app.rs` and `src/app/` orchestrate commands.
- `src/manifest.rs` and `src/lockfile.rs` own persisted formats; `src/resolver.rs`, `src/registry.rs`, and `src/source/` resolve package inputs.
- `src/target/` owns target-specific projections. Keep target capability differences there instead of scattering target checks through orchestration code.
- `src/instruction/`, `src/skill.rs`, and `src/sync.rs` implement discovery and reconciliation.
- `src/ownership.rs` and `src/transaction.rs` protect user content and atomic multi-file updates.
- `tests/*.rs` exercise the public CLI. `tests/fixtures/contracts/` contains byte-stable format fixtures; Registry and instruction fixtures live under their corresponding fixture directories.

## Implementation constraints

- Preserve aru's fail-closed model: validate the complete operation before writes, reject unsupported or ambiguous inputs, and preserve drifted or unowned content for review.
- Keep discovery, serialization, hashing, operation plans, and output ordering deterministic and bounded.
- Pass Git and MCP commands as argument arrays. Never shell-expand package metadata, execute configured direct MCP commands, or read secret values; persist only environment-variable names or placeholders.
- Route mutating multi-file behavior through the existing ownership and transaction machinery. Preserve operation locking, durable journaling, sibling backups, atomic replacement, rollback, and digest-gated recovery.
- Update capability schema versions and golden fixtures when a persisted projection or lock identity changes. Do not silently broaden unsupported target behavior.

## Managed and generated paths

- `aru.toml` is human-maintained project intent. `aru.lock` is generated but committed; regenerate it with the current source rather than editing hashes or resolved records by hand.
- Treat `.aru/`, `.agents/`, `.claude/`, `.codex/`, and `.mcp.json` as ignored local aru state or projections in this repository. Do not commit or use them as canonical source edits.
- Treat `/third_party/reference/` as ignored, read-only implementation research. Make product changes in this repository's `src/`, `tests/`, and docs instead.
- When `aru.toml`, instruction sources, package requirements, targets, lock identity, or projections change, run `cargo run --locked -- sync`, then verify replay with `cargo run --locked --quiet -- sync --locked --dry-run`.

## Verification

During implementation, use the narrowest useful compiler and test feedback: default to `cargo check --locked`, add only flags for the relevant target or feature, and run focused tests after a coherent change. Do not use `--all-targets --all-features` after every small edit.

For final verification of Rust, manifest, lockfile, or fixture changes, run the CI-equivalent gates below. Do not add a separate full `cargo check`; Clippy already performs compilation checks.

```console
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Routine tests must remain deterministic and offline, using temporary Git repositories and local fixtures. Keep the explicit public Git smoke test ignored by default. For documentation-only changes, verify referenced paths and commands and inspect the rendered diff; no Markdown checker is configured.

For release-specific work, follow `docs/releasing.md`; use `scripts/bump-version.sh` to keep `Cargo.toml` and `Cargo.lock` aligned and run `cargo package --locked`. Never move or reuse a published version tag.
