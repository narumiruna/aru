use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

use crate::digest::sha256_bytes;
use crate::error::{AruError, IoContext, Result};
use crate::instruction::{DiscoveredInstruction, InstructionScope, InstructionUnit};
use crate::manifest::{InstructionSourceScope, Manifest};
use crate::target::{InstructionCapability, capabilities};

const MAX_DEPTH: usize = 64;
const MAX_ENTRIES: usize = 100_000;
const MAX_SOURCE_BYTES: u64 = 1024 * 1024;
const RESERVED_MARKER: &str = "<!-- aru:instruction:";

pub fn discover(project: &Path, manifest: &Manifest) -> Result<Vec<DiscoveredInstruction>> {
    if manifest.instructions.sources.is_empty() {
        return Ok(Vec::new());
    }
    let patterns = manifest
        .instructions
        .sources
        .iter()
        .flat_map(|source| source.files.iter());
    let inventory = inventory(project, instruction_search_roots(patterns))?;
    let mut discovered = BTreeMap::<String, DiscoveredInstruction>::new();
    let mut folded_sources = BTreeMap::<String, String>::new();

    for declaration in &manifest.instructions.sources {
        let includes = compile(&declaration.files)?;
        let excludes = compile(&declaration.exclude)?;
        let targets: BTreeSet<_> = if declaration.targets.is_empty() {
            crate::target::instruction_targets(&manifest.project.targets)
                .into_iter()
                .collect()
        } else {
            declaration.targets.iter().copied().collect()
        };
        let mut matches = 0_usize;

        for (relative, entry) in &inventory {
            if !includes.is_match(relative) || excludes.is_match(relative) {
                continue;
            }
            matches += 1;
            if discovered.contains_key(relative) {
                return Err(AruError::msg(format!(
                    "instruction source {relative:?} is matched by more than one declaration"
                )));
            }
            reject_output_source(relative)?;
            validate_portable_source_path(relative)?;
            let folded = relative.to_lowercase();
            if let Some(previous) = folded_sources.insert(folded, relative.clone()) {
                return Err(AruError::msg(format!(
                    "instruction sources {previous:?} and {relative:?} collide on case-insensitive filesystems"
                )));
            }
            if entry.is_symlink {
                return Err(AruError::msg(format!(
                    "instruction source {relative:?} must not be a symlink"
                )));
            }
            if !entry.is_file {
                return Err(AruError::msg(format!(
                    "instruction source {relative:?} must be a regular file"
                )));
            }
            if entry.size > MAX_SOURCE_BYTES {
                return Err(AruError::msg(format!(
                    "instruction source {relative:?} exceeds {MAX_SOURCE_BYTES} bytes"
                )));
            }
            let path = project.join(relative);
            let bytes = std::fs::read(&path).at(&path)?;
            if bytes.len() as u64 > MAX_SOURCE_BYTES {
                return Err(AruError::msg(format!(
                    "instruction source {relative:?} exceeds {MAX_SOURCE_BYTES} bytes"
                )));
            }
            let content = String::from_utf8(bytes.clone()).map_err(|_| {
                AruError::msg(format!("instruction source {relative:?} is not UTF-8"))
            })?;
            if content.contains(RESERVED_MARKER) {
                return Err(AruError::msg(format!(
                    "instruction source {relative:?} contains reserved aru marker text"
                )));
            }
            let scope = if declaration.scope == Some(InstructionSourceScope::SourceDirectory) {
                if Path::new(relative)
                    .file_name()
                    .and_then(|name| name.to_str())
                    != Some("AGENTS.md")
                {
                    return Err(AruError::msg(format!(
                        "source-directory instruction source {relative:?} must be named AGENTS.md"
                    )));
                }
                InstructionScope::SourceDirectory {
                    directory: portable_parent(relative),
                }
            } else {
                if Path::new(relative)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    != Some("md")
                {
                    return Err(AruError::msg(format!(
                        "path-specific instruction source {relative:?} must be a Markdown file"
                    )));
                }
                let mut globs = declaration.apply_to.clone();
                globs.sort();
                globs.dedup();
                InstructionScope::ApplyTo { globs }
            };
            validate_target_scope(relative, &scope, &targets)?;
            discovered.insert(
                relative.clone(),
                DiscoveredInstruction {
                    unit: InstructionUnit {
                        source: PathBuf::from(relative),
                        scope,
                        targets: targets.clone(),
                        source_sha256: sha256_bytes(&bytes),
                        managed: false,
                    },
                    content,
                },
            );
        }

        if matches == 0 {
            return Err(AruError::msg(format!(
                "instruction source declaration {:?} matched no files",
                declaration.files
            )));
        }
    }

    Ok(discovered.into_values().collect())
}

