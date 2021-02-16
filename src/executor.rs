use handlebars::Handlebars;
use regex::Regex;
use serde_json::Value;
use shell_words::split;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::vec::Vec;

use crate::types::{Component, CustomError};

#[derive(Debug)]
struct CommandConfig {
    is_shell: bool,
    is_bool: bool,
}

pub struct CommandRegistry<'a> {
    commands: Vec<String>,
    is_shell_map: HashMap<String, CommandConfig>,
    handlebars: Handlebars<'a>,
}

fn new_shell_command(cmd: &str) -> Command {
    let mut com = Command::new("sh");
    com.arg("-xc").arg(cmd);
    com
}

fn new_command(cmd: &str) -> Result<Command, CustomError> {
    let args = split(&cmd).map_err(|e| CustomError::CommandParseError {
        cmd: cmd.to_owned(),
        error: e,
    })?;
    let mut com = Command::new(&args[0]);
    com.args(&args[1..]);
    Ok(com)
}

impl<'a> CommandRegistry<'a> {
    pub fn new() -> Self {
        let reg = Handlebars::new();
        CommandRegistry {
            commands: Vec::new(),
            is_shell_map: HashMap::new(),
            handlebars: reg,
        }
    }

    pub fn add_command(
        &mut self,
        name: &str,
        command: &str,
        is_shell_command: bool,
        is_bool_result: bool,
    ) -> Result<(), CustomError> {
        if self.is_shell_map.contains_key(name) {
            return Err(CustomError::DuplicatePropertyNameError {
                name: name.to_owned(),
            });
        }
        self.commands.push(name.to_owned());
        let config = CommandConfig {
            is_shell: is_shell_command,
            is_bool: is_bool_result,
        };
        self.is_shell_map.insert(name.to_owned(), config);
        self.handlebars
            .register_template_string(name, command)
            .map_err(|e| CustomError::TemplateError {
                prop_name: name.to_owned(),
                error: e,
            })
    }

    pub fn run_command(
        &self,
        name: &str,
        data: &Component,
        is_shell_command: bool,
        is_bool_result: bool,
    ) -> anyhow::Result<String> {
        let cmd =
            self.handlebars
                .render(name, data)
                .map_err(|e| CustomError::TemplateRenderError {
                    cmd_name: name.to_owned(),
                    error: e,
                })?;
        let mut com = if is_shell_command {
            new_shell_command(&cmd)
        } else {
            new_command(&cmd)?
        };
        let out = com
            .envs(component_to_envs("AVOCADO_", data)?)
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| CustomError::CommandExecutionError {
                cmd_name: name.to_owned(),
                error: e,
            })?;
        if is_bool_result {
            return Ok(match out.status.success() {
                true => "true",
                false => "false",
            }
            .to_owned());
        }
        if !out.status.success() {
            anyhow::bail!(CustomError::UnsuccessfulCommandError {
                cmd: cmd.to_owned(),
                reason: match out.status.code() {
                    Some(c) => format!("exit code {}", c),
                    None => format!("terminated by signal"),
                },
            })
        } else {
            Ok(std::str::from_utf8(&out.stdout)?.trim().to_owned())
        }
    }

    pub fn run_all(&self, data: &Component) -> anyhow::Result<Vec<(String, String)>> {
        self.commands
            .iter()
            .map(|c| {
                let config = self.is_shell_map.get(c).unwrap();
                self.run_command(c, data, config.is_shell, config.is_bool)
                    .map(|v| (c.clone(), v))
            })
            .collect()
    }
}

pub fn annotate_component(reg: &CommandRegistry, component: &mut Component) -> anyhow::Result<()> {
    let mut cres = reg.run_all(component)?;
    let m = component.rem.as_object_mut().unwrap();
    for (k, v) in cres.drain(..) {
        m.insert(k, Value::from(v));
    }
    Ok(())
}

fn component_to_envs(prefix: &str, component: &Component) -> anyhow::Result<Vec<(String, String)>> {
    let v = serde_json::to_value(component)?;
    let x = v
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(k, v)| {
            v.as_str()
                .map(|s| (key_to_env_var(k, prefix), s.to_owned()))
        })
        .collect();
    Ok(x)
}

fn key_to_env_var(key: &str, prefix: &str) -> String {
    let regex = Regex::new(r"[^a-zA-Z0-9_]+").unwrap();
    [prefix, &regex.replace_all(key, "_")]
        .concat()
        .to_uppercase()
}
