use std::path::PathBuf;

use clap::{ArgAction, ArgGroup, Args, Parser, Subcommand, ValueEnum};

use crate::manifest::Target;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Parser)]
#[command(
    name = "aru",
    version,
    about = "Project manager for coding-agent instructions, skills, and MCP servers"
)]
pub struct Cli {
    /// Project directory (defaults to the nearest ancestor containing aru.toml).
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help_heading = "Global Options"
    )]
    pub project: Option<PathBuf>,
    /// Suppress normal status output.
    #[arg(
        short,
        long,
        global = true,
        conflicts_with = "verbose",
        help_heading = "Global Options"
    )]
    pub quiet: bool,
    /// Use verbose output; repeat for additional detail.
    #[arg(short, long, global = true, action = ArgAction::Count, help_heading = "Global Options")]
    pub verbose: u8,
    /// Control color in human-readable output.
    #[arg(
        long,
        global = true,
        value_enum,
        default_value_t,
        help_heading = "Global Options"
    )]
    pub color: ColorChoice,
    /// Disable remote Git and Registry access.
    #[arg(long, global = true, help_heading = "Global Options")]
    pub offline: bool,
    /// Hide progress output.
    #[arg(long, global = true, help_heading = "Global Options")]
    pub no_progress: bool,
    /// Assert that aru.lock will remain unchanged.
    #[arg(long, global = true, help_heading = "Global Options")]
    pub locked: bool,
    /// Use the existing lock without network access (equivalent to --locked --offline).
    #[arg(long, global = true, help_heading = "Global Options")]
    pub frozen: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize an aru project.
    Init(InitArgs),
    /// Update aru.lock without projecting files.
    Lock(LockArgs),
    /// Reconcile the lock and configured target project paths.
    Sync(SyncArgs),
    /// Manage project instructions.
    Instruction {
        #[command(subcommand)]
        command: InstructionCommand,
    },
    /// Manage project targets.
    Target {
        #[command(subcommand)]
        command: TargetCommand,
    },
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
    /// Directory to initialize (defaults to the current directory).
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,
    /// Target to configure; may be repeated.
    #[arg(long, value_name = "TARGET", required = true, action = ArgAction::Append)]
    pub target: Vec<Target>,
}

#[derive(Debug, Args)]
pub struct LockArgs {
    /// Check whether aru.lock is up to date without writing.
    #[arg(long, conflicts_with = "dry_run")]
    pub check: bool,
    /// Resolve and print the plan without writing any file or cache.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Check whether the lock and target paths are synchronized without writing.
    #[arg(long, conflicts_with_all = ["dry_run", "merge", "force"])]
    pub check: bool,
    /// Resolve and print the plan without writing any file or cache.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
    /// Preserve unmanaged Markdown while adding instruction blocks.
    #[arg(long, conflicts_with = "force")]
    pub merge: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Subcommand)]
pub enum InstructionCommand {
    /// Discover and add existing AGENTS.md files.
    Add(InstructionAddArgs),
    /// Remove declared instruction source files.
    Remove(InstructionRemoveArgs),
    /// List declared instruction source files.
    List,
}

#[derive(Debug, Args)]
pub struct InstructionAddArgs {
    /// Discover conventional root and nested AGENTS.md files.
    #[arg(long, required = true)]
    pub discover: bool,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Resolve and print the plan without writing any file.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Preserve unmanaged Markdown while adding instruction blocks.
    #[arg(long, conflicts_with = "force", help_heading = "Apply Options")]
    pub merge: bool,
    /// Destructively replace colliding unmanaged instruction outputs.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct InstructionRemoveArgs {
    /// Exact declared file selector to remove; may be repeated.
    #[arg(value_name = "FILE", required = true, num_args = 1..)]
    pub files: Vec<String>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Resolve and print the plan without writing any file.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
}

#[derive(Debug, Subcommand)]
pub enum TargetCommand {
    /// Add one or more targets to the configured set.
    Add(TargetAddArgs),
    /// Remove one or more targets from the configured set.
    Remove(TargetRemoveArgs),
    /// Replace the configured target set exactly.
    Set(TargetSetArgs),
    /// List configured targets.
    List,
}

