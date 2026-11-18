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
