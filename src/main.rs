use clap::{App, AppSettings, Arg, SubCommand};
use std::path::Path;

mod dockerignore;
mod executor;
mod hasher;
mod types;
use dockerignore::*;
use executor::{annotate_component, CommandRegistry};
use hasher::*;

fn main() -> Result<(), anyhow::Error> {
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
                )
                .arg(
                    Arg::with_name("short-shas")
                        .long("short")
                        .short("-s")
                        .required(false)
                        .takes_value(false),
                )
                .arg(
                    Arg::with_name("remove-dependencies")
                        .short("-r")
                        .required(false)
                        .takes_value(false),
                )
                .arg(
                    Arg::with_name("add-exec-prop")
                        .long("add-exec-prop")
                        .required(false)
                        .takes_value(true)
                        .multiple(true)
                        .number_of_values(1),
                )
                .arg(
                    Arg::with_name("add-sh-prop")
                        .long("add-sh-prop")
                        .required(false)
                        .takes_value(true)
                        .multiple(true)
                        .number_of_values(1),
                ),
        )
        .subcommand(
            SubCommand::with_name("gen-dockerignore")
                .about("Generate .dockerignore file")
                .arg(
                    Arg::with_name("directory")
                        .short("d")
                        .required(false)
                        .default_value("."),
                )
                .arg(
                    Arg::with_name("overwrite")
                        .short("f")
                        .required(false)
                        .takes_value(false),
                )
                .arg(
                    Arg::with_name("no-include-ignore")
                        .short("n")
                        .required(false)
                        .takes_value(false),
                )
                .arg(Arg::with_name("component").required(true).index(1)),
        )
        .get_matches();
    if let Some(m) = matches.subcommand_matches("hash-components") {
        let mut reg = CommandRegistry::new();
        let p: &Path = m.value_of_os("directory").unwrap().as_ref();
        let path = p.canonicalize()?;
        let short = m.is_present("short-shas");
        if let Some(cmds) = m.values_of("add-exec-prop") {
            register_added_props(&mut reg, cmds, false)?;
        }
        if let Some(cmds) = m.values_of("add-sh-prop") {
            register_added_props(&mut reg, cmds, true)?;
        }
        run_hasher(
            &path,
            m.is_present("pretty-print"),
            m.is_present("remove-dependencies"),
            short,
            |mut c| annotate_component(&reg, &mut c),
        )
    } else if let Some(m) = matches.subcommand_matches("gen-dockerignore") {
        let p: &Path = m.value_of_os("directory").unwrap().as_ref();
        let path = p.canonicalize()?;
        let d = m.value_of("component").unwrap();
        let overwrite = m.is_present("overwrite");
        let noinclude = m.is_present("no-include-ignore");
        run_dockerignore_creator(&path, d, overwrite, noinclude)
    } else {
        panic!("unexpected subcommand")
    }
}

fn register_added_props<'a, A: Iterator<Item = &'a str>>(
    reg: &mut CommandRegistry,
    props: A,
    is_shell: bool,
) -> anyhow::Result<()> {
    for cmd in props {
        if let Some(p) = cmd.find('=') {
            let mut x = &cmd[..p];
            let is_bool = if x.ends_with("?") {
                x = &x[..p - 1];
                true
            } else {
                false
            };
            let y = &cmd[p + 1..];
            reg.add_command(x, y, is_shell, is_bool)?;
        } else {
            panic!("Invalid argument format {:?}, requires an '='", cmd);
        }
    }
    Ok(())
}
