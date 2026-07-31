use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::cli::{InfoArgs, MetadataArgs, TreeArgs, TreeFormat};
use crate::error::{AruError, Result};
use crate::export::scrub_url;
use crate::graph::{GraphEdge, GraphNode, PackageGraph};
use crate::lockfile::Lockfile;

use super::ExecutionPolicy;

fn required_lock(project: &Path) -> Result<Lockfile> {
    Lockfile::load_optional(project)?.ok_or_else(|| {
        AruError::msg("aru.lock is missing; run `aru lock` before inspecting the package graph")
    })
}

pub fn tree(project: &Path, args: TreeArgs) -> Result<()> {
    let graph = PackageGraph::from_lock(&required_lock(project)?)?;
    match args.format {
        TreeFormat::Text => print!(
            "{}",
            graph.text(args.depth, args.target, args.invert.as_deref())?
        ),
        TreeFormat::Json => {
            if args.invert.is_some() {
                return Err(AruError::msg(
                    "--invert is supported only with text tree output",
                ));
            }
            print!(
                "{}",
                String::from_utf8_lossy(&graph.json_bytes(args.target, args.depth)?)
            );
        }
    }
    Ok(())
}

pub fn info(project: &Path, args: InfoArgs, policy: ExecutionPolicy) -> Result<()> {
    if let Some(lock) = Lockfile::load_optional(project)? {
        let graph = PackageGraph::from_lock(&lock)?;
        match graph.resolve_selector(&args.package) {
            Ok(package) => {
                let source = scrub_url(&package.source, "aru package source URL")?;
                println!("name:         {}", package.name);
                println!("locked:       {}", package.package_version);
                println!("source:       {source}");
                println!("revision:     {}", package.revision);
                println!("targets:      {}", comma_targets(&package.targets));
                println!("instructions: {}", package.instruction_sources.len());
                println!("skills:       {}", package.skills.len());
                println!("mcp:          {}", package.mcp.len());
                println!("dependencies: {}", package.dependencies.len());
                return Ok(());
            }
            Err(error) if error.to_string().contains("matches no locked") => {}
            Err(error) => return Err(error),
        }
    }

    let package = crate::package::resolver::inspect_source(project, &args.package, policy.offline)?;
    println!("name:         {}", package.manifest.package.name);
    println!("available:    {}", package.version);
    println!(
        "source:       {}",
        scrub_url(&package.source, "aru package source URL")?
    );
    println!("revision:     {}", package.revision);
    println!(
        "instructions: {}",
        package.manifest.instructions.sources.len()
    );
    println!("skills:       {}", package.manifest.skills.len());
    println!("mcp:          {}", package.manifest.mcp.len());
    println!("dependencies: {}", package.manifest.dependencies.len());
    Ok(())
}

fn comma_targets(targets: &[crate::manifest::Target]) -> String {
    targets
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Serialize)]
struct MetadataReport {
    format_version: u32,
    project_root: String,
    lock_version: u32,
    project_targets: Vec<crate::manifest::Target>,
    roots: Vec<String>,
    packages: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    instructions: Vec<InstructionMetadata>,
    skills: Vec<SkillMetadata>,
    mcp: Vec<McpMetadata>,
    projections: Vec<ProjectionMetadata>,
}

#[derive(Serialize)]
struct InstructionMetadata {
    source: String,
    targets: Vec<crate::manifest::Target>,
    sha256: String,
    managed: bool,
}

#[derive(Serialize)]
struct SkillMetadata {
    source: String,
    name: String,
    version: String,
    revision: String,
    targets: Vec<crate::manifest::Target>,
    sha256: String,
}

#[derive(Serialize)]
struct McpMetadata {
    name: String,
    package: String,
    version: String,
    targets: Vec<crate::manifest::Target>,
}

#[derive(Serialize)]
struct ProjectionMetadata {
    target: crate::manifest::Target,
    kind: String,
    key: String,
    sha256: String,
}

