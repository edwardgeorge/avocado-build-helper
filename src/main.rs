use anyhow::{anyhow, Result};
use clap::{App, Arg};
use serde::{Deserialize, Serialize};
use serde_json::{from_reader, Value};
use sha2::{Digest, Sha256};
use std::borrow::Borrow;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::convert::TryInto;
use std::fs::{File};
use std::hash::Hash;
use std::io::{stdout, Write};
use std::path::Path;
use std::process::Command;
use std::str::from_utf8;
use std::vec::Vec;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Component {
    dir: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default = "Vec::new")]
    dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tree_sha: Option<String>,
    #[serde(flatten)]
    rem: Value,
}

impl Component {
    fn depset(&self) -> HashSet<String> {
        self.dependencies.iter().map(|v| v.to_owned()).collect()
    }
    fn depsorted(&self) -> Vec<String> {
        self.dependencies.iter().map(|a| a.to_owned()).collect::<BinaryHeap<_>>().into_sorted_vec()
    }
}

fn hash_for_dir(git_dir: &Path, path: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(&[
            "-C".as_ref(),
            git_dir,
            "log".as_ref(),
            "-1".as_ref(),
            "--pretty=format:%H".as_ref(),
            path,
        ])
        .output()?;
    if out.status.success() {
        let r = String::from(from_utf8(&out.stdout)?.trim());
        Ok(r)
    } else {
        match out.status.code() {
            Some(c) => Err(anyhow!(
                "git command exited with error code: {}\n{}",
                c,
                from_utf8(&out.stderr)?
            )),
            None => Err(anyhow!("git command exited with signal")),
        }
    }
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
    let mut rem: Vec<_> =
        inp.drain(..).map(|a| {
            let k = key(&a);
            let deps = fdep(&a);
            (a, k, deps)
        }).collect();
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

fn main() -> std::result::Result<(), anyhow::Error> {
    env_logger::init();
    let matches = App::new("Build Helper")
        .arg(
            Arg::with_name("directory")
                .required(false)
                .index(1)
                .default_value("."),
        ).arg(
            Arg::with_name("pretty-print")
                .short("-p")
                .required(false)
                .takes_value(false)
        )
        .get_matches();
    let p: &Path = matches.value_of_os("directory").unwrap().as_ref();
    let a = p.canonicalize()?;
    let f = File::open(p.join("components.json")).unwrap();
    let mut x: Vec<Component> = from_reader(f).unwrap();
    x = toposort(x, |a| a.dir.to_owned(), |a| a.depset());
    let mut n: HashMap<String, (i32, [u8; 32])> = HashMap::new();
    let x: Vec<_> = x.iter_mut().map(|comp| {
        log::debug!("Calculating hashes for {}, dependencies: {:?}", comp.dir, comp.dependencies);
        let commit_hash = hash_for_dir(&a, &a.join(&comp.dir)).unwrap();
        let res = hash_for_node(&commit_hash, &comp.depsorted(), &n);
        n.insert(comp.dir.to_owned(), res);
        comp.commit_sha = Some(commit_hash);
        comp.tree_sha = Some(hex::encode(res.1));
        comp
    }).collect();
    let json = if matches.is_present("pretty-print") {
        serde_json::to_string_pretty(&x)?
    } else {
        serde_json::to_string(&x)?
    };
    stdout().write_all(json.as_ref())?;
    Ok(())
}

fn hash_for_node<S, T>(
    node_hash: &str,
    deps: &Vec<S>,
    hashes: &HashMap<T, (i32, [u8; 32])>,
) -> (i32, [u8; 32])
where
    S: Borrow<T> + std::fmt::Display,
    T: Hash + Eq,
{
    // hash format:
    // [depth u16] hash [child data] [root node u8 = 2]
    // child data = * [offset u32] [depth u16] hash [end flag u8 = 0/1]
    let mut hasher = Sha256::new();
    let (d, data) = build_hash(deps, hashes);
    let mydepth = d + 1;
    let root_hash = hex::decode(node_hash).unwrap();
    hasher.update((mydepth as u16).to_be_bytes());
    hasher.update(&root_hash);
    hasher.update(&data);
    hasher.update([2]);
    log::debug!("root: [depth: {}] {:?} [child data: {:?}] [2]", mydepth as u16, root_hash, data);
    (mydepth, hasher.finalize().as_slice().try_into().expect("Wrong length hash"))
}

fn build_hash<S, T>(deps: &Vec<S>, hashes: &HashMap<T, (i32, [u8; 32])>) -> (i32, Vec<u8>)
where
    S: Borrow<T> + std::fmt::Display,
    T: Hash + Eq,
{
    let x = translate(deps, hashes);
    let mut r = Vec::with_capacity(deps.len() * (4 + 2 + 32 + 1));
    let d1 = x.iter().enumerate().fold(-1, |acc, (i, (v, hash))| {
        let last_node = deps.len() - 1 == i;
        r.extend((i as u32).to_be_bytes().iter());
        r.extend((*v as u16).to_be_bytes().iter());
        r.extend(hash.as_ref());
        r.push(if last_node { 1 } else { 0 });
        log::debug!("child ({}): [offset: {}] [depth: {}] {:?} [last node: {}]", deps[i], i as u32, *v as u16, hash, last_node);
        std::cmp::max(acc, *v)
    });
    (d1, r)
}

fn translate<'a, S, T, V>(inp: &Vec<S>, map: &'a HashMap<T, V>) -> Vec<&'a V>
where
    S: Borrow<T>,
    T: Hash + Eq,
{
    inp.iter()
        .map(|item| map.get(item.borrow()).unwrap())
        .collect()
}