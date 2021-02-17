use serde::{Deserialize, Serialize};
use serde_json::{from_reader, Value};
use std::collections::{BinaryHeap, HashSet};
use std::fs::File;
use std::hash::Hash;
use std::path::Path;
use std::vec::Vec;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CustomError {
    #[error("Component spec issue: Missing transitive dependencies: {0:?}")]
    MissingDepError(Vec<String>),
    #[error("Component spec issue: Missing components named: {0:?}")]
    MissingComponentError(Vec<String>),
    #[error("Component spec issue: Cycle found with or unfound dependencies for:\n {0}")]
    CycleError(String),
    #[error("Duplicate property name: {name}")]
    DuplicatePropertyNameError { name: String },
    #[error("Error in template for property {prop_name}:\n{error}")]
    TemplateError {
        prop_name: String,
        error: handlebars::TemplateError,
    },
    #[error("Invalid argument format {argument}, requires an '='")]
    PropMissingEqualsError { argument: String },
    #[error("Command {cmd:?} was not successful: {reason}")]
    UnsuccessfulCommandError { cmd: String, reason: String },
    #[error("Error parsing command {cmd:?}:\n{error}")]
    CommandParseError {
        cmd: String,
        error: shell_words::ParseError,
    },
    #[error("Error rendering template for {cmd_name}:\n{error}")]
    TemplateRenderError {
        cmd_name: String,
        error: handlebars::RenderError,
    },
    #[error("Error attempting to execute command for {cmd_name}:\n{error}")]
    CommandExecutionError {
        cmd_name: String,
        error: std::io::Error,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub dir: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default = "Vec::new")]
    pub dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha_short: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_sha_short: Option<String>,
    #[serde(flatten)]
    pub rem: Value,
}

impl Component {
    pub fn depset(&self) -> HashSet<String> {
        self.dependencies.iter().map(|v| v.to_owned()).collect()
    }
    pub fn depsorted(&self) -> Vec<String> {
        self.dependencies
            .iter()
            .map(|a| a.to_owned())
            .collect::<BinaryHeap<_>>()
            .into_sorted_vec()
    }
}

pub fn load_components(path: &Path) -> Vec<Component> {
    let f = File::open(path.join("components.json")).unwrap();
    from_reader(f).unwrap()
}

pub fn toposort_components(inp: Vec<Component>) -> Result<Vec<Component>, CustomError> {
    toposort(inp, |a| a.dir.to_owned(), |a| a.depset())
}

pub fn transitive_dependencies(
    inp: Vec<Component>,
    dirs: &[&str],
    include_roots: bool,
    reverse_order: bool,
) -> Result<Vec<Component>, CustomError> {
    let mut deps = toposort_components(inp)?;
    deps.reverse();
    let mut needed: HashSet<String> = dirs.iter().map(|x| (*x).to_owned()).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut result = Vec::<Component>::new();
    for item in deps.drain(..) {
        log::debug!(
            "Searching for transitive deps at '{}', needed: {:?}",
            item.dir,
            needed
        );
        if needed.remove(&item.dir) {
            let deps = item.depset();
            let is_root = !seen.contains(&item.dir[..]);
            seen.extend(deps.iter().map(|x| x.clone()));
            needed.extend(deps);
            if !is_root || include_roots {
                result.push(item);
            }
        }
        if needed.is_empty() {
            // no more dependencies needed; short-circuit
            break;
        }
    }
    if !needed.is_empty() {
        return Err(CustomError::MissingDepError(needed.drain().collect()));
    }
    if reverse_order {
        Ok(result)
    } else {
        result.reverse();
        Ok(result)
    }
}

pub fn transitive_dependents(
    inp: Vec<Component>,
    dirs: &[&str],
    include_roots: bool,
) -> Result<Vec<Component>, CustomError> {
    let mut roots: HashSet<&str> = dirs.into_iter().map(|x| *x).collect();
    let mut deps = toposort_components(inp)?;
    //let mut seen: HashSet<&str> = HashSet::new();
    let mut seen: HashSet<String> = dirs.into_iter().map(|x| (*x).to_owned()).collect();
    let mut result = Vec::<Component>::new();
    for item in deps.drain(..) {
        roots.remove(&item.dir[..]);
        if !item.depset().is_disjoint(&seen) {
            seen.insert(item.dir.clone());
            result.push(item);
        } else if seen.contains(&item.dir) && include_roots {
            // if our deps aren't in seen, but we are, then we're a root
            result.push(item);
        }
    }
    if !roots.is_empty() {
        return Err(CustomError::MissingComponentError(
            roots.drain().map(str::to_owned).collect(),
        ));
    }
    Ok(result)
}

fn toposort<A, K, F, G>(inp: Vec<A>, key: F, fdep: G) -> Result<Vec<A>, CustomError>
where
    K: Eq + Hash + std::fmt::Debug,
    F: Fn(&A) -> K,
    G: Fn(&A) -> HashSet<K>,
{
    // very simple (warning: n^2) topological sort (_not_ tarjan)
    // keeps popping items out of a vec in passes until it makes a whole pass without
    // popping any off, in this case where we cannot make progress we must
    // have a loop somewhere.
    let mut inp = inp;
    let mut res = Vec::new();
    let mut seen: HashSet<K> = HashSet::new();
    let mut rem: Vec<_> = inp
        .drain(..)
        .map(|a| {
            let k = key(&a);
            let deps = fdep(&a);
            (a, k, deps)
        })
        .collect();
    let mut non = Vec::new();
    loop {
        if rem.is_empty() {
            return Ok(res);
        }
        let mut active = false;
        for (v, k, deps) in rem.drain(..) {
            if deps.is_subset(&seen) {
                res.push(v);
                seen.insert(k);
                active = true;
            } else {
                non.push((v, k, deps));
            }
        }
        if !active {
            let keys: Vec<String> = non
                .iter()
                .map(|(_, k, d)| {
                    let unfound = d
                        .iter()
                        .filter(|k| !seen.contains(k))
                        .map(|k| format!("{:?}", k))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{:?} (unsatisfied deps: {})", k, unfound)
                })
                .collect();
            return Err(CustomError::CycleError(keys.join(",\n ")));
        }
        std::mem::swap(&mut rem, &mut non);
    }
}
