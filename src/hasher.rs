use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::convert::TryInto;
use std::hash::Hash;
use std::io::{stdout, Write};
use std::path::Path;
use std::process::Command;
use std::str::from_utf8;

use crate::types::*;

pub fn run_hasher<F>(
    path: &Path,
    pretty_print: bool,
    remove_dependencies: bool,
    post_process: F,
) -> Result<(), anyhow::Error>
where
    F: Fn(&mut Component) -> anyhow::Result<()>,
{
    let mut x = load_components(path);
    x = toposort_components(x);
    let mut n: HashMap<String, (i32, [u8; 32])> = HashMap::new();
    let x: Result<Vec<_>> = x
        .iter_mut()
        .map(|comp| {
            log::debug!(
                "Calculating hashes for {}, dependencies: {:?}",
                comp.dir,
                comp.dependencies
            );
            let commit_hash = hash_for_dir(path, &path.join(&comp.dir)).unwrap();
            let res = hash_for_node(&commit_hash, &comp.depsorted(), &n);
            n.insert(comp.dir.to_owned(), res);
            let tree_hex = hex::encode(res.1);
            comp.commit_sha_short = Some(commit_hash[..8].to_owned());
            comp.commit_sha = Some(commit_hash);
            comp.tree_sha_short = Some(tree_hex[..16].to_owned());
            comp.tree_sha = Some(tree_hex);
            if remove_dependencies {
                comp.dependencies = Vec::new();
            }
            post_process(comp)?;
            Ok(comp)
        })
        .collect();
    let y = x?;
    let json = if pretty_print {
        serde_json::to_string_pretty(&y)?
    } else {
        serde_json::to_string(&y)?
    };
    stdout().write_all(json.as_ref())?;
    Ok(())
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
    log::debug!(
        "root: [depth: {}] {:?} [child data: {:?}] [2]",
        mydepth as u16,
        root_hash,
        data
    );
    (
        mydepth,
        hasher
            .finalize()
            .as_slice()
            .try_into()
            .expect("Wrong length hash"),
    )
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
        log::debug!(
            "child ({}): [offset: {}] [depth: {}] {:?} [last node: {}]",
            deps[i],
            i as u32,
            *v as u16,
            hash,
            last_node
        );
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
