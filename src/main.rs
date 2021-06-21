use clap::{App, AppSettings, Arg, SubCommand};
use std::path::Path;

mod dockerignore;
mod executor;
mod hasher;
mod types;
use dockerignore::*;
use executor::{annotate_component, CommandConfig, CommandRegistry};
use hasher::*;
use types::CustomError;

enum Deps {
    Dependencies,
    Dependents,
}

fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    let version = env!("CARGO_PKG_VERSION");
    let matches = App::new("Build Helper")
        .version(version)
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
                )
                .arg(
                    Arg::with_name("add-prop")
                        .long("add-prop")
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
        .subcommand(
            SubCommand::with_name("toposort")
                .about("Topologically sort components")
                .arg(
                    Arg::with_name("directory")
                        .required(false)
                        .index(1)
                        .default_value("."),
                ),
        )
        .subcommand(
            SubCommand::with_name("transitive-dependencies")
                .about("List all transitive dependencies of component (topologically sorted)")
                .arg(
                    Arg::with_name("directory")
                        .short("d")
                        .required(false)
                        .default_value("."),
                )
                .arg(
                    Arg::with_name("no-include-self")
                        .short("n")
                        .required(false)
                        .takes_value(false),
                )
                .arg(
                    Arg::with_name("reverse-topological-order")
                        .short("r")
                        .required(false)
                        .takes_value(false),
                )
                .arg(
                    Arg::with_name("component")
                        .required(true)
                        .index(1)
                        .multiple(true),
                ),
        )
        .subcommand(
            SubCommand::with_name("transitive-dependents")
                .about("List all transitive dependents of component (topologically sorted)")
                .arg(
                    Arg::with_name("directory")
                        .short("d")
                        .required(false)
                        .default_value("."),
                )
                .arg(
                    Arg::with_name("no-include-self")
                        .short("n")
                        .required(false)
                        .takes_value(false),
                )
                .arg(
                    Arg::with_name("component")
                        .required(true)
                        .index(1)
                        .multiple(true),
                ),
        )
        .get_matches();
    if let Some(m) = matches.subcommand_matches("hash-components") {
        let mut reg = CommandRegistry::new();
        let p: &Path = m.value_of_os("directory").unwrap().as_ref();
        let path = p.canonicalize()?;
        let short = m.is_present("short-shas");
        if let Some(cmds) = m.values_of("add-exec-prop") {
            register_added_props(&mut reg, cmds, CommandConfig::new_exec_command())?;
        }
        if let Some(cmds) = m.values_of("add-sh-prop") {
            register_added_props(&mut reg, cmds, CommandConfig::new_shell_command())?;
        }
        if let Some(cmds) = m.values_of("add-prop") {
            register_added_props(&mut reg, cmds, CommandConfig::new_template())?;
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
    } else if let Some(m) = matches.subcommand_matches("toposort") {
        let p: &Path = m.value_of_os("directory").unwrap().as_ref();
        let path = p.canonicalize()?;
        run_topo(&path)
    } else if let Some(m) = matches.subcommand_matches("transitive-dependencies") {
        let p: &Path = m.value_of_os("directory").unwrap().as_ref();
        let path = p.canonicalize()?;
        let components: Vec<_> = m.values_of("component").unwrap().collect();
        let noinclude = m.is_present("no-include-self");
        let reverse = m.is_present("reverse-topological-order");
        run_listdeps(
            &path,
            Deps::Dependencies,
            !noinclude,
            Some(reverse),
            components,
        )
    } else if let Some(m) = matches.subcommand_matches("transitive-dependents") {
        let p: &Path = m.value_of_os("directory").unwrap().as_ref();
        let path = p.canonicalize()?;
        let components: Vec<_> = m.values_of("component").unwrap().collect();
        let noinclude = m.is_present("no-include-self");
        run_listdeps(&path, Deps::Dependents, !noinclude, None, components)
    } else {
        panic!("unexpected subcommand")
    }
}

fn register_added_props<A: Iterator<Item = T>, T: AsRef<str>>(
    reg: &mut CommandRegistry,
    props: A,
    config: CommandConfig,
) -> Result<(), CustomError> {
    for cmd_ref in props {
        let cmd = cmd_ref.as_ref();
        if let Some(p) = cmd.find('=') {
            let mut x = &cmd[..p];
            let is_command = config.is_command();
            let conf = if is_command && x.ends_with('?') {
                x = &x[..p - 1];
                config.set_bool()
            } else {
                config
            };
            let y = &cmd[p + 1..];
            reg.add_command(x, y, conf)?;
        } else {
            return Err(CustomError::PropMissingEqualsError {
                argument: cmd.to_owned(),
            });
        }
    }
    Ok(())
}

fn run_topo(path: &Path) -> anyhow::Result<()> {
    let x = types::load_components(path);
    for component in types::toposort_components(x)?.iter() {
        println!("{}", component.dir);
    }
    Ok(())
}

fn run_listdeps(
    path: &Path,
    deps: Deps,
    include_self: bool,
    reverse_order: Option<bool>,
    components: Vec<&str>,
) -> anyhow::Result<()> {
    let data = types::load_components(&path);
    let r = match deps {
        Deps::Dependencies => types::transitive_dependencies(
            data,
            &components[..],
            include_self,
            reverse_order.unwrap_or(false),
        )?,
        Deps::Dependents => types::transitive_dependents(data, &components[..], include_self)?,
    };
    for component in r.iter() {
        println!("{}", component.dir);
    }
    Ok(())
}
