use std::collections::BTreeSet;
use std::path::Path;

use crate::workspace::Workspace;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Check {
    Lint,
    UnitTests(BTreeSet<String>),
    AllUnitTests,
    EndToEnd(Suite),
}

impl Check {
    pub(crate) fn make_arguments(&self) -> Vec<String> {
        match self {
            Self::Lint => vec!["lint".into()],
            Self::AllUnitTests => vec!["test".into()],
            Self::UnitTests(crates) => vec![
                "test".into(),
                format!(
                    "CRATES={}",
                    crates.iter().cloned().collect::<Vec<_>>().join(" ")
                ),
            ],
            Self::EndToEnd(suite) => vec![suite.make_target().into()],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Suite {
    Pane,
    Page,
}

impl Suite {
    const ALL: [Self; 2] = [Self::Pane, Self::Page];

    fn make_target(self) -> &'static str {
        match self {
            Self::Pane => "e2e-tui",
            Self::Page => "e2e-explore",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Pane => "the pane's e2e tests",
            Self::Page => "the Explore page's e2e tests",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Owner {
    Package(String),
    Suite(Suite),
    Nothing,
    Lock,
    Everything,
    Unmapped,
}

const EVERYTHING: &[&str] = &["Cargo.toml", "nix/", "Makefile", ".config/"];

const SUITES: &[(&str, Suite)] = &[
    ("tests/tui/", Suite::Pane),
    ("tests/explore-page/", Suite::Page),
];

const NOTHING: &[&str] = &[
    "docs/",
    ".agents/",
    ".claude/",
    "scripts/",
    "LICENSE",
    ".gitignore",
    ".envrc",
    ".mcp.json",
    "herdr-plugin.toml",
    "herdr-app-mod/",
];

#[derive(Debug, Default)]
pub(crate) struct Changes {
    pub(crate) paths: Vec<String>,
    pub(crate) locked: Option<BTreeSet<String>>,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct Plan {
    pub(crate) checks: Vec<Check>,
    pub(crate) reasons: Vec<String>,
}

#[derive(Default)]
struct Owners {
    packages: BTreeSet<String>,
    suites: BTreeSet<Suite>,
    run_everything: Vec<String>,
}

impl Owners {
    fn of(workspace: &Workspace, changes: &Changes) -> Self {
        let mut owners = Self::default();
        for path in &changes.paths {
            match workspace.owner(path) {
                Owner::Package(name) => {
                    owners.packages.insert(name);
                }
                Owner::Suite(suite) => {
                    owners.suites.insert(suite);
                }
                Owner::Lock => match &changes.locked {
                    Some(packages) => owners.packages.extend(packages.iter().cloned()),
                    None => owners
                        .run_everything
                        .push("Cargo.lock changed in a way check-changed cannot read".into()),
                },
                Owner::Nothing => {}
                Owner::Everything => owners
                    .run_everything
                    .push(format!("{path} reaches every check")),
                Owner::Unmapped => owners
                    .run_everything
                    .push(format!("{path} is in no check's map")),
            }
        }
        owners
    }
}

impl Workspace {
    fn owner(&self, path: &str) -> Owner {
        if path == "Cargo.lock" {
            return Owner::Lock;
        }
        if EVERYTHING.iter().any(|rule| matches_rule(path, rule)) {
            return Owner::Everything;
        }
        let in_crates = path.starts_with("crates/") || path.starts_with("vendor/");
        let markdown = Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
        if markdown && !in_crates {
            return Owner::Nothing;
        }
        if let Some((_, suite)) = SUITES.iter().find(|(rule, _)| matches_rule(path, rule)) {
            return Owner::Suite(*suite);
        }
        let package = self
            .packages
            .iter()
            .filter(|(directory, _)| path.starts_with(&format!("{directory}/")))
            .max_by_key(|(directory, _)| directory.len());
        if let Some((_, name)) = package {
            return Owner::Package(name.clone());
        }
        if NOTHING.iter().any(|rule| matches_rule(path, rule)) {
            return Owner::Nothing;
        }
        Owner::Unmapped
    }

    pub(crate) fn plan(&self, changes: &Changes) -> Plan {
        let owners = Owners::of(self, changes);
        if !owners.run_everything.is_empty() {
            let mut checks = vec![Check::Lint, Check::AllUnitTests];
            checks.extend(Suite::ALL.map(Check::EndToEnd));
            return Plan {
                checks,
                reasons: owners.run_everything,
            };
        }
        let reached = self.reached(&owners.packages);
        let tested: BTreeSet<_> = reached.intersection(&self.members).cloned().collect();
        let mut plan = Plan::default();
        if !tested.is_empty() {
            plan.reasons.push(format!(
                "changed packages: {}",
                owners
                    .packages
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            plan.checks.push(Check::UnitTests(tested));
        }
        for suite in Suite::ALL {
            let built = self.built.get(&suite);
            if owners.suites.contains(&suite) {
                plan.reasons
                    .push(format!("{}: their harness changed", suite.name()));
                plan.checks.push(Check::EndToEnd(suite));
            } else if built.is_some_and(|built| !reached.is_disjoint(built)) {
                plan.reasons
                    .push(format!("{}: they build a changed crate", suite.name()));
                plan.checks.push(Check::EndToEnd(suite));
            }
        }
        if plan.checks.is_empty() {
            plan.reasons
                .push("no changed file is read by a check".into());
        } else {
            plan.checks.insert(0, Check::Lint);
        }
        plan
    }

    pub(crate) fn unmapped<'a>(&self, tracked: &'a [String]) -> Vec<&'a str> {
        tracked
            .iter()
            .filter(|path| self.owner(path) == Owner::Unmapped)
            .map(String::as_str)
            .collect()
    }

    fn reached(&self, packages: &BTreeSet<String>) -> BTreeSet<String> {
        let mut reached = packages.clone();
        let mut pending: Vec<String> = packages.iter().cloned().collect();
        while let Some(package) = pending.pop() {
            for dependent in self.dependents.get(&package).into_iter().flatten() {
                if reached.insert(dependent.clone()) {
                    pending.push(dependent.clone());
                }
            }
        }
        reached
    }
}

fn matches_rule(path: &str, rule: &str) -> bool {
    if rule.ends_with('/') {
        path.starts_with(rule)
    } else {
        path == rule
    }
}

#[cfg(test)]
#[path = "plan.tests.rs"]
mod tests;
