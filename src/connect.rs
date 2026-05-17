//! Builds the OS process that connects to a host.
//!
//! Resolution order:
//!   1. `host.cmd` — raw shell command, executed via `sh -c`.
//!   2. `host.command` — per-host structured override.
//!   3. `defaults.command` from the global config (default: `ssh`).
//!
//! Template placeholders supported in args:
//!   `{user}`, `{host}`, `{ip}`, `{port}`, `{name}`.

use std::{env, process::Command};

use crate::config::{Config, ConnectCommand, Host};

pub fn build_command(config: &Config, host: &Host) -> Command {
    if let Some(cmd) = host.cmd.as_deref() {
        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);
        return command;
    }

    let template = host.command.as_ref().unwrap_or(&config.defaults.command);

    let user = host
        .user
        .clone()
        .or_else(|| config.defaults.user.clone())
        .or_else(|| env::var("USER").ok())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("root"));

    let port = host.port.unwrap_or(config.defaults.port);

    let mut command = Command::new(template.program());

    if let Some(args) = template.args() {
        for raw in args {
            command.arg(expand(raw, &user, host, port));
        }
    } else {
        // Sensible default args for bare programs.
        match template.program() {
            "ssh" | "autossh" => {
                command.arg("-p").arg(port.to_string());
                command.arg(format!("{user}@{}", host.ip));
            }
            "mosh" => {
                if port != 22 {
                    command.arg("--ssh").arg(format!("ssh -p {port}"));
                }
                command.arg(format!("{user}@{}", host.ip));
            }
            "telnet" => {
                command.arg(&host.ip);
                command.arg(port.to_string());
            }
            _ => {
                command.arg(format!("{user}@{}", host.ip));
            }
        }
    }

    command
}

fn expand(raw: &str, user: &str, host: &Host, port: u16) -> String {
    raw.replace("{user}", user)
        .replace("{host}", &host.ip)
        .replace("{ip}", &host.ip)
        .replace("{port}", &port.to_string())
        .replace("{name}", &host.name)
}

pub fn describe(config: &Config, host: &Host) -> String {
    if let Some(cmd) = host.cmd.as_deref() {
        return cmd.to_string();
    }
    let template = host.command.as_ref().unwrap_or(&config.defaults.command);
    let program = template.program();
    let user = host
        .user
        .clone()
        .or_else(|| config.defaults.user.clone())
        .unwrap_or_else(|| "$USER".into());
    let port = host.port.unwrap_or(config.defaults.port);

    match program {
        "ssh" | "autossh" => format!("{program} {user}@{}:{port}", host.ip),
        "mosh" => format!("mosh {user}@{}", host.ip),
        "telnet" => format!("telnet {} {port}", host.ip),
        other => format!("{other} {user}@{}", host.ip),
    }
}

#[allow(dead_code)]
pub fn supports_template(template: &ConnectCommand) -> bool {
    template.args().is_some()
        || matches!(
            template.program(),
            "ssh" | "autossh" | "mosh" | "telnet" | "rsh" | "et"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Category;

    fn make_config(default: ConnectCommand) -> Config {
        Config {
            defaults: crate::config::Defaults {
                command: default,
                port: 22,
                user: None,
            },
            sync: None,
            categories: vec![Category {
                name: "ops".into(),
                icon: None,
                hosts: vec![],
            }],
        }
    }

    fn host() -> Host {
        Host {
            name: "edge".into(),
            ip: "10.0.0.1".into(),
            user: Some("alice".into()),
            port: Some(2200),
            cmd: None,
            command: None,
        }
    }

    #[test]
    fn raw_cmd_wins() {
        let cfg = make_config(ConnectCommand::default());
        let mut h = host();
        h.cmd = Some("echo hi".into());
        let cmd = build_command(&cfg, &h);
        assert_eq!(cmd.get_program(), "sh");
    }

    #[test]
    fn ssh_default_uses_port_and_user() {
        let cfg = make_config(ConnectCommand::default());
        let cmd = build_command(&cfg, &host());
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(cmd.get_program(), "ssh");
        assert_eq!(args, vec!["-p", "2200", "alice@10.0.0.1"]);
    }

    #[test]
    fn mosh_program_emits_user_at_host() {
        let cfg = make_config(ConnectCommand::Program("mosh".into()));
        let mut h = host();
        h.port = None;
        let cmd = build_command(&cfg, &h);
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(cmd.get_program(), "mosh");
        assert_eq!(args, vec!["alice@10.0.0.1"]);
    }

    #[test]
    fn template_expands_placeholders() {
        let cfg = make_config(ConnectCommand::Template {
            program: "kitty".into(),
            args: vec![
                "+kitten".into(),
                "ssh".into(),
                "-p".into(),
                "{port}".into(),
                "{user}@{host}".into(),
            ],
        });
        let cmd = build_command(&cfg, &host());
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(cmd.get_program(), "kitty");
        assert_eq!(args, vec!["+kitten", "ssh", "-p", "2200", "alice@10.0.0.1"]);
    }
}
