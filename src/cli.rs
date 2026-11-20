use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchMode {
    Live,
    Demo,
    Report(String),
    BrowserMock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run { mode: LaunchMode },
    Help,
    Version,
    Doctor,
}

pub fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut iter = args.into_iter();
    let _program = iter.next();
    let Some(first) = iter.next() else {
        return Ok(Command::Run {
            mode: LaunchMode::Live,
        });
    };

    let cmd = match first.as_str() {
        "-h" | "--help" => Command::Help,
        "-V" | "--version" => Command::Version,
        "doctor" | "--doctor" => Command::Doctor,
        "--live" => Command::Run {
            mode: LaunchMode::Live,
        },
        "--demo" | "--sample" => Command::Run {
            mode: LaunchMode::Demo,
        },
        "--browser" => Command::Run {
            mode: LaunchMode::BrowserMock,
        },
        "--report" => {
            let Some(path) = iter.next() else {
                return Err("--report requires a file path".into());
            };
            Command::Run {
                mode: LaunchMode::Report(path),
            }
        }
        other => {
            return Err(format!(
                "unknown argument: {other}\nRun `sidescope --help` for usage and config format."
            ));
        }
    };

    if let Some(extra) = iter.next() {
        return Err(format!(
            "unexpected argument: {extra}\nRun `sidescope --help` for usage."
        ));
    }

    Ok(cmd)
}

