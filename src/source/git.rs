use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use semver::{Version, VersionReq};

use crate::error::{AruError, Result};
use crate::manifest::validate_branch_name;

pub const GIT_TAG_OUTPUT_MAX_BYTES: u64 = 10 * 1024 * 1024;
pub const GIT_TAG_REF_MAX_RECORDS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSource {
    pub identity: String,
    pub fetch: String,
    pub repository_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitResolution {
    pub version: String,
    pub revision: String,
}

pub fn canonicalize(project: &Path, input: &str) -> Result<GitSource> {
    validate_source_argument(input)?;

    if is_github_shorthand(input) {
        let mut parts = input.split('/');
        let owner = parts.next().unwrap();
        let repository = parts.next().unwrap().trim_end_matches(".git");
        return Ok(GitSource {
            identity: format!("git+https://github.com/{owner}/{repository}.git"),
            fetch: format!("https://github.com/{owner}/{repository}.git"),
            repository_name: repository.to_owned(),
        });
    }

    if let Some((user_host, path)) = parse_scp_like(input) {
        let (user, host) = user_host
            .split_once('@')
            .ok_or_else(|| AruError::msg("SCP-like Git source must include user@host:path"))?;
        let repository_name = repository_name(path)?;
        let mut normalized_path = path
            .trim_start_matches('/')
            .trim_end_matches('/')
            .to_owned();
        if !normalized_path.ends_with(".git") {
            normalized_path.push_str(".git");
        }
        return Ok(GitSource {
            identity: format!("git+ssh://{user}@{host}/{normalized_path}"),
            fetch: input.to_owned(),
            repository_name,
        });
    }

    if input.contains("://") {
        let mut parsed =
            url::Url::parse(input).map_err(|_| AruError::msg("invalid Git source URL"))?;
        match parsed.scheme() {
            "https" | "ssh" | "git" | "file" => {}
            _ => {
                return Err(AruError::msg(
                    "unsupported Git source scheme; use HTTPS, SSH, git, file, or a local path",
                ));
            }
        }
        if parsed.password().is_some()
            || (parsed.scheme() == "https" && !parsed.username().is_empty())
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(AruError::msg(
                "Git source URLs with embedded credentials, query parameters, or fragments are not accepted",
            ));
        }
        let fetch = input.to_owned();
        if parsed.scheme() != "ssh" {
            parsed
                .set_username("")
                .map_err(|_| AruError::msg("could not normalize Git source userinfo"))?;
        }
        parsed.set_password(None).ok();
        let repository_name = repository_name(parsed.path())?;
        let mut identity = parsed.to_string();
        if identity.ends_with('/') {
            identity.pop();
        }
        if matches!(parsed.scheme(), "https" | "git" | "ssh") && !identity.ends_with(".git") {
            identity.push_str(".git");
        }
        return Ok(GitSource {
            identity: format!("git+{identity}"),
            fetch,
            repository_name,
        });
    }

    let path = project.join(input);
    let canonical = path.canonicalize().map_err(|_| {
        AruError::msg(format!(
            "local Git source does not exist: {}",
            path.display()
        ))
    })?;
    let repository_name = repository_name(
        canonical
            .file_name()
            .and_then(|part| part.to_str())
            .unwrap_or(""),
    )?;
    let url = url::Url::from_file_path(&canonical)
        .map_err(|_| AruError::msg("could not convert local Git source to a file URL"))?;
    Ok(GitSource {
        identity: format!("git+{url}"),
        fetch: canonical.to_string_lossy().into_owned(),
        repository_name,
    })
}

