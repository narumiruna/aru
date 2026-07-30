use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{AruError, Result};
use crate::manifest::SkillRequirement;

pub const DISCOVERY_MAX_DEPTH: usize = 6;
pub const DISCOVERY_MAX_DIRECTORIES: usize = 2_000;
pub const DISCOVERY_MAX_ENTRIES: usize = 20_000;
pub const SKILL_MD_MAX_BYTES: u64 = 1024 * 1024;
pub const SKILL_FILE_MAX_BYTES: u64 = 10 * 1024 * 1024;
pub const SKILL_TOTAL_MAX_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSkill {
    pub name: String,
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
}

pub fn discover_and_select(
    root: &Path,
    root_name: &str,
    requirement: &SkillRequirement,
) -> Result<Vec<DiscoveredSkill>> {
    let mut candidates = BTreeMap::<String, PathBuf>::new();

    if root.join("SKILL.md").is_file() {
        insert_candidate(&mut candidates, root_name, root.to_path_buf())?;
    }
    let conventional = root.join("skills");
    if conventional.exists() {
        discover_conventional(&conventional, &mut candidates)?;
    }
    for (expected_name, relative) in &requirement.paths {
        let relative_path = validate_relative_selector(relative)?;
        let selected = root.join(&relative_path);
        if !selected.join("SKILL.md").is_file() {
            return Err(AruError::msg(format!(
                "explicit skill path {relative:?} must directly contain SKILL.md"
            )));
        }
        let actual = parse_skill_name(&selected.join("SKILL.md"))?;
        if actual != *expected_name {
            return Err(AruError::msg(format!(
                "explicit path {relative:?} declares skill {actual:?}, not {expected_name:?}"
            )));
        }
        insert_candidate(&mut candidates, expected_name, selected)?;
    }

    if candidates.is_empty() {
        return Err(AruError::msg(
            "Git source exports no valid skills (expected SKILL.md or skills/**/SKILL.md)",
        ));
    }

    let mut selected_names: Vec<String> = if requirement.is_wildcard() {
        candidates
            .keys()
            .filter(|name| !requirement.exclude.contains(name))
            .cloned()
            .collect()
    } else {
        for name in &requirement.include {
            if !candidates.contains_key(name) {
                let available = candidates.keys().cloned().collect::<Vec<_>>().join(", ");
                return Err(AruError::msg(format!(
                    "skill {name:?} was not found; available skills: {available}"
                )));
            }
        }
        requirement.include.clone()
    };
    selected_names.sort();
    selected_names.dedup();

    if selected_names.is_empty() && !requirement.is_wildcard() {
        return Err(AruError::msg("skill selection is empty"));
    }

    let mut selected_paths: Vec<PathBuf> = Vec::new();
    let mut output = Vec::new();
    for name in selected_names {
        let path = candidates.get(&name).unwrap();
        for previous in &selected_paths {
            if path.starts_with(previous) || previous.starts_with(path) {
                return Err(AruError::msg(format!(
                    "selected skill trees overlap at {}",
                    path.display()
                )));
            }
        }
        let expected_parent = if path == root {
            root_name
        } else {
            path.file_name()
                .and_then(|part| part.to_str())
                .ok_or_else(|| AruError::msg("skill directory name is not UTF-8"))?
        };
        let declared = parse_skill_name(&path.join("SKILL.md"))?;
        if declared != expected_parent || declared != name {
            return Err(AruError::msg(format!(
                "SKILL.md name {declared:?} must match parent directory {expected_parent:?}"
            )));
        }
        let sha256 = canonical_skill_digest(path)?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| AruError::msg("internal error: discovered skill escaped source root"))?;
        let relative_path = if relative.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            portable_path(relative)?
        };
        selected_paths.push(path.clone());
        output.push(DiscoveredSkill {
            name,
            relative_path,
            absolute_path: path.clone(),
            sha256,
        });
    }
    output.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(output)
}

fn discover_conventional(root: &Path, candidates: &mut BTreeMap<String, PathBuf>) -> Result<()> {
    discover_conventional_with_limits(
        root,
        candidates,
        DISCOVERY_MAX_DEPTH,
        DISCOVERY_MAX_DIRECTORIES,
        DISCOVERY_MAX_ENTRIES,
    )
}

