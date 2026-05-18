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
    let explicit_user = effective_user(config, host);
    let port = host.port.unwrap_or(config.defaults.port);

    let mut command = Command::new(template.program());

    if let Some(args) = template.args() {
        // Templates opt into a `{user}` placeholder explicitly — keep the
        // legacy fallback (env $USER, then "root") so the rendered command
        // is never empty where the template asked for a user.
        let user_for_template = explicit_user
            .clone()
            .or_else(|| env::var("USER").ok())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| String::from("root"));
        for raw in args {
            command.arg(expand(raw, &user_for_template, host, port));
        }
    } else {
        // Default arg recipes. Only `ssh` injects a `user@` prefix from
        // the host/defaults — every other program either has its own
        // user-resolution (mosh, autossh via ssh_config, kitty +kitten,
        // …) or doesn't accept a user at all (telnet).
        let program = template.program();
        let ssh_user = if program == "ssh" {
            explicit_user.as_deref()
        } else {
            None
        };
        match program {
            "ssh" => {
                command.arg("-p").arg(port.to_string());
                command.arg(host_arg(ssh_user, &host.ip));
            }
            "autossh" => {
                command.arg("-p").arg(port.to_string());
                command.arg(&host.ip);
            }
            "mosh" => {
                if port != 22 {
                    command.arg("--ssh").arg(format!("ssh -p {port}"));
                }
                command.arg(&host.ip);
            }
            "telnet" => {
                command.arg(&host.ip);
                command.arg(port.to_string());
            }
            _ => {
                command.arg(&host.ip);
            }
        }
    }

    command
}

/// Resolve the connection user from host → defaults. Returns `None` when
/// the user is deliberately unset so callers can drop the `user@` prefix
/// and defer to ssh_config / the program's own logic.
pub fn effective_user(config: &Config, host: &Host) -> Option<String> {
    host.user
        .clone()
        .or_else(|| config.defaults.user.clone())
        .filter(|value| !value.is_empty())
}

fn host_arg(user: Option<&str>, ip: &str) -> String {
    match user {
        Some(u) => format!("{u}@{ip}"),
        None => ip.to_string(),
    }
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
    let port = host.port.unwrap_or(config.defaults.port);

    // Only `ssh` shows a user@ prefix — other programs read the user from
    // their own configuration (or don't accept one).
    let ssh_user = if program == "ssh" {
        effective_user(config, host)
    } else {
        None
    };
    let target = host_arg(ssh_user.as_deref(), &host.ip);

    match program {
        "ssh" => format!("ssh {target}:{port}"),
        "autossh" => format!("autossh {}:{port}", host.ip),
        "mosh" => format!("mosh {}", host.ip),
        "telnet" => format!("telnet {} {port}", host.ip),
        other => format!("{other} {}", host.ip),
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
                terminal_command: None,
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
    fn mosh_program_never_embeds_user_prefix() {
        let cfg = make_config(ConnectCommand::Program("mosh".into()));
        let mut h = host();
        h.port = None;
        // Even with host.user set, mosh defers to its own user resolution.
        let cmd = build_command(&cfg, &h);
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(cmd.get_program(), "mosh");
        assert_eq!(args, vec!["10.0.0.1"]);
    }

    #[test]
    fn autossh_omits_user_prefix_even_when_user_set() {
        let cfg = make_config(ConnectCommand::Program("autossh".into()));
        let cmd = build_command(&cfg, &host());
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(cmd.get_program(), "autossh");
        assert_eq!(args, vec!["-p", "2200", "10.0.0.1"]);
    }

    #[test]
    fn custom_program_omits_user_prefix() {
        let cfg = make_config(ConnectCommand::Program("kitty".into()));
        let cmd = build_command(&cfg, &host());
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(cmd.get_program(), "kitty");
        assert_eq!(args, vec!["10.0.0.1"]);
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

    #[test]
    fn ssh_omits_user_prefix_when_no_user_configured() {
        let cfg = make_config(ConnectCommand::default());
        let mut h = host();
        h.user = None;
        let cmd = build_command(&cfg, &h);
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(cmd.get_program(), "ssh");
        assert_eq!(args, vec!["-p", "2200", "10.0.0.1"]);
    }

    #[test]
    fn ssh_uses_defaults_user_when_host_omits_one() {
        let mut cfg = make_config(ConnectCommand::default());
        cfg.defaults.user = Some("ops".into());
        let mut h = host();
        h.user = None;
        let cmd = build_command(&cfg, &h);
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(args, vec!["-p", "2200", "ops@10.0.0.1"]);
    }

    #[test]
    fn mosh_omits_user_prefix_when_no_user_configured() {
        let cfg = make_config(ConnectCommand::Program("mosh".into()));
        let mut h = host();
        h.user = None;
        h.port = None;
        let cmd = build_command(&cfg, &h);
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(cmd.get_program(), "mosh");
        assert_eq!(args, vec!["10.0.0.1"]);
    }

    #[test]
    fn describe_omits_user_when_no_user_configured() {
        let cfg = make_config(ConnectCommand::default());
        let mut h = host();
        h.user = None;
        assert_eq!(describe(&cfg, &h), "ssh 10.0.0.1:2200");
    }

    #[test]
    fn describe_mosh_drops_user_even_when_configured() {
        let cfg = make_config(ConnectCommand::Program("mosh".into()));
        assert_eq!(describe(&cfg, &host()), "mosh 10.0.0.1");
    }

    #[test]
    fn describe_custom_program_drops_user_even_when_configured() {
        let cfg = make_config(ConnectCommand::Program("kitty".into()));
        assert_eq!(describe(&cfg, &host()), "kitty 10.0.0.1");
    }

    #[test]
    fn describe_shows_user_when_configured() {
        let cfg = make_config(ConnectCommand::default());
        assert_eq!(describe(&cfg, &host()), "ssh alice@10.0.0.1:2200");
    }

    #[test]
    fn effective_user_treats_empty_string_as_unset() {
        let cfg = make_config(ConnectCommand::default());
        let mut h = host();
        h.user = Some(String::new());
        assert_eq!(effective_user(&cfg, &h), None);
    }

    #[test]
    fn host_empty_user_overrides_defaults_user() {
        let mut cfg = make_config(ConnectCommand::default());
        cfg.defaults.user = Some("ops".into());
        let mut h = host();
        h.user = Some(String::new());
        assert_eq!(effective_user(&cfg, &h), None);
        let cmd = build_command(&cfg, &h);
        let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(args, vec!["-p", "2200", "10.0.0.1"]);
    }
}
