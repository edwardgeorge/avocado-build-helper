use handlebars::Handlebars;
use regex::Regex;
use serde_json::Value;
use shell_words::split;
use std::process::{Command, Stdio};
use std::vec::Vec;

use crate::types::Component;

pub struct CommandRegistry<'a> {
    commands: Vec<String>,
    handlebars: Handlebars<'a>,
}

impl<'a> CommandRegistry<'a> {
    pub fn new() -> Self {
        let reg = Handlebars::new();
        CommandRegistry {
            commands: Vec::new(),
            handlebars: reg,
        }
    }

    pub fn add_command(&mut self, name: &str, command: &str) -> anyhow::Result<()> {
        self.commands.push(name.to_owned());
        Ok(self.handlebars.register_template_string(name, command)?)
    }

    pub fn run_command(&self, name: &str, data: &Component) -> anyhow::Result<String> {
        let cmd = self.handlebars.render(name, data)?;
        let args = split(&cmd)?;
        let out = Command::new(&args[0])
            .args(&args[1..])
            .envs(component_to_envs("AVOCADO_", data)?)
            .stderr(Stdio::inherit())
            .output()?;
        if !out.status.success() {
            panic!(
                "Command {:?} was not successful: {:?}",
                cmd,
                out.status.code()
            );
        }
        Ok(std::str::from_utf8(&out.stdout)?.trim().to_owned())
    }

    pub fn run_all(&self, data: &Component) -> anyhow::Result<Vec<(String, String)>> {
        self.commands
            .iter()
            .map(|c| self.run_command(c, data).map(|v| (c.clone(), v)))
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
        .filter_map(|(k, v)| v.as_str().map(|s| (key_to_env_var(k, prefix), s.to_owned())))
        .collect();
    Ok(x)
}

fn key_to_env_var(key: &str, prefix: &str) -> String {
    let regex = Regex::new(r"[^a-zA-Z0-9_]+").unwrap();
    [prefix, &regex.replace_all(key, "_")].concat().to_uppercase()
}
