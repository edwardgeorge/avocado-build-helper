use clap::{App, Arg};
use std::path::Path;

mod hasher;
mod types;
use hasher::*;

fn main() -> std::result::Result<(), anyhow::Error> {
    env_logger::init();
    let matches = App::new("Build Helper")
        .arg(
            Arg::with_name("directory")
                .required(false)
                .index(1)
                .default_value("."),
        )
        .arg(
            Arg::with_name("pretty-print")
                .short("-p")
                .required(false)
                .takes_value(false),
        )
        .get_matches();
    let p: &Path = matches.value_of_os("directory").unwrap().as_ref();
    let path = p.canonicalize()?;
    run_hasher(&path, matches.is_present("pretty-print"))
}
