use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::error::{AruError, Result};
use crate::lockfile::{AruPackage, Lockfile};
use crate::manifest::Target;

#[derive(Debug, Clone)]
pub struct PackageGraph {
    nodes: BTreeMap<String, AruPackage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub source: String,
    pub name: String,
    pub version: String,
    pub revision: String,
    pub targets: Vec<Target>,
    pub instructions: usize,
    pub skills: usize,
    pub mcp: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
}

#[derive(Serialize)]
struct GraphReport {
    version: u32,
    roots: Vec<String>,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

impl PackageGraph {
    pub fn from_lock(lock: &Lockfile) -> Result<Self> {
        lock.validate()?;
        Ok(Self {
            nodes: lock
                .aru_packages
                .iter()
                .map(|package| (package.source.clone(), package.clone()))
                .collect(),
        })
    }

    pub fn package(&self, source: &str) -> Option<&AruPackage> {
        self.nodes.get(source)
    }

    pub fn resolve_selector(&self, selector: &str) -> Result<&AruPackage> {
        let exact = self
            .nodes
            .values()
            .filter(|package| package.name == selector || package.source == selector)
            .collect::<Vec<_>>();
        if exact.len() == 1 {
            return Ok(exact[0]);
        }
        let mut matches = self
            .nodes
            .values()
            .filter(|package| package.name.contains(selector) || package.source.contains(selector))
            .collect::<Vec<_>>();
        matches
            .sort_by(|left, right| (&left.name, &left.source).cmp(&(&right.name, &right.source)));
        if matches.len() == 1 {
            return Ok(matches[0]);
        }
        if matches.is_empty() {
            return Err(AruError::msg(format!(
                "package selector {selector:?} matches no locked aru package"
            )));
        }
        let matches = matches
            .iter()
            .map(|package| {
                crate::export::scrub_url(&package.source, "aru package source URL")
                    .map(|source| format!("{} ({source})", package.name))
            })
            .collect::<Result<Vec<_>>>()?;
        Err(AruError::msg(format!(
            "ambiguous package selector {selector:?}; matches: {}",
            matches.join(", ")
        )))
    }

    pub fn text(
        &self,
        max_depth: Option<usize>,
        target: Option<Target>,
        invert: Option<&str>,
    ) -> Result<String> {
        let included = self.included(target);
        if let Some(selector) = invert {
            let package = self.resolve_selector(selector)?;
            if !included.contains(&package.source) {
                return Err(AruError::msg(format!(
                    "package {:?} is not effective for the selected target",
                    package.name
                )));
            }
            let mut output = format!("{} v{}\n", package.name, package.package_version);
            let parents = self.parents(&included);
            let mut seen = BTreeSet::from([package.source.clone()]);
            self.render_children(
                &package.source,
                &parents,
                &included,
                "",
                0,
                max_depth,
                &mut seen,
                &mut output,
            );
            return Ok(output);
        }

        let mut output = String::from("project\n");
        if max_depth == Some(0) {
            return Ok(output);
        }
        let roots = self.roots(&included);
        let children = self.children(&included);
        let mut seen = BTreeSet::new();
        for (index, source) in roots.iter().enumerate() {
            self.render_node(
                source,
                &children,
                &included,
                "",
                index + 1 == roots.len(),
                1,
                max_depth,
                &mut seen,
                &mut output,
            );
        }
        Ok(output)
    }

