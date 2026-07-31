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
    match target {
        Target::Codex => Ok(SkillLayout {
            destination: PathBuf::from(format!(".agents/skills/{name}")),
            mode: SkillDeploymentMode::Copy,
            link_target: None,
        }),
        Target::Claude => {
            let link_target = (selected_targets.contains(&Target::Codex)
                && supports_project_symlink())
            .then(|| claude_link_target(name));
            Ok(SkillLayout {
                destination: PathBuf::from(format!(".claude/skills/{name}")),
                mode: if link_target.is_some() {
                    SkillDeploymentMode::Symlink
                } else {
                    SkillDeploymentMode::Copy
                },
                link_target,
            })
        }
        Target::Copilot | Target::Pi | Target::Opencode => Err(AruError::msg(format!(
            "internal error: skill projection reached unsupported target {target}"
        ))),
    }
}

pub(crate) fn destination(target: Target, name: &str) -> Option<PathBuf> {
    match target {
        Target::Codex => Some(PathBuf::from(format!(".agents/skills/{name}"))),
        Target::Claude => Some(PathBuf::from(format!(".claude/skills/{name}"))),
        Target::Copilot | Target::Pi | Target::Opencode => None,
    }
}

pub(crate) fn claude_link_target(name: &str) -> PathBuf {
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
    fn codex_and_claude_layouts_own_their_projection_topology() {
        let codex = layout(Target::Codex, &[Target::Codex, Target::Claude], "review").unwrap();
        assert_eq!(codex.destination, PathBuf::from(".agents/skills/review"));
        assert_eq!(codex.mode, SkillDeploymentMode::Copy);

        let claude = layout(Target::Claude, &[Target::Claude], "review").unwrap();
        assert_eq!(claude.destination, PathBuf::from(".claude/skills/review"));
        assert_eq!(claude.mode, SkillDeploymentMode::Copy);

        #[cfg(unix)]
        {
            let shared =
                layout(Target::Claude, &[Target::Codex, Target::Claude], "review").unwrap();
            assert_eq!(shared.mode, SkillDeploymentMode::Symlink);
            assert_eq!(
                shared.link_target,
                Some(PathBuf::from("../../.agents/skills/review"))
            );
        }
    }
}