pub fn metadata(project: &Path, args: MetadataArgs) -> Result<()> {
    if args.format_version != 1 {
        return Err(AruError::msg(format!(
            "unsupported metadata format version {}; supported version: 1",
            args.format_version
        )));
    }
    let manifest = crate::manifest::ManifestDocument::load(project)?.manifest()?;
    let lock = required_lock(project)?;
    let graph = PackageGraph::from_lock(&lock)?;
    let all = graph.all_sources();
    let included = if args.no_deps {
        graph.roots_for(&all).into_iter().collect::<BTreeSet<_>>()
    } else {
        all
    };
    let package_sources = lock
        .aru_packages
        .iter()
        .map(|package| package.source.as_str())
        .collect::<BTreeSet<_>>();
    let included_instruction_sources = lock
        .aru_packages
        .iter()
        .filter(|package| included.contains(&package.source))
        .flat_map(|package| {
            package
                .instruction_sources
                .iter()
                .map(|source| source.source.as_str())
        })
        .collect::<BTreeSet<_>>();

    let mut packages = graph.graph_nodes(&included);
    for package in &mut packages {
        package.source = scrub_url(&package.source, "aru package source URL")?;
    }
    let mut edges = graph.edges_for(&included);
    for edge in &mut edges {
        edge.from = scrub_url(&edge.from, "aru package source URL")?;
        edge.to = scrub_url(&edge.to, "aru package source URL")?;
    }
    let roots = graph
        .roots_for(&included)
        .into_iter()
        .map(|source| scrub_url(&source, "aru package source URL"))
        .collect::<Result<Vec<_>>>()?;

    let instructions: Vec<InstructionMetadata> = lock
        .instruction_sources
        .iter()
        .filter(|source| {
            !source.managed || included_instruction_sources.contains(source.source.as_str())
        })
        .map(|source| InstructionMetadata {
            source: source.source.clone(),
            targets: source.targets.clone(),
            sha256: source.sha256.clone(),
            managed: source.managed,
        })
        .collect();
    let mut skills = Vec::new();
    for package in &lock.skill_packages {
        if package_sources.contains(package.source.as_str()) && !included.contains(&package.source)
        {
            continue;
        }
        for skill in &package.skills {
            skills.push(SkillMetadata {
                source: scrub_url(&package.source, "skill source URL")?,
                name: skill.name.clone(),
                version: package.version.clone(),
                revision: package.revision.clone(),
                targets: package.targets.clone(),
                sha256: skill.sha256.clone(),
            });
        }
    }
    skills.sort_by(|left, right| (&left.name, &left.source).cmp(&(&right.name, &right.source)));
    let package_mcp = lock
        .aru_packages
        .iter()
        .filter(|package| !included.contains(&package.source))
        .flat_map(|package| package.mcp.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    let mut mcp = lock
        .mcp_servers
        .iter()
        .filter(|server| !package_mcp.contains(server.name.as_str()))
        .map(|server| McpMetadata {
            name: server.name.clone(),
            package: server.server_id.clone(),
            version: server.version.clone(),
            targets: server.targets.iter().map(|target| target.target).collect(),
        })
        .collect::<Vec<_>>();
    mcp.sort_by(|left, right| left.name.cmp(&right.name));
    let instruction_keys = instructions
        .iter()
        .map(|source| source.source.as_str())
        .collect::<BTreeSet<_>>();
    let skill_keys = skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect::<BTreeSet<_>>();
    let mcp_keys = mcp
        .iter()
        .map(|server| server.name.as_str())
        .collect::<BTreeSet<_>>();
    let projections = lock
        .projection_baselines
        .iter()
        .filter(|projection| match projection.kind.as_str() {
            "instruction" => instruction_keys.contains(projection.key.as_str()),
            "skill" => skill_keys.contains(projection.key.as_str()),
            "mcp" => mcp_keys.contains(projection.key.as_str()),
            _ => true,
        })
        .map(|projection| ProjectionMetadata {
            target: projection.target,
            kind: projection.kind.clone(),
            key: projection.key.clone(),
            sha256: projection.sha256.clone(),
        })
        .collect();

    let project_root = project
        .to_str()
        .ok_or_else(|| AruError::msg("project root is not UTF-8"))?
        .replace(std::path::MAIN_SEPARATOR, "/");
    let report = MetadataReport {
        format_version: 1,
        project_root,
        lock_version: lock.version,
        project_targets: manifest.project.targets,
        roots,
        packages,
        edges,
        instructions,
        skills,
        mcp,
        projections,
    };
    let mut bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| AruError::msg(format!("could not serialize metadata: {error}")))?;
    bytes.push(b'\n');
    print!("{}", String::from_utf8_lossy(&bytes));
    Ok(())
}
