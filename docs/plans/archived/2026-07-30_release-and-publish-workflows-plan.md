# Release and Publish Workflows Plan

## Goal

Automatically create a GitHub Release and publish `aru` to crates.io whenever the version-bump workflow pushes a stable `vX.Y.Z` tag with `PAT_TOKEN`, while keeping both distribution channels independently retryable and narrowly permissioned.

## Plan

- [x] Harden `.github/workflows/bump-version.yml` to run the committed bump script reliably, commit before verification, pass repository quality/package checks, create an annotated tag, and atomically push `main` plus the tag with `PAT_TOKEN`; verified by actionlint with ShellCheck and isolated major/minor/patch script tests.
- [x] Add `.github/workflows/release.yml` to validate a `vX.Y.Z` tag against `Cargo.toml` and `main`, build supported archives, generate checksums, and idempotently publish a GitHub Release; verified by actionlint, an isolated annotated-tag validation test, and a local static-musl archive/checksum build.
- [x] Add `.github/workflows/publish.yml` to validate the same tag, package with `--locked`, authenticate through crates.io Trusted Publishing, and idempotently publish the crate; verified by actionlint, `cargo package --locked --allow-dirty`, and the crates.io existing-version API check.
- [x] Configure the repository's `release` GitHub environment without a manual approval gate, and document the one-time crates.io Trusted Publishing binding needed for `.github/workflows/publish.yml`; verified through the GitHub API (`v*.*.*` tag policy) and crates.io API (publisher `narumiruna/aru`, `publish.yml`, `release`).
- [x] Run repository and workflow quality gates, inspect the final diff for unrelated changes, and record verification evidence. Evidence: `cargo fmt --all -- --check`, Clippy with warnings denied, 76 passing tests and one explicit network-smoke ignore, Cargo package verification, `git diff --check`, actionlint with ShellCheck, and release archive checks all passed.

## Risks

- crates.io versions are immutable; a failed release is recovered by rerunning only the failed workflow or publishing a newer version, never by overwriting a version.
- GitHub Release and crates.io publishing are intentionally independent, so one can succeed while the other fails; both workflows must safely handle reruns.
- Trusted Publishing requires a one-time crates.io owner setting before the first automated publish.

## Rollback / Recovery

- Before a crate is published, delete an incorrect unpublished tag and rerun the bump workflow with the intended version.
- After a crate is published, do not move or reuse its tag/version; yank the crate version if necessary and publish a corrected version.
- Rerun a failed GitHub workflow for the same tag; existing crate versions and GitHub releases are treated as already completed.

## Completion Checklist

- [x] A PAT-authenticated bump run can only push a tag after checks and package verification pass.
- [x] A stable version tag independently triggers both GitHub Release and crates.io publish workflows.
- [x] Workflow permissions are limited to each workflow's responsibilities.
- [x] Release archives and `SHA256SUMS` have deterministic names and contain the `aru` executable.
- [x] The crate package excludes repository-only files and passes Cargo verification.
- [x] All available local workflow, shell, formatting, lint, and test checks pass.