fn discover_conventional_with_limits(
    root: &Path,
    candidates: &mut BTreeMap<String, PathBuf>,
    max_depth: usize,
    max_directories: usize,
    max_entries: usize,
) -> Result<()> {
    if !root.is_dir() {
        return Err(AruError::msg("source skills path is not a directory"));
    }
    let mut directories = 0usize;
    let mut entries = 0usize;
    for item in WalkDir::new(root)
        .follow_links(false)
        .max_depth(max_depth + 2)
    {
        let item =
            item.map_err(|error| AruError::msg(format!("skill discovery failed: {error}")))?;
        entries += 1;
        if entries > max_entries {
            return Err(limit_error("entries", max_entries));
        }
        let depth = item.depth();
        if depth > max_depth + 1 {
            return Err(limit_error("depth", max_depth));
        }
        if item.file_type().is_dir() {
            directories += 1;
            if directories > max_directories {
                return Err(limit_error("directories", max_directories));
            }
            if depth <= max_depth && item.path().join("SKILL.md").is_file() {
                let name = parse_skill_name(&item.path().join("SKILL.md"))?;
                insert_candidate(candidates, &name, item.path().to_path_buf())?;
            }
        }
    }
    Ok(())
}

fn insert_candidate(
    candidates: &mut BTreeMap<String, PathBuf>,
    expected_name: &str,
    path: PathBuf,
) -> Result<()> {
    let name = parse_skill_name(&path.join("SKILL.md"))?;
    if name != expected_name {
        return Err(AruError::msg(format!(
            "SKILL.md name {name:?} must match parent directory {expected_name:?}"
        )));
    }
    if let Some(previous) = candidates.insert(name.clone(), path.clone())
        && previous != path
    {
        return Err(AruError::msg(format!(
            "duplicate skill name {name:?} at {} and {}",
            previous.display(),
            path.display()
        )));
    }
    Ok(())
}

fn parse_skill_name(path: &Path) -> Result<String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| AruError::msg(format!("could not inspect {}: {error}", path.display())))?;
    if !metadata.file_type().is_file() || metadata.len() > SKILL_MD_MAX_BYTES {
        return Err(AruError::msg(format!(
            "SKILL.md must be a regular file no larger than {} bytes: {}",
            SKILL_MD_MAX_BYTES,
            path.display()
        )));
    }
    let text = std::fs::read_to_string(path).map_err(|error| {
        AruError::msg(format!(
            "could not read {} as UTF-8: {error}",
            path.display()
        ))
    })?;
    let body = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .ok_or_else(|| AruError::msg(format!("{} has no YAML frontmatter", path.display())))?;
    let mut offset = 0usize;
    let mut end = None;
    for line in body.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            end = Some(offset);
            break;
        }
        offset += line.len();
    }
    let end = end.ok_or_else(|| {
        AruError::msg(format!(
            "{} has unterminated YAML frontmatter",
            path.display()
        ))
    })?;
    let frontmatter: Frontmatter = serde_yaml_ng::from_str(&body[..end])
        .map_err(|error| AruError::msg(format!("invalid SKILL.md frontmatter: {error}")))?;
    crate::manifest::validate_name(&frontmatter.name, "skill name")?;
    if frontmatter.description.trim().is_empty() || frontmatter.description.len() > 1024 {
        return Err(AruError::msg(
            "SKILL.md description must contain 1-1024 UTF-8 bytes",
        ));
    }
    Ok(frontmatter.name)
}

pub fn canonical_skill_digest(root: &Path) -> Result<String> {
    canonical_skill_digest_with_limits(root, SKILL_FILE_MAX_BYTES, SKILL_TOTAL_MAX_BYTES)
}

