use std::path::{Path, PathBuf};

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

pub(crate) fn global_directory(target: Target) -> Result<Option<PathBuf>> {
    let home = global_home_directory()?;
    let config = optional_absolute_env("XDG_CONFIG_HOME")?.unwrap_or_else(|| home.join(".config"));
    let overridden = |variable: &str, fallback: &str| -> Result<PathBuf> {
        Ok(optional_absolute_env(variable)?.unwrap_or_else(|| home.join(fallback)))
    };
    let directory = match target {
        Target::Agents => home.join(".agents/skills"),
        Target::Codex => overridden("CODEX_HOME", ".codex")?.join("skills"),
        Target::Claude => overridden("CLAUDE_CONFIG_DIR", ".claude")?.join("skills"),
        Target::Copilot => home.join(".copilot/skills"),
        Target::Opencode => config.join("opencode/skills"),
        Target::Pi => home.join(".pi/agent/skills"),
        Target::Amp | Target::Replit => config.join("agents/skills"),
        Target::Antigravity => home.join(".gemini/antigravity/skills"),
        Target::Cline
        | Target::Dexto
        | Target::Kimi
        | Target::Loaf
        | Target::Warp
        | Target::Zed => home.join(".agents/skills"),
        Target::Cursor => home.join(".cursor/skills"),
        Target::Deepagents => home.join(".deepagents/agent/skills"),
        Target::Firebender => home.join(".firebender/skills"),
        Target::Gemini => home.join(".gemini/skills"),
        Target::Adal => home.join(".adal/skills"),
        Target::AiderDesk => home.join(".aider-desk/skills"),
        Target::Astrbot => home.join(".astrbot/data/skills"),
        Target::Autohand => overridden("AUTOHAND_HOME", ".autohand")?.join("skills"),
        Target::Augment => home.join(".augment/skills"),
        Target::Bob => home.join(".bob/skills"),
        Target::Openclaw => openclaw_directory(&home),
        Target::Codearts => home.join(".codeartsdoer/skills"),
        Target::Codebuddy => home.join(".codebuddy/skills"),
        Target::Codemaker => home.join(".codemaker/skills"),
        Target::Codestudio => home.join(".codestudio/skills"),
        Target::Commandcode => home.join(".commandcode/skills"),
        Target::Continue => home.join(".continue/skills"),
        Target::Cortex => home.join(".snowflake/cortex/skills"),
        Target::Crush => home.join(".config/crush/skills"),
        Target::Devin => config.join("devin/skills"),
        Target::Droid => home.join(".factory/skills"),
        Target::Eve | Target::Promptscript => return Ok(None),
        Target::Forge => home.join(".forge/skills"),
        Target::Goose => config.join("goose/skills"),
        Target::Grok => overridden("GROK_HOME", ".grok")?.join("skills"),
        Target::Hermes => overridden("HERMES_HOME", ".hermes")?.join("skills"),
        Target::InferenceSh => home.join(".inferencesh/skills"),
        Target::Jazz => home.join(".jazz/skills"),
        Target::Junie => home.join(".junie/skills"),
        Target::Iflow => home.join(".iflow/skills"),
        Target::Kilo => home.join(".kilocode/skills"),
        Target::Kimchi => home.join(".config/kimchi/harness/skills"),
        Target::Kiro => home.join(".kiro/skills"),
        Target::Kode => home.join(".kode/skills"),
        Target::Lingma => home.join(".lingma/skills"),
        Target::Mcpjam => home.join(".mcpjam/skills"),
        Target::Minimax => home.join(".minimax/skills"),
        Target::Vibe => overridden("VIBE_HOME", ".vibe")?.join("skills"),
        Target::Moxby => home.join(".moxby/skills"),
        Target::Mux => home.join(".mux/skills"),
        Target::Openhands => home.join(".openhands/skills"),
        Target::Ona => home.join(".ona/skills"),
        Target::Posit => home.join(".posit/assistant/skills"),
        Target::Qoder => home.join(".qoder/skills"),
        Target::Qwen => home.join(".qwen/skills"),
        Target::Reasonix => home.join(".reasonix/skills"),
        Target::Rovodev => home.join(".rovodev/skills"),
        Target::Roo => home.join(".roo/skills"),
        Target::Tabnine => home.join(".tabnine/agent/skills"),
        Target::Terramind => home.join(".terramind/skills"),
        Target::Tinycloud => home.join(".tinycloud/skills"),
        Target::Trae => home.join(".trae/skills"),
        Target::Windsurf => home.join(".codeium/windsurf/skills"),
        Target::Zcode => home.join(".zcode/skills"),
        Target::Zencoder => home.join(".zencoder/skills"),
        Target::Neovate => home.join(".neovate/skills"),
        Target::Pochi => home.join(".pochi/skills"),
    };
    Ok(Some(directory))
}

pub(crate) fn global_home_directory() -> Result<PathBuf> {
    for variable in ["HOME", "USERPROFILE"] {
        if let Some(path) = optional_absolute_env(variable)? {
            if !path.is_dir() {
                return Err(AruError::msg(format!(
                    "{variable} must identify an existing directory for global skill installation"
                )));
            }
            return Ok(path);
        }
    }
    Err(AruError::msg(
        "could not determine the user home directory; set HOME or USERPROFILE",
    ))
}

fn optional_absolute_env(variable: &str) -> Result<Option<PathBuf>> {
    let Some(value) = std::env::var_os(variable) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(AruError::msg(format!(
            "{variable} must be an absolute path for global skill installation"
        )));
    }
    Ok(Some(path))
}

fn openclaw_directory(home: &Path) -> PathBuf {
    for directory in [".openclaw", ".clawdbot", ".moltbot"] {
        let root = home.join(directory);
        if root.is_dir() {
            return root.join("skills");
        }
    }
    home.join(".openclaw/skills")
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