pub fn help_text() -> String {
    let cfg = crate::config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "~/.config/sidescope/config.toml".into());
    format!(
        "\
Sidescope — native Wayland status column for system telemetry, AI usage, and weather

USAGE:
  sidescope                 Launch the dashboard
  sidescope doctor          Check runtime environment, config file, and optional tools
  sidescope --version       Print version and build revision
  sidescope --help          Print this help and configuration reference

CONFIGURATION:
  Path: {cfg}
  Override base directory with $XDG_CONFIG_HOME (uses $XDG_CONFIG_HOME/sidescope/config.toml).
  Optional TOML file; omit to use defaults. Unknown keys or invalid values fail startup.

  Top-level keys:
    enabled = [\"openrouter\", \"deepseek\", \"openai\", \"anthropic\", \"antigravity\", \"muse\"]
                            Optional AI provider filter/order (\"enabled = []\" hides AI cards)
    show_agy_gpt_oss = false
                            Include Antigravity's separate Claude & GPT OSS pool
    theme = \"original\"      Optional theme id (e.g. \"original\", \"paper\", \"terminal\")
    density = \"screen-fit\"  \"screen-fit\" (default) or \"comfortable\"

  [sensors]                 Optional hardware sensor override (defaults to labeled CPU/GPU/fans)
    channels = [
      {{ chip = \"coretemp\", device = \"/sys/devices/platform/coretemp.0\", channel = \"temp1\", label = \"CPU package\" }}
    ]

  [weather]                 Optional explicit location (no IP/GPS lookup; requires curl)
    latitude = 0.0
    longitude = 0.0
    place = \"City name\"
    units = \"celsius\"       \"celsius\" (default) or \"fahrenheit\"

KEYBOARD SHORTCUTS (in window):
  ?        Toggle shortcut overlay
  R        Refresh live report
  D        Toggle density (screen-fit / comfortable)
  [ / ]    Cycle visual themes
  J / K    Scroll column"
    )
}

pub fn find_in_path(binary: &str) -> Option<std::path::PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn run_doctor() -> (String, bool) {
    let mut lines = vec!["Sidescope doctor:".to_string()];
    let mut all_ok = true;

    match crate::config_path() {
        Ok(path) => {
            if path.exists() {
                match crate::load_config_at(&path) {
                    Ok(_) => lines.push(format!(
                        "  ok       config        {} (valid)",
                        path.display()
                    )),
                    Err(err) => {
                        all_ok = false;
                        lines.push(format!(
                            "  ERROR    config        {} ({err})",
                            path.display()
                        ));
                    }
                }
            } else {
                lines.push(format!(
                    "  ok       config        {} (not present; using defaults)",
                    path.display()
                ));
            }
        }
        Err(err) => {
            all_ok = false;
            lines.push(format!("  ERROR    config        {err}"));
        }
    }

    match std::env::var("WAYLAND_DISPLAY") {
        Ok(val) if !val.is_empty() => {
            lines.push(format!("  ok       wayland       WAYLAND_DISPLAY={val}"));
        }
        _ => {
            lines.push(
                "  warn     wayland       WAYLAND_DISPLAY is unset (needed to open a Wayland window)"
                    .into(),
            );
        }
    }

    let proc_ok = Path::new("/proc/stat").is_file() && Path::new("/proc/meminfo").is_file();
    if proc_ok {
        lines.push("  ok       procfs        /proc/stat and /proc/meminfo readable".into());
    } else {
        all_ok = false;
        lines.push("  ERROR    procfs        /proc/stat or /proc/meminfo missing".into());
    }

    if Path::new("/sys/class/hwmon").is_dir() {
        lines.push("  ok       hwmon         /sys/class/hwmon present".into());
    } else {
        lines.push(
            "  warn     hwmon         /sys/class/hwmon absent (no hardware sensor cards)".into(),
        );
    }

    match find_in_path("ai-usagebar") {
        Some(p) => lines.push(format!("  ok       ai-usagebar   {}", p.display())),
        None => lines.push(
            "  warn     ai-usagebar   not on PATH (optional; needed for AI quota/balance cards)"
                .into(),
        ),
    }

    match find_in_path("curl") {
        Some(p) => lines.push(format!("  ok       curl          {}", p.display())),
        None => lines.push(
            "  warn     curl          not on PATH (optional; needed when [weather] is configured)"
                .into(),
        ),
    }

    (lines.join("\n"), all_ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn bare_invocation_defaults_to_live_mode() {
        assert_eq!(
            parse_args(args(&["sidescope"])).unwrap(),
            Command::Run {
                mode: LaunchMode::Live
            }
        );
    }

    #[test]
    fn flags_parse_for_demo_report_help_version_and_doctor() {
        assert_eq!(
            parse_args(args(&["sidescope", "--demo"])).unwrap(),
            Command::Run {
                mode: LaunchMode::Demo
            }
        );
        assert_eq!(
            parse_args(args(&["sidescope", "--live"])).unwrap(),
            Command::Run {
                mode: LaunchMode::Live
            }
        );
        assert_eq!(
            parse_args(args(&["sidescope", "--report", "foo.json"])).unwrap(),
            Command::Run {
                mode: LaunchMode::Report("foo.json".into())
            }
        );
        assert_eq!(
            parse_args(args(&["sidescope", "--help"])).unwrap(),
            Command::Help
        );
        assert_eq!(
            parse_args(args(&["sidescope", "-h"])).unwrap(),
            Command::Help
        );
        assert_eq!(
            parse_args(args(&["sidescope", "--version"])).unwrap(),
            Command::Version
        );
        assert_eq!(
            parse_args(args(&["sidescope", "-V"])).unwrap(),
            Command::Version
        );
        assert_eq!(
            parse_args(args(&["sidescope", "doctor"])).unwrap(),
            Command::Doctor
        );
    }

    #[test]
    fn unknown_and_incomplete_flags_return_errors() {
        assert!(parse_args(args(&["sidescope", "--bogus"])).is_err());
        assert!(parse_args(args(&["sidescope", "--report"])).is_err());
        assert!(parse_args(args(&["sidescope", "--demo", "extra"])).is_err());
    }

    #[test]
    fn help_text_documents_config_path_and_schema() {
        let help = help_text();
        assert!(help.contains("config.toml"));
        assert!(help.contains("[weather]"));
        assert!(help.contains("[sensors]"));
        assert!(help.contains("sidescope doctor"));
    }
}
