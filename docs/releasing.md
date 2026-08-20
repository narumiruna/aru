# Releasing aru

A version release starts from the `Bump version` GitHub Actions workflow.
Choose the SemVer component to bump; after the release commit passes formatting, Clippy, tests, and `cargo package`, the workflow uses `PAT_TOKEN` to atomically push `main` and an annotated `vX.Y.Z` tag.

The tag independently triggers:

- `Release` (`.github/workflows/release.yml`), which publishes GitHub archives and `SHA256SUMS`;
- `Publish` (`.github/workflows/publish.yml`), which publishes the `aru` crate to crates.io;
- `Publish PyPI` (`.github/workflows/pypi.yml`), which publishes `arust` binary wheels that expose the `aru` command;
- `Installer scripts` (`.github/workflows/installers.yml`), which runs isolated Unix and Windows installer tests.
A manual installer run additionally installs the latest published release through the hosted scripts.

The Release, Publish, and Publish PyPI workflows validate that the stable SemVer tag matches `Cargo.toml` and points to a commit on `main`.
Release archive builds set `ARU_BUILD_DISTRIBUTION=standalone` and verify the marker before packaging, enabling `aru self update`.
PyPI wheel and ordinary Cargo builds remain package-manager-owned and reject self-update.
The PyPI workflow publishes two wheels: manylinux2014 for x86_64 and macOS for Apple silicon.
Manual PyPI workflow runs build and test all wheels without entering the `release` environment or requesting an OIDC publish token.
Only a validated stable tag push can run the PyPI publish job.
A failed workflow can be rerun independently.

## One-time repository configuration

### GitHub token

Create a fine-grained repository token with read/write access to repository contents and store it as the Actions repository secret `PAT_TOKEN`.
The version-bump workflow needs a PAT rather than `GITHUB_TOKEN` so its tag push triggers the release and publish workflows.

### GitHub environment

Create an Actions environment named `release` without a required reviewer, then set its deployment tag policy to `v*.*.*`. The publish workflow additionally rejects tags that are not stable `vX.Y.Z` versions on `main`.

### crates.io Trusted Publishing

The crate already needs to exist on crates.io before Trusted Publishing can be configured.
In the `aru` crate settings, add this GitHub publisher:

| Setting | Value |
| --- | --- |
| Repository owner | `narumiruna` |
| Repository name | `aru` |
| Workflow filename | `publish.yml` |
| Environment | `release` |

The publish job exchanges GitHub's OIDC identity for a short-lived crates.io token.
No long-lived crates.io token is stored in GitHub.

### PyPI Trusted Publishing

The PyPI distribution is named `arust`, while its installed executable is `aru`.
Before the first release, add a pending Trusted Publisher for `arust` from the PyPI account publishing page, or add the publisher in the existing project's settings.

| Setting | Value |
| --- | --- |
| PyPI project name | `arust` |
| Repository owner | `narumiruna` |
| Repository name | `aru` |
| Workflow filename | `pypi.yml` |
| Environment | `release` |

The PyPI publish job uses GitHub OIDC and does not require a long-lived PyPI API token.

## Recovery

- If GitHub Release creation fails, rerun `Release` for the same tag.
  A published release is treated as complete, while an existing draft is resumed.
- If crates.io publishing fails before the version exists, rerun `Publish` for the same tag.
- If the crate version already exists, `Publish` verifies the package and exits successfully without trying to overwrite it.
- If PyPI publishing fails, fix the Trusted Publisher or workflow issue and rerun `Publish PyPI` for the same tag.
  Before using `skip-existing`, the workflow requires every existing PyPI filename and SHA-256 digest to match the locally rebuilt artifact.
  An exact matching subset is accepted as a partial release, and the workflow uploads the missing distributions.
  A digest conflict, unexpected remote filename, malformed response, or PyPI transport failure stops publication.
  After upload, the workflow waits for a bounded period and verifies that PyPI exposes the complete expected filename and digest set.
- Published crates.io and PyPI versions cannot be overwritten.
  Yank a bad version if necessary, then publish a new version without moving or reusing the old tag.
