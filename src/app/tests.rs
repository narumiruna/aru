use std::process::Command;

use super::skill::remove as skill_remove;
use super::*;
use crate::cli::{SkillAddArgs, SkillRemoveArgs};
use crate::interactive::{SkillAddSelectionMode, SkillChooser};

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

fn repository(path: &Path) {
    std::fs::create_dir(path).unwrap();
    git(path, &["init", "--quiet"]);
    git(path, &["config", "user.email", "test@example.com"]);
    git(path, &["config", "user.name", "Test"]);
    git(path, &["config", "commit.gpgsign", "false"]);
    for name in ["alpha", "beta"] {
        let skill = path.join("skills").join(name);
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test\n---\n# Test\n"),
        )
        .unwrap();
    }
    let custom = path.join("extras/custom");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(
        custom.join("SKILL.md"),
        "---\nname: custom\ndescription: Test\n---\n# Test\n",
    )
    .unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "--quiet", "-m", "initial"]);
    git(path, &["tag", "1.0.0"]);
}

#[derive(Default)]
struct FakeChooser {
    response: Option<Vec<String>>,
    mutate: Option<PathBuf>,
    seen_names: Vec<String>,
    seen_defaults: Vec<String>,
}

impl SkillChooser for FakeChooser {
    fn choose(&mut self, names: &[String], defaults: &[usize]) -> Result<Option<Vec<String>>> {
        self.seen_names = names.to_vec();
        self.seen_defaults = defaults.iter().map(|index| names[*index].clone()).collect();
        if let Some(path) = &self.mutate {
            let mut manifest = std::fs::read_to_string(path).unwrap();
            manifest.push_str("\n# concurrent edit\n");
            std::fs::write(path, manifest).unwrap();
        }
        Ok(self.response.take())
    }
}

fn add_args(source: &Path) -> SkillAddArgs {
    SkillAddArgs {
        source: source.to_string_lossy().into_owned(),
        all: false,
        skills: Vec::new(),
        path: None,
        version: Some("=1.0.0".into()),
        branch: None,
        rev: None,
        targets: Vec::new(),
        upgrade: false,
        global: false,
        no_sync: false,
        dry_run: false,
        force: false,
    }
}

#[test]
fn interactive_cancel_keeps_project_files_unchanged() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let source = temporary.path().join("source");
    std::fs::create_dir(&project).unwrap();
    repository(&source);
    init(project.clone(), vec![Target::Codex]).unwrap();
    let before = std::fs::read(project.join("aru.toml")).unwrap();
    let mut chooser = FakeChooser::default();

    skill_add_with_mode(
        &project,
        add_args(&source),
        SkillAddSelectionMode::Interactive,
        &mut chooser,
    )
    .unwrap();

    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/state.toml").exists());
    assert!(!project.join(".agents").exists());
}

#[test]
fn interactive_selection_replaces_explicit_intent_and_can_narrow_wildcard() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let source = temporary.path().join("source");
    std::fs::create_dir(&project).unwrap();
    repository(&source);
    init(project.clone(), vec![Target::Codex]).unwrap();

    let mut first = FakeChooser {
        response: Some(vec!["alpha".into()]),
        ..FakeChooser::default()
    };
    skill_add_with_mode(
        &project,
        add_args(&source),
        SkillAddSelectionMode::Interactive,
        &mut first,
    )
    .unwrap();
    assert!(project.join(".agents/skills/alpha").is_dir());
    assert!(!project.join(".agents/skills/beta").exists());

    let mut replacement = FakeChooser {
        response: Some(vec!["beta".into()]),
        ..FakeChooser::default()
    };
    skill_add_with_mode(
        &project,
        add_args(&source),
        SkillAddSelectionMode::Interactive,
        &mut replacement,
    )
    .unwrap();
    assert_eq!(replacement.seen_defaults, ["alpha"]);
    assert!(!project.join(".agents/skills/alpha").exists());
    assert!(project.join(".agents/skills/beta").is_dir());

    let mut all_args = add_args(&source);
    all_args.all = true;
    skill_add_with_mode(
        &project,
        all_args,
        SkillAddSelectionMode::All,
        &mut FakeChooser::default(),
    )
    .unwrap();
    let mut preserve = FakeChooser {
        response: Some(vec!["alpha".into(), "beta".into(), "custom".into()]),
        ..FakeChooser::default()
    };
    skill_add_with_mode(
        &project,
        add_args(&source),
        SkillAddSelectionMode::Interactive,
        &mut preserve,
    )
    .unwrap();
    assert_eq!(preserve.seen_defaults, ["alpha", "beta", "custom"]);
    let manifest = ManifestDocument::load(&project)
        .unwrap()
        .manifest()
        .unwrap();
    assert!(manifest.skills.values().next().unwrap().is_wildcard());

    skill_remove(
        &project,
        SkillRemoveArgs {
            source: source.to_string_lossy().into_owned(),
            skills: vec!["alpha".into()],
            no_sync: false,
            dry_run: false,
        },
        ExecutionPolicy::default(),
    )
    .unwrap();
    let mut narrowed = FakeChooser {
        response: Some(vec!["beta".into()]),
        ..FakeChooser::default()
    };
    skill_add_with_mode(
        &project,
        add_args(&source),
        SkillAddSelectionMode::Interactive,
        &mut narrowed,
    )
    .unwrap();
    assert_eq!(narrowed.seen_defaults, ["beta", "custom"]);
    let manifest = ManifestDocument::load(&project)
        .unwrap()
        .manifest()
        .unwrap();
    let requirement = manifest.skills.values().next().unwrap();
    assert_eq!(requirement.include, ["beta"]);
    assert!(requirement.exclude.is_empty());
    assert!(!requirement.is_wildcard());
}

