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

pub(crate) fn global_directory_for_input(
    target: Target,
    requested: &str,
) -> Result<Option<PathBuf>> {
    let spec = crate::target::spec(target);
    if requested != spec.name && !spec.aliases.contains(&requested) {
        return Err(AruError::msg(format!(
            "target spelling {requested:?} does not identify {target}"
        )));
    }
    let directory = match requested {
        "universal" => Some(config_directory()?.join("agents/skills")),
        "antigravity-cli" => Some(in_home(".gemini/antigravity-cli/skills")?),
        "qoder-cn" => Some(in_home(".qoder-cn/skills")?),
        "trae-cn" => Some(in_home(".trae-cn/skills")?),
        _ => global_directory(target)?,
    };
    Ok(directory)
}

pub(crate) fn global_directory(target: Target) -> Result<Option<PathBuf>> {
    let directory = match target {
        Target::Agents => in_home(".agents/skills")?,
        Target::Codex => overridden_directory("CODEX_HOME", ".codex")?,
        Target::Claude => overridden_directory("CLAUDE_CONFIG_DIR", ".claude")?,
        Target::Copilot => in_home(".copilot/skills")?,
        Target::Opencode => config_directory()?.join("opencode/skills"),
        Target::Pi => in_home(".pi/agent/skills")?,
        Target::Amp | Target::Replit => config_directory()?.join("agents/skills"),
        Target::Antigravity => in_home(".gemini/antigravity/skills")?,
        Target::Cline
        | Target::Dexto
        | Target::Kimi
        | Target::Loaf
        | Target::Warp
        | Target::Zed => in_home(".agents/skills")?,
        Target::Cursor => in_home(".cursor/skills")?,
        Target::Deepagents => in_home(".deepagents/agent/skills")?,
        Target::Firebender => in_home(".firebender/skills")?,
        Target::Gemini => in_home(".gemini/skills")?,
        Target::Adal => in_home(".adal/skills")?,
        Target::AiderDesk => in_home(".aider-desk/skills")?,
        Target::Astrbot => in_home(".astrbot/data/skills")?,
        Target::Autohand => overridden_directory("AUTOHAND_HOME", ".autohand")?,
        Target::Augment => in_home(".augment/skills")?,
        Target::Bob => in_home(".bob/skills")?,
        Target::Openclaw => openclaw_directory(&global_home_directory()?),
        Target::Codearts => in_home(".codeartsdoer/skills")?,
        Target::Codebuddy => in_home(".codebuddy/skills")?,
        Target::Codemaker => in_home(".codemaker/skills")?,
        Target::Codestudio => in_home(".codestudio/skills")?,
        Target::Commandcode => in_home(".commandcode/skills")?,
        Target::Continue => in_home(".continue/skills")?,
        Target::Cortex => in_home(".snowflake/cortex/skills")?,
        Target::Crush => in_home(".config/crush/skills")?,
        Target::Devin => config_directory()?.join("devin/skills"),
        Target::Droid => in_home(".factory/skills")?,
        Target::Eve | Target::Promptscript => return Ok(None),
        Target::Forge => in_home(".forge/skills")?,
        Target::Goose => config_directory()?.join("goose/skills"),
        Target::Grok => overridden_directory("GROK_HOME", ".grok")?,
        Target::Hermes => overridden_directory("HERMES_HOME", ".hermes")?,
        Target::InferenceSh => in_home(".inferencesh/skills")?,
        Target::Jazz => in_home(".jazz/skills")?,
        Target::Junie => in_home(".junie/skills")?,
        Target::Iflow => in_home(".iflow/skills")?,
        Target::Kilo => in_home(".kilocode/skills")?,
        Target::Kimchi => in_home(".config/kimchi/harness/skills")?,
        Target::Kiro => in_home(".kiro/skills")?,
        Target::Kode => in_home(".kode/skills")?,
        Target::Lingma => in_home(".lingma/skills")?,
        Target::Mcpjam => in_home(".mcpjam/skills")?,
        Target::Minimax => in_home(".minimax/skills")?,
        Target::Vibe => overridden_directory("VIBE_HOME", ".vibe")?,
        Target::Moxby => in_home(".moxby/skills")?,
        Target::Mux => in_home(".mux/skills")?,
        Target::Openhands => in_home(".openhands/skills")?,
        Target::Ona => in_home(".ona/skills")?,
        Target::Posit => in_home(".posit/assistant/skills")?,
        Target::Qoder => in_home(".qoder/skills")?,
        Target::Qwen => in_home(".qwen/skills")?,
        Target::Reasonix => in_home(".reasonix/skills")?,
        Target::Rovodev => in_home(".rovodev/skills")?,
        Target::Roo => in_home(".roo/skills")?,
        Target::Tabnine => in_home(".tabnine/agent/skills")?,
        Target::Terramind => in_home(".terramind/skills")?,
        Target::Tinycloud => in_home(".tinycloud/skills")?,
        Target::Trae => in_home(".trae/skills")?,
        Target::Windsurf => in_home(".codeium/windsurf/skills")?,
        Target::Zcode => in_home(".zcode/skills")?,
        Target::Zencoder => in_home(".zencoder/skills")?,
        Target::Neovate => in_home(".neovate/skills")?,
        Target::Pochi => in_home(".pochi/skills")?,
    };
    Ok(Some(directory))
}

fn in_home(path: &str) -> Result<PathBuf> {
    Ok(global_home_directory()?.join(path))
}

fn overridden_directory(variable: &str, fallback: &str) -> Result<PathBuf> {
    let root = match optional_absolute_env(variable)? {
        Some(root) => root,
        None => global_home_directory()?.join(fallback),
    };
    Ok(root.join("skills"))
}

fn config_directory() -> Result<PathBuf> {
    match optional_absolute_env("XDG_CONFIG_HOME")? {
        Some(root) => Ok(root),
        None => Ok(global_home_directory()?.join(".config")),
    }
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