pub fn resolve(
    source: &GitSource,
    version: Option<&str>,
    branch: Option<&str>,
    rev: Option<&str>,
) -> Result<GitResolution> {
    let references =
        usize::from(version.is_some()) + usize::from(branch.is_some()) + usize::from(rev.is_some());
    if references > 1 {
        return Err(AruError::msg(
            "--version, --branch, and --rev are mutually exclusive",
        ));
    }
    if let Some(branch) = branch {
        validate_branch_name(branch)?;
        return Ok(GitResolution {
            version: branch.to_owned(),
            revision: resolve_branch(source, branch)?,
        });
    }
    if let Some(revision) = rev {
        validate_revision(revision)?;
        return Ok(GitResolution {
            version: revision.to_owned(),
            revision: resolve_revision(source, revision)?,
        });
    }

    let requirement_text = version.unwrap_or("*");
    let requirement = VersionReq::parse(requirement_text).map_err(|error| {
        AruError::msg(format!(
            "invalid SemVer requirement {requirement_text:?}: {error}"
        ))
    })?;
    let mut matches = list_semver_tags(source)?
        .into_iter()
        .filter(|(candidate, _, _)| requirement.matches(candidate))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    let (selected, _tag, revision) = matches.pop().ok_or_else(|| {
        AruError::msg(format!(
            "Git source has no SemVer tag matching {requirement_text:?}; use --rev for an unversioned source"
        ))
    })?;
    Ok(GitResolution {
        version: selected.to_string(),
        revision,
    })
}

pub fn locked_version_matches(requirement: Option<&str>, version: &str) -> bool {
    let requirement = requirement.unwrap_or("*");
    let Ok(requirement) = VersionReq::parse(requirement) else {
        return false;
    };
    Version::parse(version).is_ok_and(|version| requirement.matches(&version))
}

pub fn checkout_exact(source: &GitSource, revision: &str, destination: &Path) -> Result<String> {
    validate_revision(revision)?;
    if destination.exists() {
        return Err(AruError::msg(format!(
            "checkout destination already exists: {}",
            destination.display()
        )));
    }
    std::fs::create_dir_all(destination)
        .map_err(|error| AruError::msg(format!("could not create checkout: {error}")))?;
    run_git(destination, &["init", "--quiet"])?;
    run_git(
        destination,
        &["remote", "add", "origin", source.fetch.as_str()],
    )?;
    run_git(
        destination,
        &["fetch", "--quiet", "--depth", "1", "origin", revision],
    )?;
    run_git(
        destination,
        &[
            "-c",
            "advice.detachedHead=false",
            "checkout",
            "--quiet",
            "FETCH_HEAD",
            "--",
        ],
    )?;
    let output = run_git(destination, &["rev-parse", "HEAD"])?;
    let resolved = String::from_utf8(output.stdout)
        .map_err(|_| AruError::msg("git returned a non-UTF-8 revision"))?
        .trim()
        .to_owned();
    if resolved.len() != 40 || !resolved.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AruError::msg("git returned an invalid commit revision"));
    }
    let index = run_git(destination, &["ls-files", "--stage"])?;
    if String::from_utf8_lossy(&index.stdout)
        .lines()
        .any(|line| line.starts_with("160000 "))
    {
        return Err(AruError::msg(
            "Git source contains submodules; MVP does not materialize or lock gitlinks",
        ));
    }
    let git_dir = destination.join(".git");
    std::fs::remove_dir_all(&git_dir)
        .map_err(|error| AruError::msg(format!("could not finalize Git checkout: {error}")))?;
    Ok(resolved.to_ascii_lowercase())
}

fn resolve_branch(source: &GitSource, branch: &str) -> Result<String> {
    let reference = format!("refs/heads/{branch}");
    let output = git_stdout_bounded(
        &[
            "ls-remote",
            "--heads",
            "--refs",
            "--",
            source.fetch.as_str(),
            reference.as_str(),
        ],
        GIT_TAG_OUTPUT_MAX_BYTES,
    )?;
    let stdout = String::from_utf8(output)
        .map_err(|_| AruError::msg("git returned non-UTF-8 branch data"))?;
    parse_branch_head(&stdout, &reference)
}

