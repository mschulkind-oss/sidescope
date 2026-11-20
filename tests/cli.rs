use std::process::Command;

#[test]
fn invalid_local_config_fails_before_opening_a_window() {
    let root = std::env::temp_dir().join(format!("stripe-cli-config-test-{}", std::process::id()));
    let config_dir = root.join("sidescope");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.toml"), "enabled = ['']").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .env("XDG_CONFIG_HOME", &root)
        .args(["--report", "tests/fixtures/usage.json"])
        .output()
        .expect("launch binary");
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("must not be empty"));
}

#[test]
fn missing_report_fails_instead_of_silently_showing_sample_data() {
    let output = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .args(["--report", "/definitely/no/such/usage-report.json"])
        .output()
        .expect("launch binary");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read report"));
}

#[test]
fn malformed_report_fails_before_opening_a_window() {
    let root = std::env::temp_dir().join(format!("stripe-cli-report-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let report = root.join("report.json");
    std::fs::write(&report, "not json").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .env("XDG_CONFIG_HOME", &root)
        .arg("--report")
        .arg(&report)
        .output()
        .expect("launch binary");
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot read usage report"));
}

#[test]
fn help_and_version_and_doctor_and_unknown_flags() {
    let help = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .arg("--help")
        .output()
        .expect("run --help");
    assert!(help.status.success());
    let help_out = String::from_utf8_lossy(&help.stdout);
    assert!(help_out.contains("config.toml"));
    assert!(help_out.contains("sidescope doctor"));

    let version = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .arg("--version")
        .output()
        .expect("run --version");
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("sidescope "));

    let root = std::env::temp_dir().join(format!("sidescope-cli-doctor-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let doctor = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .env("XDG_CONFIG_HOME", &root)
        .arg("doctor")
        .output()
        .expect("run doctor");
    std::fs::remove_dir_all(root).unwrap();
    assert!(doctor.status.success());
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("Sidescope doctor:"));

    let bogus = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .arg("--definitely-unknown-flag")
        .output()
        .expect("run unknown flag");
    assert_eq!(bogus.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bogus.stderr).contains("unknown argument"));
}

fn config_root(name: &str, body: &str) -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sidescope")).unwrap();
    std::fs::write(root.join("sidescope/config.toml"), body).unwrap();
    root
}

const CHART_CONFIG: &str = r#"
[[chart_sources]]
id = "example-metrics"
command = ["example-chart-data", "--from", "{from}", "--to", "{to}", "--zone", "{zone}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"

[[chart_cards]]
id = "example-card"
source = "example-metrics"
kind = "time-series"
metric = "spend"
"#;

#[test]
fn invalid_chart_config_fails_before_opening_a_window_and_names_the_key() {
    let root = config_root(
        "cli-chart-invalid",
        &CHART_CONFIG.replace("{zone}", "{when}"),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .env("XDG_CONFIG_HOME", &root)
        .args(["--report", "tests/fixtures/usage.json"])
        .output()
        .expect("launch binary");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("command"), "{stderr}");
    assert!(stderr.contains("braces"), "{stderr}");
}

#[test]
fn help_documents_chart_tables_and_their_authority() {
    let help = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .arg("--help")
        .output()
        .expect("run --help");
    let text = String::from_utf8_lossy(&help.stdout);
    for needle in [
        "[[chart_sources]]",
        "[[chart_cards]]",
        "{from}",
        "sidescope-chart-v1",
        "columns-v1",
        "time-series",
        "ranked-bars",
        "this app's authority",
    ] {
        assert!(text.contains(needle), "help is missing {needle}");
    }
}

#[test]
fn doctor_prints_each_chart_command_in_full_with_the_authority_notice() {
    let root = config_root(
        "cli-chart-doctor",
        &CHART_CONFIG.replace("{zone}", "{timezone}"),
    );
    let doctor = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .env("XDG_CONFIG_HOME", &root)
        .arg("doctor")
        .output()
        .expect("run doctor");
    let _ = std::fs::remove_dir_all(&root);
    let text = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        text.contains(
            "example-metrics  example-chart-data --from {from} --to {to} --zone {timezone}"
        ),
        "{text}"
    );
    assert!(
        text.contains("commands run with this app's authority"),
        "{text}"
    );
}

#[test]
fn doctor_without_chart_sources_prints_no_chart_section() {
    let root = config_root("cli-no-sources", "");
    let doctor = Command::new(env!("CARGO_BIN_EXE_sidescope"))
        .env("XDG_CONFIG_HOME", &root)
        .arg("doctor")
        .output()
        .expect("run doctor");
    let _ = std::fs::remove_dir_all(&root);
    let text = String::from_utf8_lossy(&doctor.stdout);
    assert!(!text.contains("chart"), "{text}");
}
