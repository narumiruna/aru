use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{AruError, Result};
use crate::manifest::SkillRequirement;

mod projection;

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

pub fn discover_candidates(
    root: &Path,
    root_name: &str,
    explicit_paths: &BTreeMap<String, String>,
) -> Result<Vec<DiscoveredSkill>> {
    let mut candidates = BTreeMap::<String, PathBuf>::new();

    let ignored_projection_roots = projection::locked_projection_roots(root)?;
    discover_recursively(root, root_name, &ignored_projection_roots, &mut candidates)?;
    for (expected_name, relative) in explicit_paths {
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
            "Git source exports no valid skills (expected ./SKILL.md or **/SKILL.md)",
        ));
    }

    candidates
        .into_iter()
        .map(|(name, path)| {
            let relative = path.strip_prefix(root).map_err(|_| {
                AruError::msg("internal error: discovered skill escaped source root")
            })?;
            let relative_path = if relative.as_os_str().is_empty() {
                ".".to_owned()
            } else {
                portable_path(relative)?
            };
            Ok(DiscoveredSkill {
                name,
                relative_path,
                sha256: canonical_skill_digest(&path)?,
                absolute_path: path,
            })
        })
        .collect()
}

pub fn select_candidates(
    candidates: Vec<DiscoveredSkill>,
    requirement: &SkillRequirement,
) -> Result<Vec<DiscoveredSkill>> {
    let available: BTreeMap<_, _> = candidates
        .into_iter()
        .map(|candidate| (candidate.name.clone(), candidate))
        .collect();
    let mut selected_names: Vec<String> = if requirement.is_wildcard() {
        available
            .keys()
            .filter(|name| !requirement.exclude.contains(name))
            .cloned()
            .collect()
    } else {
        for name in &requirement.include {
            if !available.contains_key(name) {
                let available = available.keys().cloned().collect::<Vec<_>>().join(", ");
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
        let candidate = available.get(&name).unwrap();
        for previous in &selected_paths {
            if candidate.absolute_path.starts_with(previous)
                || previous.starts_with(&candidate.absolute_path)
            {
                return Err(AruError::msg(format!(
                    "selected skill trees overlap at {}",
                    candidate.absolute_path.display()
                )));
            }
        }
        selected_paths.push(candidate.absolute_path.clone());
        output.push(candidate.clone());
    }
    Ok(output)
}

pub fn discover_and_select(
    root: &Path,
    root_name: &str,
    requirement: &SkillRequirement,
) -> Result<Vec<DiscoveredSkill>> {
    let candidates = discover_candidates(root, root_name, &requirement.paths)?;
    select_candidates(candidates, requirement)
}

fn discover_recursively(
    root: &Path,
    root_name: &str,
    ignored_projection_roots: &BTreeSet<PathBuf>,
    candidates: &mut BTreeMap<String, PathBuf>,
) -> Result<()> {
    discover_recursively_with_limits(
        root,
        root_name,
        ignored_projection_roots,
        candidates,
        DISCOVERY_MAX_DEPTH,
        DISCOVERY_MAX_DIRECTORIES,
        DISCOVERY_MAX_ENTRIES,
    )
}

fn discover_recursively_with_limits(
    root: &Path,
    root_name: &str,
    ignored_projection_roots: &BTreeSet<PathBuf>,
    candidates: &mut BTreeMap<String, PathBuf>,
    max_depth: usize,
    max_directories: usize,
    max_entries: usize,
) -> Result<()> {
    if !root.is_dir() {
        return Err(AruError::msg("skill source root is not a directory"));
    }
    let mut directories = 0usize;
    let mut entries = 0usize;
    let walker = WalkDir::new(root)
        .follow_links(false)
        .max_depth(max_depth + 2)
        .into_iter()
        .filter_entry(|item| !ignored_projection_roots.contains(item.path()));
    for item in walker {
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
                let expected_name = if depth == 0 {
                    root_name
                } else {
                    item.file_name()
                        .to_str()
                        .ok_or_else(|| AruError::msg("skill directory name is not valid UTF-8"))?
                };
                insert_candidate(candidates, expected_name, item.path().to_path_buf())?;
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
    canonical_skill_digest_with_structural_budget(root, file_max_bytes, total_max_bytes, None)
}

struct SkillTreeStructuralBudget {
    max_depth: usize,
    max_directories: usize,
    max_entries: usize,
    directories: usize,
    entries: usize,
}

impl SkillTreeStructuralBudget {
    fn new(max_depth: usize, max_directories: usize, max_entries: usize) -> Self {
        Self {
            max_depth,
            max_directories,
            max_entries,
            directories: 0,
            entries: 0,
        }
    }

    fn consume(&mut self, item: &walkdir::DirEntry) -> Result<()> {
        if self.entries >= self.max_entries {
            return Err(skill_tree_limit_error("entries", self.max_entries));
        }
        self.entries += 1;
        if item.depth() > self.max_depth + 1 {
            return Err(skill_tree_limit_error("depth", self.max_depth));
        }
        if item.file_type().is_dir() {
            if self.directories >= self.max_directories {
                return Err(skill_tree_limit_error("directories", self.max_directories));
            }
            self.directories += 1;
        }
        Ok(())
    }
}

fn canonical_skill_digest_with_structural_budget(
    root: &Path,
    file_max_bytes: u64,
    total_max_bytes: u64,
    mut structural_budget: Option<&mut SkillTreeStructuralBudget>,
) -> Result<String> {
    let mut files = Vec::new();
    let mut folded = BTreeSet::new();
    let mut total_bytes = 0u64;
    let walker = WalkDir::new(root).follow_links(false).into_iter();
    for item in walker {
        let item = item.map_err(|error| AruError::msg(format!("skill walk failed: {error}")))?;
        if let Some(budget) = structural_budget.as_deref_mut() {
            budget.consume(&item)?;
        }
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

fn skill_tree_limit_error(kind: &str, limit: usize) -> AruError {
    AruError::msg(format!("skill tree exceeded {kind} limit {limit}"))
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

    fn write_lock(path: &Path, skills: &[(&str, &[crate::manifest::Target])]) {
        let mut lock = crate::lockfile::Lockfile::empty();
        for (index, (name, targets)) in skills.iter().enumerate() {
            let sha256 = targets
                .iter()
                .find_map(|target| {
                    let projection = crate::target::spec(*target).project_skills;
                    projection
                        .starts_with('.')
                        .then(|| canonical_skill_digest(&path.join(projection).join(name)).ok())
                        .flatten()
                })
                .unwrap_or_else(|| format!("sha256:{}", "0".repeat(64)));
            lock.skill_packages.push(crate::lockfile::SkillPackage {
                source: format!("git+https://example.com/source-{index}.git"),
                requirement: "version:*".into(),
                version: "1.0.0".into(),
                revision: format!("{index:040x}"),
                repository_name: format!("source-{index}"),
                targets: targets.to_vec(),
                skills: vec![crate::lockfile::LockedSkill {
                    name: (*name).into(),
                    path: format!("skills/{name}"),
                    sha256,
                    origin: None,
                }],
            });
        }
        std::fs::write(path.join(crate::lockfile::LOCK_FILE), lock.bytes().unwrap()).unwrap();
    }

    #[test]
    fn repository_wide_discovery_and_explicit_selection_are_stable() {
        let temporary = tempfile::tempdir().unwrap();
        write_skill(
            &temporary.path().join("collections/a/nested/alpha"),
            "alpha",
        );
        let requirement = SkillRequirement {
            include: vec!["alpha".into()],
            ..SkillRequirement::default()
        };
        let selected = discover_and_select(temporary.path(), "repository", &requirement).unwrap();
        assert_eq!(selected[0].relative_path, "collections/a/nested/alpha");
        assert_eq!(selected[0].sha256.len(), 71);
    }

    #[test]
    fn repository_wide_discovery_includes_root_and_top_level_skills() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("repository");
        write_skill(&root, "repository");
        write_skill(&root.join("benchmark-model"), "benchmark-model");

        let candidates = discover_candidates(&root, "repository", &BTreeMap::new()).unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| (candidate.name.as_str(), candidate.relative_path.as_str()))
                .collect::<Vec<_>>(),
            [("benchmark-model", "benchmark-model"), ("repository", ".")]
        );
    }

    #[test]
    fn discovery_ignores_only_actual_locked_projection_targets() {
        use crate::manifest::Target;

        let temporary = tempfile::tempdir().unwrap();
        write_skill(&temporary.path().join("skills/source"), "source");
        write_skill(
            &temporary.path().join(".agents/skills/installed"),
            "installed",
        );
        write_skill(
            &temporary.path().join(".pi/skills/pi-installed"),
            "pi-installed",
        );
        write_skill(
            &temporary.path().join(".claude/skills/claude-installed"),
            "claude-installed",
        );
        write_skill(
            &temporary.path().join(".agents/skills/claude-only"),
            "claude-only",
        );
        write_skill(
            &temporary.path().join(".agents/skills/openclaw-only"),
            "openclaw-only",
        );
        write_skill(
            &temporary.path().join(".claude/skills/untracked"),
            "untracked",
        );
        write_lock(
            temporary.path(),
            &[
                ("installed", &[Target::Codex]),
                ("pi-installed", &[Target::Pi]),
                ("claude-installed", &[Target::Claude]),
                ("claude-only", &[Target::Claude]),
                ("openclaw-only", &[Target::Openclaw]),
            ],
        );

        let candidates =
            discover_candidates(temporary.path(), "repository", &BTreeMap::new()).unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.name.as_str())
                .collect::<Vec<_>>(),
            ["claude-only", "openclaw-only", "source", "untracked"]
        );

        let explicit = BTreeMap::from([("installed".into(), ".agents/skills/installed".into())]);
        let candidates = discover_candidates(temporary.path(), "repository", &explicit).unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.name.as_str())
                .collect::<Vec<_>>(),
            [
                "claude-only",
                "installed",
                "openclaw-only",
                "source",
                "untracked"
            ]
        );
    }

    #[test]
    fn discovery_preserves_drifted_locked_projection_content() {
        use crate::manifest::Target;

        let temporary = tempfile::tempdir().unwrap();
        let projection = temporary.path().join(".agents/skills/drifted");
        write_skill(&projection, "drifted");
        write_lock(temporary.path(), &[("drifted", &[Target::Codex])]);
        std::fs::write(projection.join("notes.md"), "authored after lock\n").unwrap();

        let candidates =
            discover_candidates(temporary.path(), "repository", &BTreeMap::new()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "drifted");
        assert_eq!(candidates[0].relative_path, ".agents/skills/drifted");
    }

    #[test]
    fn discovery_prunes_locked_projection_subtrees() {
        use crate::manifest::Target;

        let temporary = tempfile::tempdir().unwrap();
        write_skill(&temporary.path().join("skills/source"), "source");
        let installed = temporary.path().join(".agents/skills/installed");
        write_skill(&installed, "installed");
        write_skill(&installed.join("nested"), "nested");
        write_lock(temporary.path(), &[("installed", &[Target::Codex])]);

        let candidates =
            discover_candidates(temporary.path(), "repository", &BTreeMap::new()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "source");
    }

    #[test]
    fn projection_digest_enforces_discovery_structure_limits() {
        use crate::manifest::Target;

        let temporary = tempfile::tempdir().unwrap();
        let projection = temporary.path().join(".agents/skills/installed");
        write_skill(&projection, "installed");
        let mut deep = projection;
        for index in 0..=DISCOVERY_MAX_DEPTH + 1 {
            deep.push(format!("d{index}"));
        }
        std::fs::create_dir_all(deep).unwrap();
        write_lock(temporary.path(), &[("installed", &[Target::Codex])]);
        assert!(
            projection::locked_projection_roots(temporary.path())
                .unwrap()
                .is_empty()
        );

        let entries = tempfile::tempdir().unwrap();
        write_skill(
            entries.path(),
            entries.path().file_name().unwrap().to_str().unwrap(),
        );
        let mut budget =
            SkillTreeStructuralBudget::new(DISCOVERY_MAX_DEPTH, DISCOVERY_MAX_DIRECTORIES, 1);
        let error = canonical_skill_digest_with_structural_budget(
            entries.path(),
            SKILL_FILE_MAX_BYTES,
            SKILL_TOTAL_MAX_BYTES,
            Some(&mut budget),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("entries limit"));
    }

    #[test]
    fn projection_digests_share_one_discovery_budget() {
        use crate::manifest::Target;

        let temporary = tempfile::tempdir().unwrap();
        let first = temporary.path().join(".agents/skills/first");
        let second = temporary.path().join(".agents/skills/second");
        write_skill(&first, "first");
        write_skill(&second, "second");
        write_lock(
            temporary.path(),
            &[("first", &[Target::Codex]), ("second", &[Target::Codex])],
        );

        let ignored = projection::locked_projection_roots_with_limits(
            temporary.path(),
            u64::MAX,
            DISCOVERY_MAX_DEPTH,
            DISCOVERY_MAX_DIRECTORIES,
            3,
        )
        .unwrap();

        assert_eq!(ignored, BTreeSet::from([first]));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_projection_ancestor_is_not_hashed_or_pruned() {
        use crate::manifest::Target;

        let temporary = tempfile::tempdir().unwrap();
        let projection = temporary.path().join(".agents/skills/installed");
        write_skill(&projection, "installed");
        write_lock(temporary.path(), &[("installed", &[Target::Codex])]);

        let external = tempfile::tempdir().unwrap();
        let external_agents = external.path().join("agents");
        std::fs::rename(temporary.path().join(".agents"), &external_agents).unwrap();
        std::os::unix::fs::symlink(&external_agents, temporary.path().join(".agents")).unwrap();

        assert!(
            projection::locked_projection_roots(temporary.path())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn oversized_source_lock_fails_instead_of_disabling_projection_filtering() {
        use crate::manifest::Target;

        let temporary = tempfile::tempdir().unwrap();
        write_lock(temporary.path(), &[("installed", &[Target::Codex])]);
        let error = projection::locked_projection_roots_with_limit(temporary.path(), 1)
            .unwrap_err()
            .to_string();
        assert!(error.contains("source aru.lock is"));
        assert!(error.contains("skill discovery limit is 1 byte"));
    }

    #[test]
    fn invalid_source_lock_does_not_hide_projection_directory_skills() {
        let temporary = tempfile::tempdir().unwrap();
        write_skill(&temporary.path().join(".agents/skills/source"), "source");
        std::fs::write(
            temporary.path().join(crate::lockfile::LOCK_FILE),
            "not valid TOML [",
        )
        .unwrap();

        let candidates =
            discover_candidates(temporary.path(), "repository", &BTreeMap::new()).unwrap();
        assert_eq!(candidates[0].name, "source");
    }

    #[test]
    fn candidate_inventory_is_sorted_and_includes_explicit_paths() {
        let temporary = tempfile::tempdir().unwrap();
        write_skill(&temporary.path().join("skills/zeta"), "zeta");
        write_skill(&temporary.path().join("skills/alpha"), "alpha");
        write_skill(&temporary.path().join("extras/custom"), "custom");
        let paths = BTreeMap::from([("custom".into(), "extras/custom".into())]);

        let candidates = discover_candidates(temporary.path(), "repository", &paths).unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "custom", "zeta"]
        );
        assert_eq!(candidates[1].relative_path, "extras/custom");
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.sha256.starts_with("sha256:"))
        );
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
        let error = discover_recursively(
            &temporary.path().join("skills"),
            "skills",
            &BTreeSet::new(),
            &mut candidates,
        )
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
            discover_recursively_with_limits(
                &temporary.path().join("skills"),
                "skills",
                &BTreeSet::new(),
                &mut candidates,
                DISCOVERY_MAX_DEPTH,
                1,
                DISCOVERY_MAX_ENTRIES,
            )
            .is_err()
        );
        candidates.clear();
        assert!(
            discover_recursively_with_limits(
                &temporary.path().join("skills"),
                "skills",
                &BTreeSet::new(),
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
