//! End to end without a display: a real configuration file text, the real
//! runtime spawning synthetic commands, the shipped normalizer and the card
//! models. Nothing here reads a real ledger; `fake-ledger.sh` prints synthetic
//! fixtures.

use sidescope::chart::state::{SourceSnapshot, SourceStatus};
use sidescope::chart::time::Timestamp;
use sidescope::chart_mapping::normalize_output;
use sidescope::chart_runtime::ChartRuntime;
use sidescope::chart_view::build_model;
use sidescope::{AppConfig, parse_config};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn profile_with_command(executable: &str) -> AppConfig {
    let text = std::fs::read_to_string(format!("{ROOT}/examples/charts/agent-activity.toml"))
        .unwrap()
        .replace(
            "\"agent-activity\", \"ledger\", \"series\"",
            &format!("\"{executable}\", \"ledger\", \"series\""),
        );
    parse_config(&text).expect("shipped profile stays valid")
}

fn wait_for_answers(runtime: &ChartRuntime, sources: usize) -> Vec<SourceSnapshot> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let snapshots = runtime.snapshots();
        let settled = snapshots
            .iter()
            .filter(|snapshot| snapshot.dataset.is_some() || snapshot.last_error.is_some())
            .count();
        if settled == sources {
            return snapshots;
        }
        assert!(Instant::now() < deadline, "sources never answered");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn counting_wrapper(name: &str) -> (PathBuf, PathBuf) {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("chart-integration-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("calls");
    let wrapper = dir.join("ledger.sh");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nexec '{ROOT}/examples/charts/fake-ledger.sh' \"$@\"\n",
            log.display()
        ),
    )
    .unwrap();
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&wrapper)
        .status()
        .unwrap();
    (wrapper, log)
}

#[test]
fn shipped_profile_runs_end_to_end_through_the_generic_mapper() {
    let (wrapper, log) = counting_wrapper("profile");
    let config = profile_with_command(wrapper.to_str().unwrap());
    let runtime = ChartRuntime::start(&config.chart_sources, normalize_output);
    let snapshots = wait_for_answers(&runtime, config.chart_sources.len());
    for snapshot in &snapshots {
        assert!(
            snapshot.dataset.is_some() && snapshot.last_error.is_none(),
            "{}: {:?}",
            snapshot.source_id,
            snapshot.last_error
        );
        assert_eq!(snapshot.status(Timestamp::now()), SourceStatus::Fresh);
    }
    // One answer per source on first load, however many cards read it.
    let calls = std::fs::read_to_string(&log).unwrap();
    assert_eq!(calls.lines().count(), config.chart_sources.len(), "{calls}");
    let minute = calls
        .lines()
        .filter(|l| l.contains("--bucket minute"))
        .count();
    let day = calls.lines().filter(|l| l.contains("--bucket day")).count();
    assert_eq!((minute, day), (1, config.chart_sources.len() - 1));

    let now = Timestamp::now();
    for card in &config.chart_cards {
        let snapshot = snapshots
            .iter()
            .find(|snapshot| snapshot.source_id == card.source)
            .unwrap();
        let model = build_model(card, snapshot, now);
        assert!(model.headline().is_some(), "{} has no headline", card.id);
        assert!(
            model.banner().is_none(),
            "{}: {:?}",
            card.id,
            model.banner()
        );
        assert!(
            model.message().is_none(),
            "{}: {:?}",
            card.id,
            model.message()
        );
        assert!(!model.table_rows().is_empty(), "{}", card.id);
    }
    // The daily bars and the project ranking are two cards over one snapshot.
    let sources_of_daily: Vec<_> = config
        .chart_cards
        .iter()
        .filter(|card| card.id == "daily-cost" || card.id == "project-ranking")
        .map(|card| card.source.as_str())
        .collect();
    assert_eq!(sources_of_daily, ["daily", "daily"]);
}

#[test]
fn a_missing_executable_surfaces_as_unavailable_cards_not_a_crash() {
    let config = profile_with_command("/definitely/not/installed/agent-activity");
    let runtime = ChartRuntime::start(&config.chart_sources, normalize_output);
    let snapshots = wait_for_answers(&runtime, config.chart_sources.len());
    let now = Timestamp::now();
    for card in &config.chart_cards {
        let snapshot = snapshots
            .iter()
            .find(|snapshot| snapshot.source_id == card.source)
            .unwrap();
        assert_eq!(snapshot.status(now), SourceStatus::Unavailable);
        assert!(snapshot.info.retry_suspended, "{}", card.source);
        let model = build_model(card, snapshot, now);
        let banner = model.banner().unwrap_or_default().to_lowercase();
        assert!(banner.contains("unavailable"), "{}: {banner}", card.id);
        assert!(!banner.contains("/definitely"), "path leaked: {banner}");
    }
}

#[test]
fn native_synthetic_commands_feed_unrelated_charts_through_the_same_path() {
    let toml = format!(
        r#"
[[chart_sources]]
id = "spend"
command = ["{ROOT}/examples/charts/synthetic-spend.sh", "--from", "{{from}}", "--to", "{{to}}", "--timezone", "{{timezone}}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"

[[chart_sources]]
id = "latency"
command = ["{ROOT}/examples/charts/synthetic-latency.sh", "--from", "{{from}}", "--to", "{{to}}", "--timezone", "{{timezone}}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"

[[chart_cards]]
id = "spend-line"
source = "spend"
kind = "time-series"
metric = "amount"
group_by = ["project"]

[[chart_cards]]
id = "latency-line"
source = "latency"
kind = "time-series"
metric = "latency_ms"
group_by = ["service"]
"#
    );
    let config = parse_config(&toml).unwrap();
    let runtime = ChartRuntime::start(&config.chart_sources, normalize_output);
    let snapshots = wait_for_answers(&runtime, 2);
    let now = Timestamp::now();
    for card in &config.chart_cards {
        let snapshot = snapshots
            .iter()
            .find(|snapshot| snapshot.source_id == card.source)
            .unwrap();
        assert!(
            snapshot.dataset.is_some(),
            "{}: {:?}",
            card.id,
            snapshot.last_error
        );
        let model = build_model(card, snapshot, now);
        assert!(model.headline().is_some(), "{}", card.id);
        assert!(
            model.banner().is_none(),
            "{}: {:?}",
            card.id,
            model.banner()
        );
    }
}

#[test]
fn no_chart_configuration_means_no_chart_state() {
    let config = parse_config("enabled = []\n").unwrap();
    assert!(config.chart_sources.is_empty() && config.chart_cards.is_empty());
    let runtime = ChartRuntime::start(&config.chart_sources, normalize_output);
    assert!(runtime.snapshots().is_empty());
    assert!(runtime.poll().is_empty());
}
