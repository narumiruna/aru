use std::path::PathBuf;

use clap::{ArgAction, ArgGroup, Args, Parser, Subcommand, ValueEnum};

use crate::manifest::{PluginComponent, PluginFormat, Target};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum AuditFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ExportFormat {
    #[default]
    #[value(name = "cyclonedx1.5")]
    CycloneDx15,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum TreeFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, Parser)]
#[command(
    name = "aru",
    version,
    about = "Project manager for coding-agent instructions, skills, and MCP servers"
)]
pub struct Cli {
    /// Project or package directory (defaults to the nearest relevant manifest).
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
    /// Add a native aru package dependency.
    Add(PackageAddArgs),
    /// Remove a native aru package dependency.
    Remove(PackageRemoveArgs),
    /// Update all or selected native aru packages.
    Update(PackageUpdateArgs),
    /// Update aru.lock without projecting files.
    Lock(LockArgs),
    /// Reconcile the lock and configured target project paths.
    Sync(SyncArgs),
    /// Inspect project integrity without changing project state.
    Audit(AuditArgs),
    /// Export a deterministic inventory from aru.lock.
    Export(ExportArgs),
    /// Display the locked native-package dependency graph.
    Tree(TreeArgs),
    /// Inspect one locked or available native package.
    Info(InfoArgs),
    /// Print versioned machine-readable project metadata.
    Metadata(MetadataArgs),
    /// Generate a shell completion script.
    GenerateShellCompletion(GenerateShellCompletionArgs),
    /// Build a verified deterministic native-package archive.
    Package(PackageArchiveArgs),
    /// Manage the aru executable.
    #[command(name = "self")]
    SelfManagement {
        #[command(subcommand)]
        command: SelfCommand,
    },
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
    /// Manage plugin dependencies.
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    /// Inspect a locked plugin name or an available plugin source.
    Info(PluginInfoArgs),
    /// Add a plugin dependency and project its selected capabilities.
    Add(PluginAddArgs),
    /// List configured plugins.
    List,
    /// Update all or selected plugin dependencies.
    Update(PluginUpdateArgs),
    /// Remove a complete plugin declaration.
    Remove(PluginRemoveArgs),
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("reference").args(["version", "branch", "rev"]).multiple(false)))]
pub struct PluginInfoArgs {
    /// Locked plugin name, Git source, local Git repository, or plain local directory.
    #[arg(value_name = "SOURCE")]
    pub source: String,
    /// Select the source format instead of auto-detecting it.
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<PluginFormat>,
    /// Repository-relative plugin root.
    #[arg(long, value_name = "PATH")]
    pub subdir: Option<String>,
    /// Cargo-style SemVer tag requirement.
    #[arg(long, help_heading = "Source Options")]
    pub version: Option<String>,
    /// Moving Git branch to inspect.
    #[arg(long, value_name = "NAME", help_heading = "Source Options")]
    pub branch: Option<String>,
    /// Exact Git commit.
    #[arg(long, help_heading = "Source Options")]
    pub rev: Option<String>,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("reference").args(["version", "branch", "rev"]).multiple(false)))]
