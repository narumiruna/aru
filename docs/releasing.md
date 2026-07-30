# Releasing aru

A version release starts from the `Bump version` GitHub Actions workflow. Choose the SemVer component to bump; after the release commit passes formatting, Clippy, tests, and `cargo package`, the workflow uses `PAT_TOKEN` to atomically push `main` and an annotated `vX.Y.Z` tag.

The tag independently triggers:

- `Release` (`.github/workflows/release.yml`), which publishes GitHub archives and `SHA256SUMS`;
- `Publish` (`.github/workflows/publish.yml`), which publishes the crate to crates.io.

Both workflows validate that the stable SemVer tag matches `Cargo.toml` and points to a commit on `main`. A failed workflow can be rerun independently.

## One-time repository configuration

### GitHub token

Create a fine-grained repository token with read/write access to repository contents and store it as the Actions repository secret `PAT_TOKEN`. The version-bump workflow needs a PAT rather than `GITHUB_TOKEN` so its tag push triggers the release and publish workflows.

### GitHub environment

Create an Actions environment named `release` without a required reviewer, then set its deployment tag policy to `v*.*.*`. The publish workflow additionally rejects tags that are not stable `vX.Y.Z` versions on `main`.

### crates.io Trusted Publishing

The crate already needs to exist on crates.io before Trusted Publishing can be configured. In the `aru` crate settings, add this GitHub publisher:

| Setting | Value |
| --- | --- |
| Repository owner | `narumiruna` |
| Repository name | `aru` |
| Workflow filename | `publish.yml` |
| Environment | `release` |

The publish job exchanges GitHub's OIDC identity for a short-lived crates.io token; no long-lived crates.io token is stored in GitHub.

## Recovery

- If GitHub Release creation fails, rerun `Release` for the same tag. A published release is treated as complete, while an existing draft is resumed.
- If crates.io publishing fails before the version exists, rerun `Publish` for the same tag.
- If the crate version already exists, `Publish` verifies the package and exits successfully without trying to overwrite it.
- Published crates.io versions cannot be overwritten. Yank a bad version if necessary, then publish a new version without moving or reusing the old tag.
