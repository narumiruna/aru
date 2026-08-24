use std::io::{self, IsTerminal};

use inquire::MultiSelect;
use inquire::error::InquireError;
use inquire::list_option::ListOption;
use inquire::validator::Validation;

use crate::error::{AruError, Result};
use crate::manifest::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillAddSelectionMode {
    All,
    Explicit,
    Interactive,
}

pub fn selection_mode(
    all: bool,
    has_skills: bool,
    has_path: bool,
    stdin_terminal: bool,
    stderr_terminal: bool,
) -> Result<SkillAddSelectionMode> {
    if all {
        return Ok(SkillAddSelectionMode::All);
    }
    if has_skills || has_path {
        return Ok(SkillAddSelectionMode::Explicit);
    }
    if stdin_terminal && stderr_terminal {
        Ok(SkillAddSelectionMode::Interactive)
    } else {
        Err(AruError::msg(
            "interactive skill selection requires a terminal; pass --all, --skill, or --path",
        ))
    }
}

pub fn terminal_selection_mode(
    all: bool,
    has_skills: bool,
    has_path: bool,
) -> Result<SkillAddSelectionMode> {
    selection_mode(
        all,
        has_skills,
        has_path,
        io::stdin().is_terminal(),
        io::stderr().is_terminal(),
    )
}

pub trait SkillChooser {
    fn choose(&mut self, names: &[String], defaults: &[usize]) -> Result<Option<Vec<String>>>;
}

pub trait TargetChooser {
    fn choose(&mut self, targets: &[Target]) -> Result<Option<Vec<Target>>>;
}

pub struct InquireTargetChooser;

#[derive(Clone)]
struct TargetOption {
    target: Target,
    label: String,
}

impl std::fmt::Display for TargetOption {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

impl TargetChooser for InquireTargetChooser {
    fn choose(&mut self, targets: &[Target]) -> Result<Option<Vec<Target>>> {
        let options = targets
            .iter()
            .map(|target| TargetOption {
                target: *target,
                label: format!(
                    "{} ({})",
                    target,
                    crate::target::spec(*target).project_skills
                ),
            })
            .collect::<Vec<_>>();
        MultiSelect::new("Select targets to install to", options)
            .with_page_size(targets.len().clamp(1, 12))
            .with_help_message(
                "↑↓ move, space select, → all, ← none, type filter, enter confirm, esc cancel",
            )
            .with_validator(|selection: &[ListOption<&TargetOption>]| {
                Ok(if selection.is_empty() {
                    Validation::Invalid("select at least one target".into())
                } else {
                    Validation::Valid
                })
            })
            .prompt_skippable()
            .map(|selected| {
                selected.map(|options| options.into_iter().map(|option| option.target).collect())
            })
            .map_err(map_prompt_error)
    }
}

pub fn terminal_choose_targets(chooser: &mut dyn TargetChooser) -> Result<Option<Vec<Target>>> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(AruError::msg(
            "interactive target selection requires a terminal; pass --target",
        ));
    }
    let mut targets = crate::target::specs()
        .iter()
        .filter(|spec| spec.capabilities.skills)
        .map(|spec| spec.target)
        .collect::<Vec<_>>();
    targets.sort_by_key(|target| crate::target::spec(*target).name);
    choose_targets(chooser, &targets)
}

pub fn choose_targets(
    chooser: &mut dyn TargetChooser,
    available: &[Target],
) -> Result<Option<Vec<Target>>> {
    let mut options = available.to_vec();
    options.sort_by_key(|target| crate::target::spec(*target).name);
    options.dedup();
    if options.is_empty() {
        return Err(AruError::msg("no targets support Agent Skills"));
    }
    let Some(mut selected) = chooser.choose(&options)? else {
        return Ok(None);
    };
    selected.sort();
    selected.dedup();
    if selected.is_empty() {
        return Err(AruError::msg("select at least one target"));
    }
    if selected.iter().any(|target| !options.contains(target)) {
        return Err(AruError::msg(
            "interactive target selection returned an unknown target",
        ));
    }
    Ok(Some(selected))
}

pub struct InquireSkillChooser;

impl SkillChooser for InquireSkillChooser {
    fn choose(&mut self, names: &[String], defaults: &[usize]) -> Result<Option<Vec<String>>> {
        MultiSelect::new("Select skills to install", names.to_vec())
            .with_default(defaults)
            .with_page_size(names.len().clamp(1, 12))
            .with_help_message(
                "↑↓ move, space select, → all, ← none, type filter, enter confirm, esc cancel",
            )
            .with_validator(|selection: &[ListOption<&String>]| {
                Ok(if selection.is_empty() {
                    Validation::Invalid("select at least one skill".into())
                } else {
                    Validation::Valid
                })
            })
            .prompt_skippable()
            .map_err(map_prompt_error)
    }
}

fn map_prompt_error(error: InquireError) -> AruError {
    match error {
        InquireError::OperationInterrupted => {
            AruError::msg("interactive skill selection was interrupted")
        }
        other => AruError::msg(format!("interactive skill selection failed: {other}")),
    }
}