pub struct PluginAddArgs {
    /// GitHub owner/repo, Git URL, SSH source, or local Git repository.
    #[arg(value_name = "SOURCE")]
    pub source: String,
    /// Select the source format instead of auto-detecting it.
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<PluginFormat>,
    /// Repository-relative plugin root.
    #[arg(long, value_name = "PATH")]
    pub subdir: Option<String>,
    /// Cargo-style SemVer tag requirement.
    #[arg(long, help_heading = "Source Options")]
    pub version: Option<String>,
    /// Moving Git branch to resolve and pin.
    #[arg(long, value_name = "NAME", help_heading = "Source Options")]
    pub branch: Option<String>,
    /// Exact Git commit.
    #[arg(long, help_heading = "Source Options")]
    pub rev: Option<String>,
    /// Select every export of this component; may be repeated.
    #[arg(long = "component", value_name = "COMPONENT", action = ArgAction::Append, help_heading = "Selection Options")]
    pub components: Vec<PluginComponent>,
    /// Select one skill by name; may be repeated.
    #[arg(long = "skill", value_name = "NAME", action = ArgAction::Append, help_heading = "Selection Options")]
    pub skills: Vec<String>,
    /// Select one MCP server by name; may be repeated.
    #[arg(long = "mcp", value_name = "NAME", action = ArgAction::Append, help_heading = "Selection Options")]
    pub mcp: Vec<String>,
    /// Configured target that should receive selected resources; may be repeated.
    #[arg(long = "target", value_name = "TARGET", action = ArgAction::Append, help_heading = "Selection Options")]
    pub targets: Vec<Target>,
    /// Explicitly trust a selected plugin MCP name; may be repeated.
    #[arg(long = "trust-mcp", value_name = "NAME", action = ArgAction::Append, help_heading = "Trust Options")]
    pub trust_mcp: Vec<String>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Resolve and print the plan without writing any file or persistent cache.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct PluginUpdateArgs {
    /// Configured plugin name; omit to update all.
    #[arg(value_name = "NAME")]
    pub names: Vec<String>,
    /// Select one exact SemVer version compatible with the declaration.
    #[arg(long, value_name = "VERSION")]
    pub precise: Option<String>,
    /// Update lock intent but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Resolve and print the plan without writing any file or persistent cache.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct PluginRemoveArgs {
    /// Configured plugin name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Resolve and print the plan without writing any file or persistent cache.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct TreeArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t)]
    pub format: TreeFormat,
    /// Limit package graph depth (the project root is depth 0).
    #[arg(long, value_name = "N")]
    pub depth: Option<usize>,
    /// Show only packages effective for this target.
    #[arg(long, value_name = "TARGET")]
    pub target: Option<Target>,
    /// Display reverse dependencies for a package selector.
    #[arg(short = 'i', long, value_name = "PACKAGE")]
    pub invert: Option<String>,
}

#[derive(Debug, Args)]
pub struct InfoArgs {
    /// Locked package name/source or a Git source to inspect.
    #[arg(value_name = "PACKAGE")]
    pub package: String,
}

#[derive(Debug, Args)]
pub struct MetadataArgs {
    /// Machine contract version (1 or 2).
    #[arg(long, value_name = "VERSION")]
    pub format_version: u32,
    /// Include only direct package dependencies.
    #[arg(long)]
    pub no_deps: bool,
}

#[derive(Debug, Args)]
pub struct GenerateShellCompletionArgs {
    /// Shell to generate completions for.
    #[arg(value_enum)]
    pub shell: CompletionShell,
}

#[derive(Debug, Subcommand)]
pub enum SelfCommand {
    /// Update a standalone aru installation to the latest stable release.
    Update(SelfUpdateArgs),
}

#[derive(Debug, Args)]
pub struct SelfUpdateArgs {
    /// Download and validate the update without replacing aru.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct PackageArchiveArgs {
    /// List archive paths without writing an archive.
    #[arg(long, conflicts_with = "output")]
    pub list: bool,
    /// Permit tracked modifications and untracked, non-ignored files.
    #[arg(long)]
    pub allow_dirty: bool,
    /// Write the archive to this path.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,
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
#[command(group(ArgGroup::new("reference").args(["version", "branch", "rev"]).multiple(false)))]
pub struct PackageAddArgs {
    /// GitHub owner/repo, Git URL, SSH source, or local Git repository.
    pub source: String,
    /// Cargo-style SemVer tag requirement (a bare version uses caret semantics).
    #[arg(long, help_heading = "Source Options")]
    pub version: Option<String>,
    /// Moving Git branch to resolve and pin to an exact commit.
    #[arg(long, value_name = "NAME", help_heading = "Source Options")]
    pub branch: Option<String>,
    /// Exact Git commit (7-40 hexadecimal characters).
    #[arg(long, help_heading = "Source Options")]
    pub rev: Option<String>,
    /// Resolve this package instead of reusing its compatible lock.
    #[arg(short = 'U', long, help_heading = "Source Options")]
    pub upgrade: bool,
    /// Configured target that should receive this package; may be repeated.
    #[arg(long = "target", value_name = "TARGET", action = ArgAction::Append)]
    pub targets: Vec<Target>,
    /// Explicitly trust a package-provided MCP name; may be repeated.
    #[arg(long = "trust-mcp", value_name = "NAME", action = ArgAction::Append)]
    pub trust_mcp: Vec<String>,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Resolve and print the plan without writing any file or cache.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Preserve unmanaged Markdown while adding package instruction blocks.
    #[arg(long, conflicts_with = "force", help_heading = "Apply Options")]
    pub merge: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct PackageRemoveArgs {
    /// Declared native aru package source.
    pub source: String,
    /// Update manifest and lock but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Resolve and print the plan without writing any file or cache.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct PackageUpdateArgs {
    /// Declared or transitive package source; omit to update all.
    #[arg(value_name = "PACKAGE")]
    pub packages: Vec<String>,
    /// Select one exact SemVer version compatible with its declared requirement.
    #[arg(long, value_name = "VERSION")]
    pub precise: Option<String>,
    /// Update lock intent but skip target project paths.
    #[arg(long, help_heading = "Apply Options")]
    pub no_sync: bool,
    /// Resolve and print the plan without writing any file or cache.
    #[arg(short = 'n', long, help_heading = "Apply Options")]
    pub dry_run: bool,
    /// Preserve unmanaged Markdown while adding package instruction blocks.
    #[arg(long, conflicts_with = "force", help_heading = "Apply Options")]
    pub merge: bool,
    /// Destructively take over colliding unmanaged entries.
    #[arg(long, help_heading = "Apply Options")]
    pub force: bool,
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

#[derive(Debug, Args)]
pub struct AuditArgs {
    /// Select human-readable text or versioned JSON output.
    #[arg(long, value_enum, default_value_t)]
    pub format: AuditFormat,
    /// Write the report to a file instead of stdout or stderr.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Select the inventory format.
    #[arg(long, value_enum, default_value_t)]
    pub format: ExportFormat,
    /// Write the inventory to a file instead of stdout.
    #[arg(long, value_name = "PATH")]
    pub output_file: Option<PathBuf>,
    /// Set a deterministic RFC 3339 UTC metadata timestamp.
    #[arg(long, value_name = "TIMESTAMP")]
    pub timestamp: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum InstructionCommand {
    /// Add existing AGENTS.md files.
    Add(InstructionAddArgs),
    /// Remove declared instruction source files.
    Remove(InstructionRemoveArgs),
    /// List declared instruction source files.
    List,
}

#[derive(Debug, Args)]
pub struct InstructionAddArgs {
    /// Exact project-relative AGENTS.md file path; may be repeated.
    #[arg(value_name = "FILE", required = true, num_args = 1..)]
    pub files: Vec<String>,
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
    /// List configured or available targets.
    List(TargetListArgs),
}

#[derive(Debug, Args)]
pub struct TargetListArgs {
    /// List every available canonical target, capability, path, and alias.
    #[arg(long)]
    pub available: bool,
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
    /// Target that should receive this dependency; may be repeated.
    #[arg(long = "target", value_name = "TARGET", action = ArgAction::Append, help_heading = "Selection Options")]
    pub targets: Vec<Target>,
    /// Resolve the latest compatible tag or branch head instead of reusing the lock.
    #[arg(short = 'U', long, help_heading = "Source Options")]
    pub upgrade: bool,
    /// Install into target-native user directories instead of the current directory.
    #[arg(short = 'g', long, help_heading = "Apply Options")]
    pub global: bool,
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
    /// Environment variable to forward to --command; may be repeated.
    #[arg(long = "env-var", value_name = "NAME", action = ArgAction::Append, requires = "command", help_heading = "Authentication Options")]
    pub env_vars: Vec<String>,
    /// Environment-backed HTTP header as HEADER=ENV; may be repeated.
    #[arg(long = "header-env", value_name = "HEADER=ENV", action = ArgAction::Append, requires = "url", help_heading = "Authentication Options")]
    pub header_env: Vec<String>,
    /// Registry base URL.
    #[arg(long, help_heading = "Registry Options")]
    pub registry: Option<String>,
    /// Server SemVer requirement, or =literal for an exact non-SemVer version.
    #[arg(long, help_heading = "Registry Options")]
    pub version: Option<String>,
    /// Candidate transport selector (stdio or streamable-http).
    #[arg(long, help_heading = "Registry Options")]
    pub transport: Option<String>,
    /// Candidate package registry selector (npm or PyPI with an explicit uvx hint).
    #[arg(long = "package-registry", help_heading = "Registry Options")]
    pub package_registry: Option<String>,
    /// Environment variable containing a bearer token.
    #[arg(long = "bearer-token-env", help_heading = "Authentication Options")]
    pub bearer_token_env: Option<String>,
    /// Target that should receive this dependency; may be repeated.
    #[arg(long = "target", value_name = "TARGET", action = ArgAction::Append, help_heading = "Apply Options")]
    pub targets: Vec<Target>,
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
