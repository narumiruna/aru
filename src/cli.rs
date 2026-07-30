use std::path::PathBuf;

use clap::{ArgAction, ArgGroup, Args, Parser, Subcommand};

use crate::manifest::Agent;

#[derive(Debug, Parser)]
#[command(name = "aru", version, about = "Agent package and project manager")]
pub struct Cli {
    /// Project directory (defaults to the nearest ancestor containing aru.toml).
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize an aru project.
    Init(InitArgs),
    /// Resolve aru.toml and update aru.lock without projecting files.
    Lock(LockArgs),
    /// Reconcile the lock and all configured agent project paths.
    Sync(SyncArgs),
    /// Manage Agent Skills packages.
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Manage MCP servers.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Agent to configure; may be repeated.
    #[arg(long, value_name = "AGENT", required = true, action = ArgAction::Append)]
    pub agent: Vec<Agent>,
}

#[derive(Debug, Args)]
pub struct LockArgs {
    /// Resolve and print the plan without writing any file or cache.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Require aru.lock to be present and exactly current.
    #[arg(long)]
    pub locked: bool,
    /// Resolve and print the plan without writing any file or cache.
    #[arg(long)]
    pub dry_run: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Add all or selected skills from a Git source.
    Add(SkillAddArgs),
    /// Remove a skill source or selected exports.
    Remove(SkillRemoveArgs),
    /// Upgrade all or selected declared skill sources.
    Update(SkillUpdateArgs),
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("selector").args(["all", "skills", "path"]).multiple(false)))]
#[command(group(ArgGroup::new("reference").args(["version", "branch", "rev"]).multiple(false)))]
pub struct SkillAddArgs {
    /// GitHub owner/repo, Git URL, SSH source, or local Git repository.
    pub source: String,
    /// Select every current and future skill exported by this source.
    #[arg(short = 'a', long)]
    pub all: bool,
    /// Select a stable skill name; may be repeated.
    #[arg(long = "skill", value_name = "NAME", action = ArgAction::Append)]
    pub skills: Vec<String>,
    /// Select one non-standard repository-relative skill directory.
    #[arg(long, value_name = "PATH")]
    pub path: Option<String>,
    /// Cargo-style SemVer tag requirement (a bare version uses caret semantics).
    #[arg(long)]
    pub version: Option<String>,
    /// Moving Git branch to resolve and pin to an exact commit.
    #[arg(long, value_name = "NAME")]
    pub branch: Option<String>,
    /// Exact Git commit (7-40 hexadecimal characters).
    #[arg(long)]
    pub rev: Option<String>,
    /// Update manifest and lock but skip agent project paths.
    #[arg(long)]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(long)]
    pub dry_run: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SkillRemoveArgs {
    pub source: String,
    /// Remove only this export; may be repeated.
    #[arg(long = "skill", value_name = "NAME", action = ArgAction::Append)]
    pub skills: Vec<String>,
    #[arg(long)]
    pub no_sync: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SkillUpdateArgs {
    /// Declared source to unlock; omit to update all.
    #[arg(value_name = "SOURCE")]
    pub sources: Vec<String>,
    #[arg(long)]
    pub no_sync: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Add an MCP Registry server or direct remote.
    Add(McpAddArgs),
    /// Remove a named MCP server.
    Remove(McpRemoveArgs),
    /// Upgrade all or selected MCP Registry servers.
    Update(McpUpdateArgs),
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("source").required(true).args(["server", "url"])))]
pub struct McpAddArgs {
    /// MCP Registry server id.
    pub server: Option<String>,
    /// Stable project-local server name.
    #[arg(long)]
    pub name: String,
    /// Registry base URL.
    #[arg(long)]
    pub registry: Option<String>,
    /// Server SemVer requirement, or =literal for an exact non-SemVer version.
    #[arg(long)]
    pub version: Option<String>,
    /// Candidate transport selector (stdio or streamable-http).
    #[arg(long)]
    pub transport: Option<String>,
    /// Candidate package registry selector (MVP supports npm).
    #[arg(long = "package-registry")]
    pub package_registry: Option<String>,
    /// Direct HTTPS streamable MCP endpoint.
    #[arg(long, conflicts_with = "server")]
    pub url: Option<String>,
    /// Environment variable containing a bearer token (Codex-only capability).
    #[arg(long = "bearer-token-env")]
    pub bearer_token_env: Option<String>,
    #[arg(long)]
    pub no_sync: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct McpRemoveArgs {
    pub name: String,
    #[arg(long)]
    pub no_sync: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct McpUpdateArgs {
    /// MCP names to unlock; omit to update every registry server.
    #[arg(value_name = "NAME")]
    pub names: Vec<String>,
    #[arg(long)]
    pub no_sync: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
}