#[derive(Debug)]
struct InventoryEntry {
    is_file: bool,
    is_symlink: bool,
    size: u64,
}

fn instruction_search_roots<'a>(patterns: impl Iterator<Item = &'a String>) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    for pattern in patterns {
        let mut root = PathBuf::new();
        for component in pattern.split('/') {
            if component.contains(['*', '?', '[', '{']) {
                break;
            }
            root.push(component);
        }
        roots.insert(root);
    }
    let mut roots: Vec<_> = roots.into_iter().collect();
    roots.sort_by_key(|root| (root.components().count(), root.clone()));
    let mut minimal = Vec::<PathBuf>::new();
    for root in roots {
        if minimal.iter().any(|ancestor| {
            ancestor.as_os_str().is_empty() || (ancestor != &root && root.starts_with(ancestor))
        }) {
            continue;
        }
        minimal.push(root);
    }
    minimal
}

fn inventory(
    project: &Path,
    search_roots: impl IntoIterator<Item = PathBuf>,
) -> Result<BTreeMap<String, InventoryEntry>> {
    let mut output = BTreeMap::new();
    let mut count = 0_usize;
    for search_root in search_roots {
        if excluded_directory(&search_root) {
            continue;
        }
        let root_depth = search_root.components().count();
        if root_depth > MAX_DEPTH {
            return Err(AruError::msg(format!(
                "instruction discovery exceeds maximum depth {MAX_DEPTH}"
            )));
        }
        let Some(search_root) = safe_search_root(project, &search_root)? else {
            continue;
        };
        let walker = WalkDir::new(project.join(&search_root))
            .follow_links(false)
            .max_depth(MAX_DEPTH - root_depth)
            .into_iter()
            .filter_entry(|entry| {
                let Ok(relative) = entry.path().strip_prefix(project) else {
                    return false;
                };
                relative.as_os_str().is_empty() || !excluded_directory(relative)
            });
        for item in walker {
            let item = item
                .map_err(|error| AruError::msg(format!("instruction discovery failed: {error}")))?;
            let relative = item
                .path()
                .strip_prefix(project)
                .map_err(|_| AruError::msg("instruction path escaped project root"))?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            let depth = relative.components().count();
            if depth == MAX_DEPTH
                && item.file_type().is_dir()
                && std::fs::read_dir(item.path())
                    .at(item.path())?
                    .next()
                    .is_some()
            {
                return Err(AruError::msg(format!(
                    "instruction discovery exceeds maximum depth {MAX_DEPTH}"
                )));
            }
            count += 1;
            if count > MAX_ENTRIES {
                return Err(AruError::msg(format!(
                    "instruction discovery exceeds {MAX_ENTRIES} entries"
                )));
            }
            let Ok(portable) = portable_path(relative) else {
                continue;
            };
            let metadata = std::fs::symlink_metadata(item.path()).at(item.path())?;
            output.insert(
                portable,
                InventoryEntry {
                    is_file: metadata.is_file(),
                    is_symlink: metadata.file_type().is_symlink(),
                    size: metadata.len(),
                },
            );
        }
    }
    Ok(output)
}

fn safe_search_root(project: &Path, search_root: &Path) -> Result<Option<PathBuf>> {
    let mut candidate = PathBuf::new();
    for component in search_root.components() {
        candidate.push(component);
        let path = project.join(&candidate);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(AruError::Io {
                    path: path.clone(),
                    source,
                });
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Ok(Some(candidate));
        }
    }
    Ok(Some(search_root.to_path_buf()))
}

