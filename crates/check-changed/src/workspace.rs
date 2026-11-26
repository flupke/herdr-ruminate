use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use eyre::{Context, ContextCompat, Result};
use serde_json::Value;

use crate::plan::Suite;

const PANE_ROOT: &str = "reviewer";
const PAGE_ROOT: &str = "review-explore-page-server";
const PANE_HARNESS: &str = "tests/tui/Cargo.toml";

#[derive(Debug, Default)]
pub(crate) struct Workspace {
    pub(crate) packages: BTreeMap<String, String>,
    pub(crate) members: BTreeSet<String>,
    pub(crate) dependents: BTreeMap<String, BTreeSet<String>>,
    pub(crate) built: BTreeMap<Suite, BTreeSet<String>>,
}

impl Workspace {
    pub(crate) fn read(root: &Path) -> Result<Self> {
        let graph = Graph::new(root, &metadata(root, None)?)?;
        let harness = Graph::new(root, &metadata(root, Some(PANE_HARNESS))?)?;
        let mut pane = graph.built_by(PANE_ROOT)?;
        pane.extend(harness.dependencies.into_keys());
        Ok(Self {
            built: BTreeMap::from([
                (Suite::Pane, pane),
                (Suite::Page, graph.built_by(PAGE_ROOT)?),
            ]),
            members: graph.members.clone(),
            dependents: graph.dependents(),
            packages: graph.packages,
        })
    }
}

fn metadata(root: &Path, manifest: Option<&str>) -> Result<Value> {
    let mut command = Command::new("cargo");
    command
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(root);
    if let Some(manifest) = manifest {
        command.args(["--manifest-path", manifest]);
    }
    let output = command.output().context("run cargo metadata")?;
    eyre::ensure!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("read cargo metadata")
}

struct Graph {
    packages: BTreeMap<String, String>,
    members: BTreeSet<String>,
    dependencies: BTreeMap<String, Vec<Dependency>>,
}

struct Dependency {
    name: String,
    built: bool,
}

impl Graph {
    fn new(root: &Path, metadata: &Value) -> Result<Self> {
        let mut names = BTreeMap::new();
        let mut packages = BTreeMap::new();
        for package in array(metadata, "packages")? {
            let name = text(package, "name")?;
            names.insert(text(package, "id")?.to_owned(), name.to_owned());
            if package["source"].is_null()
                && let Some(directory) = Path::new(text(package, "manifest_path")?)
                    .parent()
                    .and_then(|directory| directory.strip_prefix(root).ok())
            {
                packages.insert(directory.display().to_string(), name.to_owned());
            }
        }
        let members = array(metadata, "workspace_members")?
            .iter()
            .filter_map(|id| id.as_str().and_then(|id| names.get(id)).cloned())
            .collect();
        let mut dependencies: BTreeMap<String, Vec<Dependency>> = BTreeMap::new();
        for node in array(&metadata["resolve"], "nodes")? {
            let name = names
                .get(text(node, "id")?)
                .context("cargo metadata resolves an unknown package")?;
            let of_node = array(node, "deps")?.iter().filter_map(|dependency| {
                let name = names.get(dependency["pkg"].as_str()?)?.clone();
                // cargo metadata gives a normal dependency the kind null, else "build" or "dev".
                let built = dependency["dep_kinds"]
                    .as_array()?
                    .iter()
                    .any(|kind| kind["kind"].as_str() != Some("dev"));
                Some(Dependency { name, built })
            });
            dependencies
                .entry(name.clone())
                .or_default()
                .extend(of_node);
        }
        Ok(Self {
            packages,
            members,
            dependencies,
        })
    }

    fn dependents(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (package, dependencies) in &self.dependencies {
            for dependency in dependencies {
                dependents
                    .entry(dependency.name.clone())
                    .or_default()
                    .insert(package.clone());
            }
        }
        dependents
    }

    fn built_by(&self, root: &str) -> Result<BTreeSet<String>> {
        eyre::ensure!(
            self.dependencies.contains_key(root),
            "{root} is not a package of the workspace"
        );
        let mut built = BTreeSet::from([root.to_owned()]);
        let mut pending = vec![root.to_owned()];
        while let Some(package) = pending.pop() {
            for dependency in self.dependencies.get(&package).into_iter().flatten() {
                if dependency.built && built.insert(dependency.name.clone()) {
                    pending.push(dependency.name.clone());
                }
            }
        }
        Ok(built)
    }
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value[key]
        .as_array()
        .with_context(|| format!("cargo metadata has no {key} list"))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("cargo metadata has no {key}"))
}

#[cfg(test)]
#[path = "workspace.tests.rs"]
mod tests;