fn canonical_skill_digest_with_limits(
    root: &Path,
    file_max_bytes: u64,
    total_max_bytes: u64,
) -> Result<String> {
    let mut files = Vec::new();
    let mut folded = BTreeSet::new();
    let mut total_bytes = 0u64;
    for item in WalkDir::new(root).follow_links(false) {
        let item = item.map_err(|error| AruError::msg(format!("skill walk failed: {error}")))?;
        if item.depth() == 0 {
            continue;
        }
        let relative = item
            .path()
            .strip_prefix(root)
            .map_err(|_| AruError::msg("skill entry escaped its root"))?;
        let portable = portable_path(relative)?;
        validate_portable_components(&portable)?;
        let folded_path = portable.to_lowercase();
        if !folded.insert(folded_path) {
            return Err(AruError::msg(format!(
                "skill contains case-folding path collision at {portable:?}"
            )));
        }
        let file_type = item.file_type();
        if file_type.is_symlink() || (!file_type.is_file() && !file_type.is_dir()) {
            return Err(AruError::msg(format!(
                "skill contains a symlink or special entry at {portable:?}; MVP accepts only regular files and directories"
            )));
        }
        if file_type.is_file() {
            let metadata = item.metadata().map_err(|error| {
                AruError::msg(format!(
                    "could not inspect skill file {portable:?}: {error}"
                ))
            })?;
            if metadata.len() > file_max_bytes {
                return Err(AruError::msg(format!(
                    "skill file {portable:?} is {} bytes; limit is {} bytes",
                    metadata.len(),
                    file_max_bytes
                )));
            }
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| AruError::msg("skill byte count overflow"))?;
            if total_bytes > total_max_bytes {
                return Err(AruError::msg(format!(
                    "skill tree is {total_bytes} bytes; limit is {total_max_bytes} bytes"
                )));
            }
            files.push((portable, item.path().to_path_buf(), executable(&metadata)));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    hasher.update(b"aru-skill-digest-v1\0");
    for (relative, path, executable) in files {
        let relative_bytes = relative.as_bytes();
        hasher.update((relative_bytes.len() as u64).to_be_bytes());
        hasher.update(relative_bytes);
        hasher.update([u8::from(executable)]);
        let length = std::fs::metadata(&path)
            .map_err(|error| {
                AruError::msg(format!("could not inspect {}: {error}", path.display()))
            })?
            .len();
        hasher.update(length.to_be_bytes());
        let mut file = File::open(&path).map_err(|error| {
            AruError::msg(format!("could not read {}: {error}", path.display()))
        })?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|error| {
                AruError::msg(format!("could not hash {}: {error}", path.display()))
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

pub fn validate_relative_selector(value: &str) -> Result<PathBuf> {
    if value.is_empty()
        || value == "."
        || value.contains('\0')
        || value.contains('\\')
        || value.starts_with('/')
        || value.starts_with("//")
        || value.as_bytes().get(1) == Some(&b':')
    {
        return Err(AruError::msg(format!(
            "invalid skill path {value:?}; expected a portable repository-relative directory"
        )));
    }
    let path = Path::new(value);
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(AruError::msg(format!(
                "invalid skill path {value:?}; ., .., roots, and prefixes are forbidden"
            )));
        }
    }
    validate_portable_components(value)?;
    Ok(path.to_path_buf())
}

fn portable_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(AruError::msg("skill contains a non-relative path"));
        };
        let value = component
            .to_str()
            .ok_or_else(|| AruError::msg("skill contains a non-UTF-8 path"))?;
        parts.push(value);
    }
    Ok(parts.join("/"))
}