fn excluded_directory(path: &Path) -> bool {
    matches!(
        path.components().next(),
        Some(Component::Normal(name)) if name == ".git" || name == ".aru"
    )
}

fn compile(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).map_err(|error| {
            AruError::msg(format!("invalid instruction glob {pattern:?}: {error}"))
        })?);
    }
    builder
        .build()
        .map_err(|error| AruError::msg(format!("could not compile instruction globs: {error}")))
}

fn validate_target_scope(
    source: &str,
    scope: &InstructionScope,
    targets: &BTreeSet<crate::manifest::Target>,
) -> Result<()> {
    if let InstructionScope::ApplyTo { globs } = scope {
        for target in targets {
            if capabilities(*target).instructions == Some(InstructionCapability::NativeAgents) {
                return Err(AruError::msg(format!(
                    "instruction source {source:?} uses apply-to globs unsupported by {target}; restrict its targets to claude and/or copilot"
                )));
            }
            if *target == crate::manifest::Target::Copilot
                && globs.iter().any(|glob| glob.contains(','))
            {
                return Err(AruError::msg(format!(
                    "instruction source {source:?} contains a comma in apply-to that Copilot cannot represent exactly"
                )));
            }
        }
    }
    if let InstructionScope::SourceDirectory { directory } = scope
        && targets.contains(&crate::manifest::Target::Copilot)
        && directory.contains(',')
    {
        return Err(AruError::msg(format!(
            "instruction source {source:?} has a directory scope that Copilot cannot represent exactly"
        )));
    }
    Ok(())
}

fn validate_portable_source_path(relative: &str) -> Result<()> {
    for component in relative.split('/') {
        let upper = component.trim_end_matches('.').to_ascii_uppercase();
        let stem = upper.split('.').next().unwrap_or(&upper);
        let reserved = matches!(stem, "CON" | "PRN" | "AUX" | "NUL")
            || stem
                .strip_prefix("COM")
                .or_else(|| stem.strip_prefix("LPT"))
                .is_some_and(|suffix| {
                    matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
                });
        if component.ends_with([' ', '.'])
            || component.contains(['<', '>', ':', '"', '|', '?', '*'])
            || reserved
        {
            return Err(AruError::msg(format!(
                "instruction source path {relative:?} is not portable"
            )));
        }
    }
    Ok(())
}

fn reject_output_source(relative: &str) -> Result<()> {
    let generated = relative == ".github/copilot-instructions.md"
        || relative.starts_with(".github/instructions/aru/")
        || relative.starts_with(".claude/rules/aru/")
        || Path::new(relative)
            .file_name()
            .and_then(|name| name.to_str())
            == Some("CLAUDE.md");
    if generated {
        Err(AruError::msg(format!(
            "instruction source {relative:?} overlaps an aru output path"
        )))
    } else {
        Ok(())
    }
}