    pub fn json_bytes(&self, target: Option<Target>, max_depth: Option<usize>) -> Result<Vec<u8>> {
        let mut included = self.included(target);
        if let Some(max_depth) = max_depth {
            included = self.limit_depth(&included, max_depth);
        }
        let mut roots = self.roots(&included);
        let mut nodes = self.graph_nodes(&included);
        let mut edges = self.edges(&included);
        for source in &mut roots {
            *source = crate::export::scrub_url(source, "aru package source URL")?;
        }
        for node in &mut nodes {
            node.source = crate::export::scrub_url(&node.source, "aru package source URL")?;
        }
        for edge in &mut edges {
            edge.from = crate::export::scrub_url(&edge.from, "aru package source URL")?;
            edge.to = crate::export::scrub_url(&edge.to, "aru package source URL")?;
        }
        let report = GraphReport {
            version: 1,
            roots,
            nodes,
            edges,
        };
        let mut bytes = serde_json::to_vec_pretty(&report).map_err(|error| {
            AruError::msg(format!("could not serialize package graph: {error}"))
        })?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn graph_nodes(&self, included: &BTreeSet<String>) -> Vec<GraphNode> {
        let mut nodes = included
            .iter()
            .filter_map(|source| self.nodes.get(source))
            .map(|package| GraphNode {
                source: package.source.clone(),
                name: package.name.clone(),
                version: package.package_version.clone(),
                revision: package.revision.clone(),
                targets: package.targets.clone(),
                instructions: package.instruction_sources.len(),
                skills: package.skills.len(),
                mcp: package.mcp.len(),
            })
            .collect::<Vec<_>>();
        nodes.sort_by(|left, right| (&left.name, &left.source).cmp(&(&right.name, &right.source)));
        nodes
    }

    pub fn all_sources(&self) -> BTreeSet<String> {
        self.nodes.keys().cloned().collect()
    }

    pub fn roots_for(&self, included: &BTreeSet<String>) -> Vec<String> {
        self.roots(included)
    }

    pub fn edges_for(&self, included: &BTreeSet<String>) -> Vec<GraphEdge> {
        self.edges(included)
    }

    fn limit_depth(&self, included: &BTreeSet<String>, max_depth: usize) -> BTreeSet<String> {
        if max_depth == 0 {
            return BTreeSet::new();
        }
        let children = self.children(included);
        let mut selected = BTreeSet::new();
        let mut frontier = self.roots(included);
        for _ in 0..max_depth {
            let mut next = Vec::new();
            for source in frontier {
                if !selected.insert(source.clone()) {
                    continue;
                }
                next.extend(children.get(&source).cloned().unwrap_or_default());
            }
            frontier = next;
        }
        selected
    }

    fn included(&self, target: Option<Target>) -> BTreeSet<String> {
        self.nodes
            .values()
            .filter(|package| target.is_none_or(|target| package.targets.contains(&target)))
            .map(|package| package.source.clone())
            .collect()
    }

    fn roots(&self, included: &BTreeSet<String>) -> Vec<String> {
        let incoming = self
            .edges(included)
            .into_iter()
            .map(|edge| edge.to)
            .collect::<BTreeSet<_>>();
        let mut roots = included
            .iter()
            .filter(|source| !incoming.contains(*source))
            .cloned()
            .collect::<Vec<_>>();
        self.sort_sources(&mut roots);
        roots
    }

    fn edges(&self, included: &BTreeSet<String>) -> Vec<GraphEdge> {
        let mut edges = Vec::new();
        for package in self.nodes.values() {
            if !included.contains(&package.source) {
                continue;
            }
            for dependency in &package.dependencies {
                if included.contains(dependency) {
                    edges.push(GraphEdge {
                        from: package.source.clone(),
                        to: dependency.clone(),
                    });
                }
            }
        }
        edges.sort_by(|left, right| {
            let left_names = (&self.nodes[&left.from].name, &self.nodes[&left.to].name);
            let right_names = (&self.nodes[&right.from].name, &self.nodes[&right.to].name);
            left_names
                .cmp(&right_names)
                .then((&left.from, &left.to).cmp(&(&right.from, &right.to)))
        });
        edges
    }

    fn children(&self, included: &BTreeSet<String>) -> BTreeMap<String, Vec<String>> {
        let mut output = BTreeMap::<String, Vec<String>>::new();
        for edge in self.edges(included) {
            output.entry(edge.from).or_default().push(edge.to);
        }
        for children in output.values_mut() {
            self.sort_sources(children);
        }
        output
    }

    fn parents(&self, included: &BTreeSet<String>) -> BTreeMap<String, Vec<String>> {
        let mut output = BTreeMap::<String, Vec<String>>::new();
        for edge in self.edges(included) {
            output.entry(edge.to).or_default().push(edge.from);
        }
        for parents in output.values_mut() {
            self.sort_sources(parents);
        }
        output
    }

    #[allow(clippy::too_many_arguments)]
    fn render_children(
        &self,
        source: &str,
        adjacency: &BTreeMap<String, Vec<String>>,
        included: &BTreeSet<String>,
        prefix: &str,
        parent_depth: usize,
        max_depth: Option<usize>,
        seen: &mut BTreeSet<String>,
        output: &mut String,
    ) {
        if max_depth.is_some_and(|depth| parent_depth >= depth) {
            return;
        }
        let children = adjacency.get(source).cloned().unwrap_or_default();
        for (index, child) in children.iter().enumerate() {
            self.render_node(
                child,
                adjacency,
                included,
                prefix,
                index + 1 == children.len(),
                parent_depth + 1,
                max_depth,
                seen,
                output,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_node(
        &self,
        source: &str,
        adjacency: &BTreeMap<String, Vec<String>>,
        included: &BTreeSet<String>,
        prefix: &str,
        is_last: bool,
        depth: usize,
        max_depth: Option<usize>,
        seen: &mut BTreeSet<String>,
        output: &mut String,
    ) {
        if !included.contains(source) {
            return;
        }
        let package = &self.nodes[source];
        output.push_str(prefix);
        output.push_str(if is_last { "└── " } else { "├── " });
        output.push_str(&package.name);
        output.push_str(" v");
        output.push_str(&package.package_version);
        if !seen.insert(source.into()) {
            output.push_str(" (*)\n");
            return;
        }
        output.push('\n');
        if max_depth.is_some_and(|limit| depth >= limit) {
            return;
        }
        let next_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
        self.render_children(
            source,
            adjacency,
            included,
            &next_prefix,
            depth,
            max_depth,
            seen,
            output,
        );
    }

    fn sort_sources(&self, sources: &mut [String]) {
        sources.sort_by(|left, right| {
            (&self.nodes[left].name, left).cmp(&(&self.nodes[right].name, right))
        });
    }
}
