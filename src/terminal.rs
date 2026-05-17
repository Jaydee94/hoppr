//! Spawns a connection command in a new terminal window.
//!
//! Detection order (when no `terminal_command` is configured):
//!   1. Windows Terminal — `$WT_SESSION` is set
//!   2. iTerm2 — `$TERM_PROGRAM == "iTerm.app"`
//!   3. macOS Terminal — `$TERM_PROGRAM == "Apple_Terminal"`
//!   4. GNOME Terminal — `$GNOME_DESKTOP_SESSION_ID` is set
//!   5. Konsole — `$XDG_CURRENT_DESKTOP` contains "KDE"
//!   6. xterm — last-resort fallback on Linux/BSD

use std::{env, process::Command};

use anyhow::{bail, Result};

#[derive(Debug, Clone)]
enum Backend {
    WindowsTerminal,
    MacOSTerminal,
    ITerm2,
    GnomeTerminal,
    Konsole,
    Xterm,
    Custom(String),
}

pub struct TerminalLauncher {
    backend: Option<Backend>,
}

impl TerminalLauncher {
    /// Detect the available terminal. `config_override` takes precedence.
    pub fn detect(config_override: Option<&str>) -> Self {
        if let Some(cmd) = config_override.filter(|s| !s.is_empty()) {
            return Self {
                backend: Some(Backend::Custom(cmd.to_owned())),
            };
        }
        Self {
            backend: detect_backend(),
        }
    }

    /// Returns `true` when a terminal emulator is available.
    pub fn is_available(&self) -> bool {
        self.backend.is_some()
    }

    /// Spawn `argv` (program + args) in a new terminal window, detached.
    pub fn spawn(&self, argv: &[String]) -> Result<()> {
        let Some(ref backend) = self.backend else {
            bail!(
                "no terminal emulator detected — set defaults.terminal_command in config \
                 (e.g. \"wt\", \"gnome-terminal\", \"xterm\")"
            );
        };

        match backend {
            Backend::WindowsTerminal => {
                Command::new("wt")
                    .arg("new-tab")
                    .arg("--")
                    .args(argv)
                    .spawn()?;
            }
            Backend::MacOSTerminal => {
                let cmd_str = shell_join(argv);
                Command::new("osascript")
                    .arg("-e")
                    .arg(format!(
                        "tell application \"Terminal\" to do script \"{}\"",
                        cmd_str
                    ))
                    .spawn()?;
            }
            Backend::ITerm2 => {
                let cmd_str = shell_join(argv);
                Command::new("osascript")
                    .arg("-e")
                    .arg(format!(
                        "tell application \"iTerm\" to create window with default profile \
                         command \"{}\"",
                        cmd_str
                    ))
                    .spawn()?;
            }
            Backend::GnomeTerminal => {
                Command::new("gnome-terminal")
                    .arg("--")
                    .args(argv)
                    .spawn()?;
            }
            Backend::Konsole => {
                Command::new("konsole").arg("-e").args(argv).spawn()?;
            }
            Backend::Xterm => {
                Command::new("xterm").arg("-e").args(argv).spawn()?;
            }
            Backend::Custom(terminal_cmd) => {
                let parts: Vec<&str> = terminal_cmd.split_whitespace().collect();
                if let Some(prog) = parts.first() {
                    Command::new(prog).args(&parts[1..]).args(argv).spawn()?;
                }
            }
        }
        Ok(())
    }
}

fn detect_backend() -> Option<Backend> {
    if env::var("WT_SESSION").is_ok() {
        return Some(Backend::WindowsTerminal);
    }

    if let Ok(term_prog) = env::var("TERM_PROGRAM") {
        if term_prog == "iTerm.app" {
            return Some(Backend::ITerm2);
        }
        if term_prog == "Apple_Terminal" {
            return Some(Backend::MacOSTerminal);
        }
    }

    if env::var("GNOME_DESKTOP_SESSION_ID").is_ok() {
        return Some(Backend::GnomeTerminal);
    }

    if let Ok(desktop) = env::var("XDG_CURRENT_DESKTOP") {
        if desktop.to_uppercase().contains("KDE") {
            return Some(Backend::Konsole);
        }
    }

    if Command::new("xterm").arg("-version").output().is_ok() {
        return Some(Backend::Xterm);
    }

    None
}

/// Join argv into a shell-safe string for embedding in AppleScript quotes.
fn shell_join(argv: &[String]) -> String {
    argv.iter()
        .map(|a| a.replace('\\', "\\\\").replace('"', "\\\""))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_override_is_used() {
        let launcher = TerminalLauncher::detect(Some("myterm"));
        assert!(launcher.is_available());
        assert!(matches!(launcher.backend, Some(Backend::Custom(_))));
    }

    #[test]
    fn empty_override_falls_through_to_detection() {
        let launcher = TerminalLauncher::detect(Some(""));
        // We can't assert is_available since CI has no terminal, but at least it
        // shouldn't panic and should not use Custom.
        assert!(!matches!(launcher.backend, Some(Backend::Custom(_))));
    }

    #[test]
    fn shell_join_escapes_special_chars() {
        let argv = vec!["ssh".to_owned(), "user@10.0.0.1".to_owned()];
        let joined = shell_join(&argv);
        assert_eq!(joined, "ssh user@10.0.0.1");

        let argv2 = vec!["sh".to_owned(), "-c".to_owned(), "echo \"hi\"".to_owned()];
        let joined2 = shell_join(&argv2);
        assert!(joined2.contains("\\\""));
    }
}