fn parse_branch_head(stdout: &str, expected_reference: &str) -> Result<String> {
    if stdout.lines().count() > GIT_TAG_REF_MAX_RECORDS {
        return Err(AruError::msg(format!(
            "Git branch inventory exceeds record limit {GIT_TAG_REF_MAX_RECORDS}"
        )));
    }
    let mut found = None;
    for (index, line) in stdout.lines().enumerate() {
        if index >= GIT_TAG_REF_MAX_RECORDS {
            return Err(AruError::msg(format!(
                "Git branch inventory exceeds record limit {GIT_TAG_REF_MAX_RECORDS}"
            )));
        }
        let Some((revision, reference)) = line.split_once('\t') else {
            return Err(AruError::msg("git returned malformed branch data"));
        };
        if reference != expected_reference
            || revision.len() != 40
            || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
            || found.is_some()
        {
            return Err(AruError::msg(
                "git returned ambiguous or malformed branch data",
            ));
        }
        found = Some(revision.to_ascii_lowercase());
    }
    found.ok_or_else(|| {
        let branch = expected_reference
            .strip_prefix("refs/heads/")
            .unwrap_or(expected_reference);
        AruError::msg(format!("Git source has no branch named {branch:?}"))
    })
}

fn resolve_revision(source: &GitSource, revision: &str) -> Result<String> {
    let temporary = tempfile::tempdir()
        .map_err(|error| AruError::msg(format!("could not create temporary checkout: {error}")))?;
    let checkout = temporary.path().join("source");
    checkout_exact(source, revision, &checkout)
}

fn list_semver_tags(source: &GitSource) -> Result<Vec<(Version, String, String)>> {
    let output = git_stdout_bounded(
        &["ls-remote", "--tags", "--", source.fetch.as_str()],
        GIT_TAG_OUTPUT_MAX_BYTES,
    )?;
    let stdout =
        String::from_utf8(output).map_err(|_| AruError::msg("git returned non-UTF-8 tag data"))?;
    parse_semver_tags(&stdout)
}

fn parse_semver_tags(stdout: &str) -> Result<Vec<(Version, String, String)>> {
    let mut references = std::collections::BTreeMap::new();
    for (index, line) in stdout.lines().enumerate() {
        if index >= GIT_TAG_REF_MAX_RECORDS {
            return Err(AruError::msg(format!(
                "Git tag inventory exceeds record limit {GIT_TAG_REF_MAX_RECORDS}"
            )));
        }
        let Some((revision, reference)) = line.split_once('\t') else {
            return Err(AruError::msg("git returned malformed tag data"));
        };
        if revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            references.insert(reference, revision.to_ascii_lowercase());
        }
    }
    let mut tags = Vec::new();
    for (reference, revision) in &references {
        let Some(tag) = reference.strip_prefix("refs/tags/") else {
            continue;
        };
        if tag.ends_with("^{}") {
            continue;
        }
        let semantic = tag.strip_prefix('v').unwrap_or(tag);
        if let Ok(version) = Version::parse(semantic) {
            let peeled_reference = format!("{reference}^{{}}");
            let peeled = references
                .get(peeled_reference.as_str())
                .unwrap_or(revision);
            tags.push((version, tag.to_owned(), peeled.clone()));
        }
    }
    Ok(tags)
}

fn git_stdout_bounded(arguments: &[&str], limit: u64) -> Result<Vec<u8>> {
    let mut child = Command::new("git")
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| AruError::msg(format!("could not execute git: {error}")))?;
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .take(limit + 1)
        .read_to_end(&mut output)
        .map_err(|error| AruError::msg(format!("could not read git output: {error}")))?;
    if output.len() as u64 > limit {
        let _ = child.kill();
        let _ = child.wait();
        return Err(AruError::msg(format!(
            "Git tag inventory exceeds output limit {limit} bytes"
        )));
    }
    let status = child
        .wait()
        .map_err(|error| AruError::msg(format!("could not wait for git: {error}")))?;
    if status.success() {
        Ok(output)
    } else {
        Err(AruError::msg(format!(
            "git command failed with status {status} (remote output redacted)"
        )))
    }
}