#[derive(Debug, Args)]
pub struct TargetAddArgs {
    /// Target to add; may be repeated.
    #[arg(value_name = "TARGET", required = true, num_args = 1..)]
    pub targets: Vec<Target>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Preserve unmanaged Markdown while adding instruction blocks.
    #[arg(long, conflicts_with = "force", help_heading = "Apply Options")]
    pub merge: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct TargetRemoveArgs {
    /// Configured target to remove; may be repeated.
    #[arg(value_name = "TARGET", required = true, num_args = 1..)]
    pub targets: Vec<Target>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct TargetSetArgs {
    /// Exact target set.
    #[arg(value_name = "TARGET", required = true, num_args = 1..)]
    pub targets: Vec<Target>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Preserve unmanaged Markdown while adding instruction blocks.
    #[arg(long, conflicts_with = "force", help_heading = "Apply Options")]
    pub merge: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Add all or selected skills from a Git source.
    Add(SkillAddArgs),
    /// Remove a skill source or selected exports.
    Remove(SkillRemoveArgs),
    /// Update all or selected declared skill sources.
    Update(SkillUpdateArgs),
    /// List locked skills and declared sources.
    List,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("selector").args(["all", "skills", "path"]).multiple(false)))]
#[command(group(ArgGroup::new("reference").args(["version", "branch", "rev"]).multiple(false)))]
pub struct SkillAddArgs {
    /// GitHub owner/repo, Git URL, SSH source, or local Git repository.
    pub source: String,
    /// Select every current and future skill exported by this source.
    #[arg(short = 'a', long, help_heading = "Selection Options")]
    pub all: bool,
    /// Select a stable skill name; may be repeated.
    #[arg(long = "skill", value_name = "NAME", action = ArgAction::Append, help_heading = "Selection Options")]
    pub skills: Vec<String>,
    /// Select one non-standard repository-relative skill directory.
    #[arg(long, value_name = "PATH", help_heading = "Selection Options")]
    pub path: Option<String>,
    /// Cargo-style SemVer tag requirement (a bare version uses caret semantics).
    #[arg(long, help_heading = "Source Options")]
    pub version: Option<String>,
    /// Moving Git branch to resolve and pin to an exact commit.
    #[arg(long, value_name = "NAME", help_heading = "Source Options")]
    pub branch: Option<String>,
    /// Exact Git commit (7-40 hexadecimal characters).
    #[arg(long, help_heading = "Source Options")]
    pub rev: Option<String>,
    /// Resolve the latest compatible tag or branch head instead of reusing the lock.
    #[arg(short = 'U', long, help_heading = "Source Options")]
    pub upgrade: bool,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SkillRemoveArgs {
    pub source: String,
    /// Remove only this export; may be repeated.
    #[arg(long = "skill", value_name = "NAME", action = ArgAction::Append)]
    pub skills: Vec<String>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SkillUpdateArgs {
    /// Declared source to update; omit to update all.
    #[arg(value_name = "SOURCE")]
    pub sources: Vec<String>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Add an MCP Registry server, direct remote, or direct stdio command.
    Add(Box<McpAddArgs>),
    /// Remove a named MCP server.
    Remove(McpRemoveArgs),
    /// Update all or selected MCP Registry servers.
    Update(McpUpdateArgs),
    /// List declared MCP servers.
    List,
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("source")
        .required(true)
        .args(["server", "url", "command"])
))]
pub struct McpAddArgs {
    /// MCP Registry server id.
    #[arg(help_heading = "Source Options")]
    pub server: Option<String>,
    /// Stable project-local server name.
    #[arg(long, help_heading = "Source Options")]
    pub name: String,
    /// Direct HTTPS streamable MCP endpoint.
    #[arg(long, help_heading = "Source Options")]
    pub url: Option<String>,
    /// Direct stdio executable; aru records argv but never executes it.
    #[arg(long, value_name = "COMMAND", help_heading = "Source Options")]
    pub command: Option<String>,
    /// Ordered argument for --command; may be repeated.
    #[arg(long = "arg", value_name = "ARG", action = ArgAction::Append, requires = "command", help_heading = "Source Options")]
    pub args: Vec<String>,
    /// Registry base URL.
    #[arg(long, help_heading = "Registry Options")]
    pub registry: Option<String>,
    /// Server SemVer requirement, or =literal for an exact non-SemVer version.
    #[arg(long, help_heading = "Registry Options")]
    pub version: Option<String>,
    /// Candidate transport selector (stdio or streamable-http).
    #[arg(long, help_heading = "Registry Options")]
    pub transport: Option<String>,
    /// Candidate package registry selector (MVP supports npm).
    #[arg(long = "package-registry", help_heading = "Registry Options")]
    pub package_registry: Option<String>,
    /// Environment variable containing a bearer token.
    #[arg(long = "bearer-token-env", help_heading = "Authentication Options")]
    pub bearer_token_env: Option<String>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct McpRemoveArgs {
    pub name: String,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct McpUpdateArgs {
    /// MCP names to update; omit to update every Registry server.
    #[arg(value_name = "NAME")]
    pub names: Vec<String>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Print a deterministic plan without writing.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}