#[test]
fn interactive_selection_preserves_or_removes_custom_path_with_its_skill() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let source = temporary.path().join("source");
    std::fs::create_dir(&project).unwrap();
    repository(&source);
    init(project.clone(), vec![Target::Codex]).unwrap();
    let mut path_args = add_args(&source);
    path_args.path = Some("extras/custom".into());
    path_args.version = None;
    skill_add_with_mode(
        &project,
        path_args,
        SkillAddSelectionMode::Explicit,
        &mut FakeChooser::default(),
    )
    .unwrap();

    let mut keep = FakeChooser {
        response: Some(vec!["custom".into()]),
        ..FakeChooser::default()
    };
    skill_add_with_mode(
        &project,
        add_args(&source),
        SkillAddSelectionMode::Interactive,
        &mut keep,
    )
    .unwrap();
    assert_eq!(keep.seen_defaults, ["custom"]);
    let manifest = ManifestDocument::load(&project)
        .unwrap()
        .manifest()
        .unwrap();
    assert_eq!(
        manifest.skills.values().next().unwrap().paths["custom"],
        "extras/custom"
    );

    let mut remove = FakeChooser {
        response: Some(vec!["alpha".into()]),
        ..FakeChooser::default()
    };
    skill_add_with_mode(
        &project,
        add_args(&source),
        SkillAddSelectionMode::Interactive,
        &mut remove,
    )
    .unwrap();
    let manifest = ManifestDocument::load(&project)
        .unwrap()
        .manifest()
        .unwrap();
    let requirement = manifest.skills.values().next().unwrap();
    assert!(requirement.paths.is_empty());
    assert_eq!(requirement.include, ["alpha"]);
}

#[test]
fn interactive_upgrade_previews_and_installs_the_new_release() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let source = temporary.path().join("source");
    std::fs::create_dir(&project).unwrap();
    repository(&source);
    init(project.clone(), vec![Target::Codex]).unwrap();

    let mut initial_args = add_args(&source);
    initial_args.version = None;
    let mut initial = FakeChooser {
        response: Some(vec!["alpha".into()]),
        ..FakeChooser::default()
    };
    skill_add_with_mode(
        &project,
        initial_args,
        SkillAddSelectionMode::Interactive,
        &mut initial,
    )
    .unwrap();

    let gamma = source.join("skills/gamma");
    std::fs::create_dir_all(&gamma).unwrap();
    std::fs::write(
        gamma.join("SKILL.md"),
        "---\nname: gamma\ndescription: Test\n---\n# Test\n",
    )
    .unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "--quiet", "-m", "1.1.0"]);
    git(&source, &["tag", "1.1.0"]);

    let mut upgrade_args = add_args(&source);
    upgrade_args.version = None;
    upgrade_args.upgrade = true;
    let mut upgrade = FakeChooser {
        response: Some(vec!["gamma".into()]),
        ..FakeChooser::default()
    };
    skill_add_with_mode(
        &project,
        upgrade_args,
        SkillAddSelectionMode::Interactive,
        &mut upgrade,
    )
    .unwrap();

    assert!(upgrade.seen_names.contains(&"gamma".into()));
    assert!(project.join(".agents/skills/gamma").is_dir());
    let lock = Lockfile::load_optional(&project).unwrap().unwrap();
    assert_eq!(lock.skill_packages[0].version, "1.1.0");
}

#[test]
fn interactive_commit_rejects_a_concurrent_manifest_change() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let source = temporary.path().join("source");
    std::fs::create_dir(&project).unwrap();
    repository(&source);
    init(project.clone(), vec![Target::Codex]).unwrap();
    let manifest_path = project.join("aru.toml");
    let mut chooser = FakeChooser {
        response: Some(vec!["alpha".into()]),
        mutate: Some(manifest_path.clone()),
        ..FakeChooser::default()
    };

    let error = skill_add_with_mode(
        &project,
        add_args(&source),
        SkillAddSelectionMode::Interactive,
        &mut chooser,
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("changed during interactive skill selection"));
    assert!(
        std::fs::read_to_string(manifest_path)
            .unwrap()
            .contains("concurrent edit")
    );
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".agents").exists());
}
