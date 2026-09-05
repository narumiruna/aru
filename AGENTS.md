# Repository Guidance

## Scope and Authority

This file applies to the entire repository.
Add a nested `AGENTS.md` only when a subtree has materially different rules.

For explanation, review, diagnosis, or planning requests, inspect and report without changing files.
For requested implementation or documentation work, make bounded local changes and run proportionate non-destructive checks.
Do not push, open or merge pull requests, create tags, dispatch workflows, publish releases, use aru's destructive `--force` takeover, or materially expand scope unless explicitly requested.

## Communication & Documentation

- Lead with the most important relevant information and omit anything unnecessary or repeated.
- Use clear structure, familiar words, and concise sentences.
- Explain the main idea simply before adding necessary detail.
- Keep information accurate.
- Make documented rules specific and verifiable.
- Draw diagrams using Mermaid syntax.

## Code Style

- Follow KISS (Keep It Simple) and YAGNI (You Aren't Gonna Need It).
- Prefer simple, minimal solutions over unnecessary complexity.
- Split source files over 1,000 lines along clear responsibility boundaries, or document why they must remain intact.
- Prefer aru CLI structure and terminology to follow uv and Cargo where domain semantics align; retain aru's fail-closed behavior over superficial parity.

## Project Map

- `src/cli.rs` defines the Clap command contract.
- `src/app.rs` and `src/app/` orchestrate commands.
- `src/manifest.rs` and `src/lockfile.rs` own persisted formats.
- `src/resolver.rs`, `src/registry.rs`, and `src/source/` resolve package inputs.
- `src/target/` owns target-specific projections.
- Keep target capability differences in `src/target/` instead of scattering target checks through orchestration code.
- `src/instruction/`, `src/skill.rs`, and `src/sync.rs` implement discovery and reconciliation.
- `src/ownership.rs` and `src/transaction.rs` protect user content and atomic multi-file updates.
- `tests/*.rs` exercise the public CLI.
- `tests/fixtures/contracts/` contains byte-stable format fixtures.
- Registry and instruction fixtures live under their corresponding fixture directories.

## Implementation Constraints

- Preserve aru's fail-closed model by validating the complete operation before any write.
- Reject unsupported or ambiguous inputs.
- Preserve drifted or unowned content for review.
- Keep discovery, serialization, hashing, operation plans, and output ordering deterministic and bounded.
- Pass Git and MCP commands as argument arrays.
- Never shell-expand package metadata.
- Never execute configured direct MCP commands.
- Never read secret values.
- Persist only environment-variable names or placeholders.
- Route mutating multi-file behavior through the existing ownership and transaction machinery.
- Preserve operation locking, durable journaling, sibling backups, atomic replacement, rollback, and digest-gated recovery.
- Update capability schema versions and golden fixtures when a persisted projection or lock identity changes.
- Do not silently broaden unsupported target behavior.

## Managed and Generated Paths

- `aru.toml` is human-maintained project intent.
- `aru.lock` is generated but committed.
- Regenerate `aru.lock` with the current source instead of editing hashes or resolved records by hand.
- Treat `.aru/`, `.agents/`, `.claude/`, `.codex/`, and `.mcp.json` as ignored local aru state or projections.
- Do not commit ignored aru state or use it as the canonical source for edits.
- Treat `/third_party/reference/` as ignored, read-only implementation research.
- Make product changes in this repository's `src/`, `tests/`, and documentation instead.
- When `aru.toml`, instruction sources, package requirements, targets, lock identity, or projections change, run `cargo run --locked -- sync`.
- Then verify replay with `cargo run --locked --quiet -- sync --locked --dry-run`.

## Verification

- During implementation, start with the narrowest useful compiler and test feedback.
- Default to `cargo check --locked` and add only flags relevant to the changed target or feature.
- Run focused tests after each coherent change.
- Do not use `--all-targets --all-features` after every small edit.
- For final Rust, manifest, lockfile, or fixture changes, run the following CI-equivalent gates.

```console
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

- Do not add a separate full `cargo check` because Clippy already performs compilation checks.
- Keep routine tests deterministic and offline.
- Use temporary Git repositories and local fixtures in routine tests.
- Set repository-local `commit.gpgsign=false` in every temporary Git fixture that creates commits so inherited signing helpers cannot break tests.
- Keep the explicit public Git smoke test ignored by default.
- For documentation-only changes, verify referenced paths and commands and inspect the rendered diff.
- No Markdown checker is configured.
- For release work, follow `docs/releasing.md`.
- Use `scripts/bump-version.sh` to keep `Cargo.toml` and `Cargo.lock` aligned.
- Run `cargo package --locked` for release work.
- Never move or reuse a published version tag.
