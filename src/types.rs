use serde::{Deserialize, Serialize};
use serde_json::{from_reader, Value};
use std::collections::{BinaryHeap, HashSet};
use std::fs::File;
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
