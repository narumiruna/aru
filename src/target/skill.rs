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
    let link_target = (target != Target::Codex
        && selected_targets.contains(&Target::Codex)
        && supports_project_symlink())
    .then(|| shared_link_target(name));
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
    let root = match target {
        Target::Codex => ".agents/skills",
        Target::Claude => ".claude/skills",
        Target::Copilot => ".github/skills",
        Target::Pi => ".pi/skills",
        Target::Opencode => ".opencode/skills",
    };
    Some(PathBuf::from(format!("{root}/{name}")))
}

pub(crate) fn shared_link_target(name: &str) -> PathBuf {
    PathBuf::from(format!("../../.agents/skills/{name}"))
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
        for (target, expected) in [
            (Target::Codex, ".agents/skills/review"),
            (Target::Claude, ".claude/skills/review"),
            (Target::Copilot, ".github/skills/review"),
            (Target::Pi, ".pi/skills/review"),
            (Target::Opencode, ".opencode/skills/review"),
        ] {
            let native = layout(target, &[target], "review").unwrap();
            assert_eq!(native.destination, PathBuf::from(expected));
            assert_eq!(native.mode, SkillDeploymentMode::Copy);
        }

        #[cfg(unix)]
        for target in [
            Target::Claude,
            Target::Copilot,
            Target::Pi,
            Target::Opencode,
        ] {
            let shared = layout(target, &[Target::Codex, target], "review").unwrap();
            assert_eq!(shared.mode, SkillDeploymentMode::Symlink);
            assert_eq!(
                shared.link_target,
                Some(PathBuf::from("../../.agents/skills/review"))
            );
        }
    }
}
