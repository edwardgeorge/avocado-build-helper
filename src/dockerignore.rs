use std::fs::read_to_string;
use std::io::Write;
use std::path::Path;

use crate::types::*;

pub fn run_dockerignore_creator(
    path: &Path,
    dir: &str,
    write_to_file: bool,
    no_include_ignore: bool,
) -> Result<(), anyhow::Error> {
    let dockerignore_path = path.join(".dockerignore");
    let x = transitive_dependencies(load_components(path), dir.to_owned(), true)?;
    let contents =
        if !no_include_ignore && dockerignore_path.exists() && dockerignore_path.is_file() {
            Some(read_to_string(&dockerignore_path)?)
        } else {
            None
        };
    let mut output: Box<dyn Write> = if write_to_file {
        Box::new(
            std::fs::OpenOptions::new()
                .write(true)
                .open(&dockerignore_path)?,
        )
    } else {
        Box::new(std::io::stdout())
    };
    output.write("*\n".as_ref())?;
    for i in x.iter() {
        output.write(format!("!{}/**\n", i.dir).as_ref())?;
    }
    if let Some(d) = contents {
        output.write(d.as_ref())?;
    }
    Ok(())
}
