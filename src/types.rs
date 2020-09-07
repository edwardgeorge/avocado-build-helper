use serde::{Deserialize, Serialize};
use serde_json::{from_reader, Value};
use std::collections::{BinaryHeap, HashSet};
use std::fs::File;
use std::hash::Hash;
use std::path::Path;
use std::vec::Vec;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub dir: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default = "Vec::new")]
    pub dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_sha: Option<String>,
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

pub fn toposort_components(inp: Vec<Component>) -> Vec<Component> {
    toposort(inp, |a| a.dir.to_owned(), |a| a.depset())
}

fn toposort<A, K, F, G>(inp: Vec<A>, key: F, fdep: G) -> Vec<A>
where
    K: Eq + Hash + std::fmt::Debug,
    F: Fn(&A) -> K,
    G: Fn(&A) -> HashSet<K>,
{
    // very simple topological sort (_not_ tarjan)
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
            return res;
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
            let keys: Vec<_> = non.iter().map(|(_, k, _)| k).collect();
            panic!("Cycle found in deps! participants: {:?}", keys);
        }
        std::mem::swap(&mut rem, &mut non);
    }
}