pub fn choose_skills(
    chooser: &mut dyn SkillChooser,
    names: &[String],
    current: &[String],
) -> Result<Option<Vec<String>>> {
    if names.is_empty() {
        return Err(AruError::msg("skill source has no selectable skills"));
    }
    let mut options = names.to_vec();
    options.sort();
    options.dedup();
    let defaults = options
        .iter()
        .enumerate()
        .filter_map(|(index, name)| current.contains(name).then_some(index))
        .collect::<Vec<_>>();
    let Some(mut selected) = chooser.choose(&options, &defaults)? else {
        return Ok(None);
    };
    selected.sort();
    selected.dedup();
    if selected.is_empty() {
        return Err(AruError::msg("select at least one skill"));
    }
    if selected.iter().any(|name| !options.contains(name)) {
        return Err(AruError::msg(
            "interactive skill selection returned an unknown skill",
        ));
    }
    Ok(Some(selected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeChooser {
        response: Option<Result<Option<Vec<String>>>>,
        seen_names: Vec<String>,
        seen_defaults: Vec<usize>,
    }

    #[derive(Default)]
    struct FakeTargetChooser {
        response: Option<Result<Option<Vec<Target>>>>,
        seen_targets: Vec<Target>,
    }

    impl TargetChooser for FakeTargetChooser {
        fn choose(&mut self, targets: &[Target]) -> Result<Option<Vec<Target>>> {
            self.seen_targets = targets.to_vec();
            self.response.take().unwrap()
        }
    }

    impl SkillChooser for FakeChooser {
        fn choose(&mut self, names: &[String], defaults: &[usize]) -> Result<Option<Vec<String>>> {
            self.seen_names = names.to_vec();
            self.seen_defaults = defaults.to_vec();
            self.response.take().unwrap()
        }
    }

    #[test]
    fn selection_mode_requires_both_terminals_for_bare_add() {
        assert_eq!(
            selection_mode(false, false, false, true, true).unwrap(),
            SkillAddSelectionMode::Interactive
        );
        assert_eq!(
            selection_mode(true, false, false, false, false).unwrap(),
            SkillAddSelectionMode::All
        );
        assert_eq!(
            selection_mode(false, true, false, false, false).unwrap(),
            SkillAddSelectionMode::Explicit
        );
        let error = selection_mode(false, false, false, true, false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("--all"));
        assert!(error.contains("--skill"));
        assert!(error.contains("--path"));
    }

    #[test]
    fn chooser_receives_stable_names_and_existing_defaults() {
        let names = vec!["zeta".into(), "alpha".into(), "review".into()];
        let mut chooser = FakeChooser {
            response: Some(Ok(Some(vec!["zeta".into(), "alpha".into()]))),
            ..FakeChooser::default()
        };
        let selected = choose_skills(&mut chooser, &names, &["review".into(), "missing".into()])
            .unwrap()
            .unwrap();
        assert_eq!(chooser.seen_names, ["alpha", "review", "zeta"]);
        assert_eq!(chooser.seen_defaults, [1]);
        assert_eq!(selected, ["alpha", "zeta"]);
    }

    #[test]
    fn target_chooser_receives_stable_registry_and_validates_response() {
        let mut chooser = FakeTargetChooser {
            response: Some(Ok(Some(vec![Target::Kiro, Target::Codex]))),
            ..FakeTargetChooser::default()
        };
        let selected = choose_targets(&mut chooser, &[Target::Kiro, Target::Claude, Target::Codex])
            .unwrap()
            .unwrap();
        assert_eq!(
            chooser.seen_targets,
            [Target::Claude, Target::Codex, Target::Kiro]
        );
        assert_eq!(selected, [Target::Codex, Target::Kiro]);

        let mut unknown = FakeTargetChooser {
            response: Some(Ok(Some(vec![Target::Pi]))),
            ..FakeTargetChooser::default()
        };
        assert!(
            choose_targets(&mut unknown, &[Target::Codex])
                .unwrap_err()
                .to_string()
                .contains("unknown target")
        );
    }

    #[test]
    fn chooser_cancel_empty_error_and_terminal_error_are_distinct() {
        let names = vec!["alpha".into()];
        let mut canceled = FakeChooser {
            response: Some(Ok(None)),
            ..FakeChooser::default()
        };
        assert_eq!(choose_skills(&mut canceled, &names, &[]).unwrap(), None);

        let mut empty = FakeChooser {
            response: Some(Ok(Some(Vec::new()))),
            ..FakeChooser::default()
        };
        assert!(
            choose_skills(&mut empty, &names, &[])
                .unwrap_err()
                .to_string()
                .contains("at least one")
        );

        let mut failed = FakeChooser {
            response: Some(Err(AruError::msg("terminal failed"))),
            ..FakeChooser::default()
        };
        assert!(
            choose_skills(&mut failed, &names, &[])
                .unwrap_err()
                .to_string()
                .contains("terminal failed")
        );
    }
}