fn validate_portable_components(path: &str) -> Result<()> {
    for component in path.split('/') {
        if component.is_empty() || component.ends_with('.') || component.ends_with(' ') {
            return Err(AruError::msg(format!("non-portable skill path {path:?}")));
        }
        let stem = component
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit()
                && stem.as_bytes()[3] != b'0');
        if reserved
            || component
                .chars()
                .any(|character| "<>:\"|?*".contains(character))
        {
            return Err(AruError::msg(format!("non-portable skill path {path:?}")));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn limit_error(kind: &str, limit: usize) -> AruError {
    AruError::msg(format!(
        "skill discovery exceeded {kind} limit {limit}; result is truncated and no skills were selected"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(path: &Path, name: &str) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: test\n---\n# Test\n"),
        )
        .unwrap();
    }

    #[test]
    fn nested_collection_and_explicit_selection_are_stable() {
        let temporary = tempfile::tempdir().unwrap();
        write_skill(&temporary.path().join("skills/a/nested/alpha"), "alpha");
        let requirement = SkillRequirement {
            include: vec!["alpha".into()],
            ..SkillRequirement::default()
        };
        let selected = discover_and_select(temporary.path(), "repository", &requirement).unwrap();
        assert_eq!(selected[0].relative_path, "skills/a/nested/alpha");
        assert_eq!(selected[0].sha256.len(), 71);
    }

    #[test]
    fn rejects_path_traversal_and_symlinks() {
        assert!(validate_relative_selector("../skill").is_err());
        assert!(validate_relative_selector("C:/skill").is_err());
        let temporary = tempfile::tempdir().unwrap();
        let skill = temporary.path().join("skills/demo");
        write_skill(&skill, "demo");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("SKILL.md", skill.join("link")).unwrap();
            assert!(canonical_skill_digest(&skill).is_err());
        }
    }

    #[test]
    fn digest_delimits_paths_and_contents() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        write_skill(
            left.path(),
            left.path().file_name().unwrap().to_str().unwrap(),
        );
        write_skill(
            right.path(),
            right.path().file_name().unwrap().to_str().unwrap(),
        );
        std::fs::write(left.path().join("ab"), "c").unwrap();
        std::fs::write(right.path().join("a"), "bc").unwrap();
        assert_ne!(
            canonical_skill_digest(left.path()).unwrap(),
            canonical_skill_digest(right.path()).unwrap()
        );
    }

    #[test]
    fn discovery_depth_limit_fails_instead_of_returning_partial_results() {
        let temporary = tempfile::tempdir().unwrap();
        let mut deep = temporary.path().join("skills");
        for index in 0..=DISCOVERY_MAX_DEPTH + 1 {
            deep.push(format!("d{index}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        let mut candidates = BTreeMap::new();
        let error = discover_conventional(&temporary.path().join("skills"), &mut candidates)
            .unwrap_err()
            .to_string();
        assert!(error.contains("depth limit"));
        assert!(candidates.is_empty());
    }

    #[test]
    fn discovery_directory_and_entry_limits_fail_closed() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temporary.path().join("skills/a")).unwrap();
        std::fs::create_dir_all(temporary.path().join("skills/b")).unwrap();
        let mut candidates = BTreeMap::new();
        assert!(
            discover_conventional_with_limits(
                &temporary.path().join("skills"),
                &mut candidates,
                DISCOVERY_MAX_DEPTH,
                1,
                DISCOVERY_MAX_ENTRIES,
            )
            .is_err()
        );
        candidates.clear();
        assert!(
            discover_conventional_with_limits(
                &temporary.path().join("skills"),
                &mut candidates,
                DISCOVERY_MAX_DEPTH,
                DISCOVERY_MAX_DIRECTORIES,
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn duplicate_names_and_oversized_skill_markdown_are_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        write_skill(&temporary.path().join("skills/a/alpha"), "alpha");
        write_skill(&temporary.path().join("skills/b/alpha"), "alpha");
        assert!(
            discover_and_select(temporary.path(), "repository", &SkillRequirement::default())
                .is_err()
        );

        let oversized = tempfile::tempdir().unwrap();
        let directory = oversized.path().join("skills/demo");
        write_skill(&directory, "demo");
        File::options()
            .write(true)
            .open(directory.join("SKILL.md"))
            .unwrap()
            .set_len(SKILL_MD_MAX_BYTES + 1)
            .unwrap();
        assert!(
            discover_and_select(oversized.path(), "repository", &SkillRequirement::default())
                .is_err()
        );
    }

    #[test]
    fn cumulative_skill_byte_limit_is_enforced() {
        let temporary = tempfile::tempdir().unwrap();
        write_skill(
            temporary.path(),
            temporary.path().file_name().unwrap().to_str().unwrap(),
        );
        let skill_md_bytes = std::fs::metadata(temporary.path().join("SKILL.md"))
            .unwrap()
            .len();
        std::fs::write(temporary.path().join("a"), "12").unwrap();
        std::fs::write(temporary.path().join("b"), "34").unwrap();
        assert!(
            canonical_skill_digest_with_limits(
                temporary.path(),
                SKILL_FILE_MAX_BYTES,
                skill_md_bytes + 3,
            )
            .is_err()
        );
    }

    #[test]
    fn oversized_case_colliding_and_reserved_entries_fail_closed() {
        let oversized = tempfile::tempdir().unwrap();
        write_skill(
            oversized.path(),
            oversized.path().file_name().unwrap().to_str().unwrap(),
        );
        let file = File::create(oversized.path().join("large.bin")).unwrap();
        file.set_len(SKILL_FILE_MAX_BYTES + 1).unwrap();
        assert!(canonical_skill_digest(oversized.path()).is_err());

        let collision = tempfile::tempdir().unwrap();
        write_skill(
            collision.path(),
            collision.path().file_name().unwrap().to_str().unwrap(),
        );
        std::fs::write(collision.path().join("Readme"), "one").unwrap();
        std::fs::write(collision.path().join("README"), "two").unwrap();
        assert!(canonical_skill_digest(collision.path()).is_err());

        assert!(validate_relative_selector("skills/CON").is_err());
        assert!(validate_relative_selector("skills/bad:name").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn special_files_are_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        write_skill(
            temporary.path(),
            temporary.path().file_name().unwrap().to_str().unwrap(),
        );
        let status = std::process::Command::new("mkfifo")
            .arg(temporary.path().join("pipe"))
            .status()
            .unwrap();
        assert!(status.success());
        assert!(canonical_skill_digest(temporary.path()).is_err());
    }
}
