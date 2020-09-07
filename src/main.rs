use clap::{App, AppSettings, Arg, SubCommand};
use std::path::Path;

mod hasher;
mod types;
use hasher::*;

fn main() -> std::result::Result<(), anyhow::Error> {
    env_logger::init();
    let matches = App::new("Build Helper")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("hash-components")
                .about("Annotate components.json with hashes")
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
                ),
        )
        .get_matches();
    if let Some(m) = matches.subcommand_matches("hash-components") {
        let p: &Path = m.value_of_os("directory").unwrap().as_ref();
        let path = p.canonicalize()?;
        run_hasher(&path, m.is_present("pretty-print"))
    } else {
        panic!("unexpected subcommand")
    }
}