fn run_git(cwd: &Path, arguments: &[&str]) -> Result<Output> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| AruError::msg(format!("could not execute git: {error}")))?;
    check_git(output)
}

fn check_git(output: Output) -> Result<Output> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(AruError::msg(format!(
            "git command failed with status {} (remote output redacted)",
            output.status
        )))
    }
}

fn validate_source_argument(input: &str) -> Result<()> {
    if input.is_empty()
        || input.starts_with('-')
        || input
            .chars()
            .any(|character| character == '\0' || character.is_control())
    {
        return Err(AruError::msg("invalid Git source argument"));
    }
    Ok(())
}

fn validate_revision(revision: &str) -> Result<()> {
    if (7..=40).contains(&revision.len()) && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AruError::msg(
            "Git revision must be 7-40 hexadecimal characters",
        ))
    }
}

fn is_github_shorthand(input: &str) -> bool {
    let mut parts = input.split('/');
    let Some(owner) = parts.next() else {
        return false;
    };
    let Some(repository) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !owner.is_empty()
        && !repository.is_empty()
        && [owner, repository].iter().all(|part| {
            part.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        })
}

fn parse_scp_like(input: &str) -> Option<(&str, &str)> {
    if input.contains("://") {
        return None;
    }
    let (host, path) = input.split_once(':')?;
    if host.contains('@') && !path.is_empty() {
        Some((host, path))
    } else {
        None
    }
}

fn repository_name(path: &str) -> Result<String> {
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".git");
    if name.is_empty() {
        Err(AruError::msg("Git source has no repository name"))
    } else {
        Ok(name.to_owned())
    }
}

