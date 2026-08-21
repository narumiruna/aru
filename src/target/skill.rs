use std::path::PathBuf;

use crate::error::{AruError, Result};
use crate::manifest::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillDeploymentMode {
    Copy,
    Symlink,
}

impl SkillDeploymentMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Symlink => "symlink",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillLayout {
    pub(crate) destination: PathBuf,
    pub(crate) mode: SkillDeploymentMode,
    pub(crate) link_target: Option<PathBuf>,
}

pub(crate) fn layout(
    target: Target,
    selected_targets: &[Target],
    name: &str,
) -> Result<SkillLayout> {
    let destination = destination(target, name).ok_or_else(|| {
        AruError::msg(format!(
            "internal error: skill projection reached unsupported target {target}"
        ))
    })?;
    let has_shared_agents_root = selected_targets
        .iter()
        .any(|selected| crate::target::spec(*selected).project_skills == ".agents/skills");
    let link_target = (crate::target::spec(target).project_skills != ".agents/skills"
        && has_shared_agents_root
        && supports_project_symlink())
    .then(|| shared_link_target(&destination));
    Ok(SkillLayout {
        destination,
        mode: if link_target.is_some() {
            SkillDeploymentMode::Symlink
        } else {
            SkillDeploymentMode::Copy
        },
        link_target,
    })
}

pub(crate) fn destination(target: Target, name: &str) -> Option<PathBuf> {
    Some(PathBuf::from(format!(
        "{}/{name}",
        crate::target::spec(target).project_skills
    )))
}

pub(crate) fn shared_link_target(destination: &std::path::Path) -> PathBuf {
    let parent = destination
        .parent()
        .expect("skill destinations always have a parent");
    let mut target = PathBuf::new();
    for _ in parent.components() {
        target.push("..");
    }
    target.push(".agents/skills");
    target.push(
        destination
            .file_name()
            .expect("skill destinations always have a name"),
    );
    target
}

#[cfg(unix)]
fn supports_project_symlink() -> bool {
    true
}

#[cfg(not(unix))]
fn supports_project_symlink() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_layouts_own_their_native_projection_topology() {
        for spec in crate::target::specs() {
            let native = layout(spec.target, &[spec.target], "review").unwrap();
            assert_eq!(
                native.destination,
                PathBuf::from(spec.project_skills).join("review")
            );
            assert_eq!(native.mode, SkillDeploymentMode::Copy);
        }

        #[cfg(unix)]
        for canonical in [Target::Agents, Target::Codex] {
            for target in [
                Target::Claude,
                Target::Copilot,
                Target::Pi,
                Target::Opencode,
                Target::Kiro,
                Target::Droid,
                Target::Posit,
            ] {
                let shared = layout(target, &[canonical, target], "review").unwrap();
                assert_eq!(shared.mode, SkillDeploymentMode::Symlink);
                assert_eq!(
                    shared.link_target,
                    Some(shared_link_target(&shared.destination))
                );
            }
        }
    }

    #[test]
    #[cfg(unix)]
    fn shared_link_targets_account_for_destination_depth() {
        for (target, expected) in [
            (Target::Claude, "../../.agents/skills/review"),
            (Target::Openclaw, "../.agents/skills/review"),
            (Target::Posit, "../../../.agents/skills/review"),
            (Target::Tabnine, "../../../.agents/skills/review"),
        ] {
            let layout = layout(target, &[Target::Agents, target], "review").unwrap();
            assert_eq!(layout.link_target, Some(PathBuf::from(expected)));
        }
    }
}
