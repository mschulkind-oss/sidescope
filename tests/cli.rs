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