fn portable_parent(relative: &str) -> String {
    Path::new(relative)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .and_then(Path::to_str)
        .unwrap_or(".")
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn portable_path(path: &Path) -> Result<String> {
    path.to_str()
        .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| AruError::msg("instruction source path is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{InstructionSource, Instructions, Project, Target};

    fn manifest(source: InstructionSource) -> Manifest {
        Manifest {
            project: Project {
                targets: vec![Target::Codex, Target::Claude, Target::Copilot],
            },
            instructions: Instructions {
                sources: vec![source],
            },
            skills: BTreeMap::new(),
            mcp: BTreeMap::new(),
            packages: BTreeMap::new(),
            package_trust: BTreeMap::new(),
        }
    }

    #[test]
    fn root_and_nested_sources_have_stable_directory_scopes() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("AGENTS.md"), "root\n").unwrap();
        std::fs::create_dir_all(project.path().join("src/api")).unwrap();
        std::fs::write(project.path().join("src/api/AGENTS.md"), "api\n").unwrap();
        let found = discover(
            project.path(),
            &manifest(InstructionSource {
                files: vec!["AGENTS.md".into(), "src/**/AGENTS.md".into()],
                exclude: Vec::new(),
                scope: Some(InstructionSourceScope::SourceDirectory),
                apply_to: Vec::new(),
                targets: Vec::new(),
            }),
        )
        .unwrap();
        assert_eq!(
            found
                .iter()
                .map(|item| (&item.unit.source, &item.unit.scope))
                .collect::<Vec<_>>(),
            [
                (
                    &PathBuf::from("AGENTS.md"),
                    &InstructionScope::SourceDirectory {
                        directory: ".".into()
                    }
                ),
                (
                    &PathBuf::from("src/api/AGENTS.md"),
                    &InstructionScope::SourceDirectory {
                        directory: "src/api".into()
                    }
                )
            ]
        );
    }

    #[test]
    fn explicit_globs_fail_for_native_targets() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("docs")).unwrap();
        std::fs::write(project.path().join("docs/rust.md"), "rust\n").unwrap();
        let error = discover(
            project.path(),
            &manifest(InstructionSource {
                files: vec!["docs/rust.md".into()],
                exclude: Vec::new(),
                scope: None,
                apply_to: vec!["**/*.rs".into()],
                targets: Vec::new(),
            }),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("unsupported by codex"));

        let copilot_only = Manifest {
            project: Project {
                targets: vec![Target::Copilot],
            },
            instructions: Instructions {
                sources: vec![InstructionSource {
                    files: vec!["docs/rust.md".into()],
                    exclude: Vec::new(),
                    scope: None,
                    apply_to: vec!["src/a,b/**".into()],
                    targets: Vec::new(),
                }],
            },
            skills: BTreeMap::new(),
            mcp: BTreeMap::new(),
            packages: BTreeMap::new(),
            package_trust: BTreeMap::new(),
        };
        assert!(
            discover(project.path(), &copilot_only)
                .unwrap_err()
                .to_string()
                .contains("cannot represent exactly")
        );
    }

    #[test]
    fn excludes_duplicates_outputs_markers_and_size_fail_closed() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project.path().join("src/api")).unwrap();
        std::fs::write(project.path().join("AGENTS.md"), "root\n").unwrap();
        std::fs::write(project.path().join("src/api/AGENTS.md"), "api\n").unwrap();
        let selected = discover(
            project.path(),
            &manifest(InstructionSource {
                files: vec!["AGENTS.md".into(), "src/**/AGENTS.md".into()],
                exclude: vec!["src/**".into()],
                scope: Some(InstructionSourceScope::SourceDirectory),
                apply_to: Vec::new(),
                targets: Vec::new(),
            }),
        )
        .unwrap();
        assert_eq!(selected.len(), 1);

        let duplicate = InstructionSource {
            files: vec!["AGENTS.md".into()],
            exclude: Vec::new(),
            scope: Some(InstructionSourceScope::SourceDirectory),
            apply_to: Vec::new(),
            targets: Vec::new(),
        };
        let mut duplicate_manifest = manifest(duplicate.clone());
        duplicate_manifest.instructions.sources.push(duplicate);
        assert!(
            discover(project.path(), &duplicate_manifest)
                .unwrap_err()
                .to_string()
                .contains("more than one declaration")
        );

        std::fs::write(project.path().join("CLAUDE.md"), "manual\n").unwrap();
        let output_error = discover(
            project.path(),
            &manifest(InstructionSource {
                files: vec!["CLAUDE.md".into()],
                exclude: Vec::new(),
                scope: Some(InstructionSourceScope::SourceDirectory),
                apply_to: Vec::new(),
                targets: vec![Target::Claude],
            }),
        )
        .unwrap_err()
        .to_string();
        assert!(output_error.contains("output path"));

        std::fs::write(
            project.path().join("AGENTS.md"),
            "<!-- aru:instruction:start injected -->\n",
        )
        .unwrap();
        assert!(
            discover(
                project.path(),
                &manifest(duplicate_manifest.instructions.sources[0].clone())
            )
            .unwrap_err()
            .to_string()
            .contains("reserved aru marker")
        );
        std::fs::write(
            project.path().join("AGENTS.md"),
            vec![b'x'; MAX_SOURCE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(
            discover(
                project.path(),
                &manifest(duplicate_manifest.instructions.sources[0].clone())
            )
            .unwrap_err()
            .to_string()
            .contains("exceeds")
        );
    }

    #[cfg(unix)]
    #[test]
    fn case_colliding_sources_are_rejected() {
        let project = tempfile::tempdir().unwrap();
        for directory in ["Src", "src"] {
            std::fs::create_dir(project.path().join(directory)).unwrap();
            std::fs::write(project.path().join(directory).join("AGENTS.md"), "rules\n").unwrap();
        }
        let error = discover(
            project.path(),
            &manifest(InstructionSource {
                files: vec!["*/AGENTS.md".into()],
                exclude: Vec::new(),
                scope: Some(InstructionSourceScope::SourceDirectory),
                apply_to: Vec::new(),
                targets: Vec::new(),
            }),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("case-insensitive"));
    }

    #[cfg(unix)]
    #[test]
    fn matching_symlink_is_rejected() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("outside.md"), "outside\n").unwrap();
        std::os::unix::fs::symlink("outside.md", project.path().join("AGENTS.md")).unwrap();
        let error = discover(
            project.path(),
            &manifest(InstructionSource {
                files: vec!["AGENTS.md".into()],
                exclude: Vec::new(),
                scope: Some(InstructionSourceScope::SourceDirectory),
                apply_to: Vec::new(),
                targets: Vec::new(),
            }),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("must not be a symlink"));
    }

    #[test]
    fn empty_manifest_and_targeted_selectors_skip_unrelated_trees() {
        let project = tempfile::tempdir().unwrap();
        let empty = Manifest {
            project: Project {
                targets: vec![Target::Codex],
            },
            instructions: Instructions::default(),
            skills: BTreeMap::new(),
            mcp: BTreeMap::new(),
            packages: BTreeMap::new(),
            package_trust: BTreeMap::new(),
        };
        let mut unrelated = project.path().join("unrelated");
        for index in 0..=MAX_DEPTH {
            unrelated.push(format!("d{index}"));
            std::fs::create_dir_all(&unrelated).unwrap();
        }
        assert!(discover(project.path(), &empty).unwrap().is_empty());

        std::fs::write(project.path().join("AGENTS.md"), "root\n").unwrap();
        let found = discover(
            project.path(),
            &manifest(InstructionSource {
                files: vec!["AGENTS.md".into()],
                exclude: Vec::new(),
                scope: Some(InstructionSourceScope::SourceDirectory),
                apply_to: Vec::new(),
                targets: Vec::new(),
            }),
        )
        .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].unit.source, PathBuf::from("AGENTS.md"));
    }

    #[test]
    fn selector_roots_are_minimal_and_deterministic() {
        let patterns = [
            "src/**/AGENTS.md".to_owned(),
            "AGENTS.md".to_owned(),
            "src/api/*.md".to_owned(),
            "docs/{rust,go}.md".to_owned(),
        ];
        assert_eq!(
            instruction_search_roots(patterns.iter()),
            [
                PathBuf::from("AGENTS.md"),
                PathBuf::from("docs"),
                PathBuf::from("src")
            ]
        );
    }

    #[test]
    fn discovery_depth_limit_fails_instead_of_returning_partial_sources() {
        let project = tempfile::tempdir().unwrap();
        let mut directory = project.path().to_path_buf();
        for index in 0..=MAX_DEPTH {
            directory.push(format!("d{index}"));
            std::fs::create_dir(&directory).unwrap();
        }
        std::fs::write(directory.join("AGENTS.md"), "deep\n").unwrap();
        let error = discover(
            project.path(),
            &manifest(InstructionSource {
                files: vec!["**/AGENTS.md".into()],
                exclude: Vec::new(),
                scope: Some(InstructionSourceScope::SourceDirectory),
                apply_to: Vec::new(),
                targets: Vec::new(),
            }),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("maximum depth"));
    }
}