pub fn checkout_path(base: &Path, source_hash: &str, revision: &str) -> PathBuf {
    base.join("git").join(source_hash).join(revision)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repository: &Path, arguments: &[&str]) {
        assert!(
            Command::new("git")
                .current_dir(repository)
                .args(arguments)
                .status()
                .unwrap()
                .success()
        );
    }

    fn git_output(repository: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(repository)
            .args(arguments)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn canonicalizes_shorthand_without_credentials() {
        let source = canonicalize(Path::new("."), "narumiruna/skills").unwrap();
        assert_eq!(
            source.identity,
            "git+https://github.com/narumiruna/skills.git"
        );
        assert_eq!(source.repository_name, "skills");
    }

    #[test]
    fn rejects_credentials_and_option_injection_but_preserves_ssh_user() {
        assert!(canonicalize(Path::new("."), "--upload-pack=evil").is_err());
        assert!(
            canonicalize(
                Path::new("."),
                "https://embedded-token@example.com/owner/repository.git"
            )
            .is_err()
        );
        assert!(
            canonicalize(
                Path::new("."),
                "https://example.com/owner/repository.git?token=secret"
            )
            .is_err()
        );
        assert_eq!(
            canonicalize(Path::new("."), "ssh://git@example.com/team/repository.git")
                .unwrap()
                .identity,
            "git+ssh://git@example.com/team/repository.git"
        );
        assert!(
            resolve(
                &GitSource {
                    identity: "x".into(),
                    fetch: "x".into(),
                    repository_name: "x".into(),
                },
                None,
                None,
                Some("--help")
            )
            .is_err()
        );
    }

    #[test]
    fn tag_inventory_record_limit_fails_closed() {
        let line = "0123456789abcdef0123456789abcdef01234567\trefs/tags/1.0.0\n";
        let inventory = line.repeat(GIT_TAG_REF_MAX_RECORDS + 1);
        assert!(parse_semver_tags(&inventory).is_err());
    }

    #[test]
    fn branch_resolution_is_exact_moving_and_strictly_validated() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("repository");
        std::fs::create_dir(&repository).unwrap();
        git(&repository, &["init", "--quiet"]);
        git(&repository, &["config", "user.email", "test@example.com"]);
        git(&repository, &["config", "user.name", "Test"]);
        std::fs::write(repository.join("file"), "first").unwrap();
        git(&repository, &["add", "."]);
        git(&repository, &["commit", "--quiet", "-m", "first"]);
        git(&repository, &["tag", "1.0.0"]);
        git(&repository, &["branch", "feature/nested"]);
        let first = git_output(&repository, &["rev-parse", "HEAD"]);
        let source = canonicalize(temporary.path(), "repository").unwrap();

        let resolved = resolve(&source, None, Some("feature/nested"), None).unwrap();
        assert_eq!(resolved.version, "feature/nested");
        assert_eq!(resolved.revision, first.trim());

        std::fs::write(repository.join("file"), "second").unwrap();
        git(&repository, &["commit", "--quiet", "-am", "second"]);
        git(
            &repository,
            &["branch", "--force", "feature/nested", "HEAD"],
        );
        let second = resolve(&source, None, Some("feature/nested"), None).unwrap();
        assert_ne!(second.revision, resolved.revision);
        let default_release = resolve(&source, None, None, None).unwrap();
        assert_eq!(default_release.version, "1.0.0");
        assert_eq!(default_release.revision, first.trim());
        assert!(resolve(&source, None, Some("missing"), None).is_err());
        for invalid in ["-main", "bad..name", "wild*card", "@{upstream}"] {
            assert!(resolve(&source, None, Some(invalid), None).is_err());
        }
    }

    #[test]
    fn branch_head_parser_rejects_ambiguous_or_malformed_output() {
        let sha = "0123456789012345678901234567890123456789";
        assert_eq!(
            parse_branch_head(&format!("{sha}\trefs/heads/main\n"), "refs/heads/main").unwrap(),
            sha
        );
        assert!(parse_branch_head("malformed\n", "refs/heads/main").is_err());
        assert!(
            parse_branch_head(
                &format!("{sha}\trefs/heads/main\n{sha}\trefs/heads/main\n"),
                "refs/heads/main"
            )
            .is_err()
        );
        assert!(
            parse_branch_head(&format!("{sha}\trefs/heads/other\n"), "refs/heads/main").is_err()
        );
        let oversized = format!("{sha}\trefs/heads/main\n").repeat(GIT_TAG_REF_MAX_RECORDS + 1);
        assert!(
            parse_branch_head(&oversized, "refs/heads/main")
                .unwrap_err()
                .to_string()
                .contains("record limit")
        );
    }

    #[test]
    fn annotated_semver_tags_lock_the_peeled_commit() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("repository");
        std::fs::create_dir(&repository).unwrap();
        for arguments in [
            vec!["init", "--quiet"],
            vec!["config", "user.email", "git@example.com"],
            vec!["config", "user.name", "git test"],
        ] {
            assert!(
                Command::new("git")
                    .current_dir(&repository)
                    .args(arguments)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        std::fs::write(repository.join("file"), "content").unwrap();
        for arguments in [
            vec!["add", "file"],
            vec!["commit", "--quiet", "-m", "initial"],
            vec!["tag", "--annotate", "2.0.0", "-m", "release"],
        ] {
            assert!(
                Command::new("git")
                    .current_dir(&repository)
                    .args(arguments)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let expected = String::from_utf8(
            Command::new("git")
                .current_dir(&repository)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let bare = temporary.path().join("remote.git");
        assert!(
            Command::new("git")
                .current_dir(temporary.path())
                .args([
                    "clone",
                    "--quiet",
                    "--bare",
                    repository.to_str().unwrap(),
                    bare.to_str().unwrap(),
                ])
                .status()
                .unwrap()
                .success()
        );
        let source = canonicalize(temporary.path(), "remote.git").unwrap();
        let resolved = resolve(&source, Some("=2.0.0"), None, None).unwrap();
        assert_eq!(resolved.revision, expected.trim());
        let checkout = temporary.path().join("checkout");
        assert_eq!(
            checkout_exact(&source, &resolved.revision, &checkout).unwrap(),
            expected.trim()
        );
        assert_eq!(std::fs::read(checkout.join("file")).unwrap(), b"content");
    }
}
