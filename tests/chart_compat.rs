//! Compatibility of the imported producer answers and the quota wrappers with
//! the authoring tools: every case drives the real `sidescope charts` binary
//! (and the wrappers' `--convert-stdin` mode) over invented fixtures. Nothing
//! runs a real producer or reads an account. Expected values are written by
//! hand; the jq line above each test derives them from the fixture text.

mod common {
    pub mod charts;
}

use common::charts::*;
use serde_json::Value;
use sidescope::chart::state::SourceSnapshot;
use sidescope::chart_mapping::normalize_output;
use sidescope::chart_runtime::ChartRuntime;
use sidescope::parse_config;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const AT: &str = "2026-11-16T15:04:05Z";
const QUOTA_PROFILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/charts/agent-activity-quota.toml"
);
const LEGACY_PROFILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/charts/agent-activity.toml"
);
const FILTERED_PROFILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/charts/agent-activity-filtered.toml"
);

/// The metrics the shipped money profiles read, over one columns-v1 source `a`.
/// `dimension` adds a `channel` dimension at `/keys/0` (only for answers whose
/// groups carry a key).
fn ledger_source(command: &str, dimension: bool) -> String {
    let dimensions = if dimension {
        "dimensions = [{ name = \"channel\", pointer = \"/keys/0\" }]\n"
    } else {
        ""
    };
    format!(
        r#"[[chart_sources]]
id = "a"
command = ["{command}"]
format = "columns-v1"
window = "15m"
timezone = "UTC"

[chart_sources.mapping]
buckets = "/buckets"
interval_start = "/start"
interval_end = "/end"
groups = "/groups"
{dimensions}range_start = "/range/from"
range_end = "/range/to"
generated_at = "/generatedAt"

[[chart_sources.mapping.metrics]]
id = "cost"
label = "Cost"
unit = "USD"
quantity = "interval_total"
estimate = true
unknown_contribution = "nonnegative"
values = "/columns/knownCostUSD"
quality_counter = "/columns/unpriced"
omitted = "zero"

[[chart_sources.mapping.metrics]]
id = "copilot"
label = "Copilot cost"
unit = "USD"
quantity = "interval_total"
estimate = true
unknown_contribution = "nonnegative"
values = "/columns/subscriptionKnownCostUSD"
quality_counter = "/columns/subscriptionUnpriced"
omitted = "zero"

[[chart_sources.mapping.metrics]]
id = "responses"
label = "Responses"
unit = "responses"
quantity = "interval_total"
values = "/columns/subscriptionResponses"
omitted = "zero"

"#
    )
}

fn money_config(scratch: &Scratch, dimension: bool, cards: &str) -> String {
    let producer = scratch.producer("a.sh", "exit 0");
    scratch.config(&format!("{}{cards}", ledger_source(&producer, dimension)))
}

fn replay_at(config: &str, at: &str, pairs: &[String], extra: &[&str]) -> Out {
    let mut args = vec!["charts", "replay", "--config", config, "--at", at];
    for pair in pairs {
        args.push("--data");
        args.push(pair);
    }
    args.extend_from_slice(extra);
    run(&args)
}

fn ledger_card(id: &str, kind: &str, metric: &str, extra: &str) -> String {
    card_toml(id, "a", kind, metric, extra)
}

fn upstream(name: &str) -> String {
    fixture(&format!("upstream/{name}"))
}

fn rows(card: &Value) -> Vec<Value> {
    card["details"]["rows"].as_array().unwrap().clone()
}

fn convert(script: &str, args: &[&str], input: &[u8]) -> (i32, Vec<u8>, String) {
    let mut child = Command::new("python3")
        .arg(format!("{ROOT}/examples/charts/{script}"))
        .args(args)
        .arg("--convert-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 must be installed to convert quota fixtures");
    let mut stdin = child.stdin.take().unwrap();
    let data = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    (
        output.status.code().unwrap(),
        output.stdout,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn estimate(scratch: &Scratch, quota: &str, group: &str, fixture_path: &str, file: &str) -> String {
    let input = std::fs::read(fixture(fixture_path)).unwrap();
    let (code, stdout, stderr) = convert(
        "quota-estimate.py",
        &[
            "--quota",
            quota,
            "--bucket",
            "5min",
            "--group-by",
            group,
            "--from",
            "2026-11-16T12:00:00Z",
            "--to",
            "2026-11-16T12:15:00Z",
            "--timezone",
            "UTC",
        ],
        &input,
    );
    assert_eq!(code, 0, "{stderr}");
    scratch.write(file, stdout)
}

fn observed(scratch: &Scratch, window: &str, fixture_path: &str, file: &str) -> String {
    let input = std::fs::read(fixture(fixture_path)).unwrap();
    let (code, stdout, stderr) = convert(
        "quota-observed.py",
        &[
            "--account-key",
            "acct-a",
            "--window",
            window,
            "--from",
            "2026-11-16T12:00:00Z",
            "--to",
            "2026-11-16T15:00:00Z",
            "--timezone",
            "UTC",
        ],
        &input,
    );
    assert_eq!(code, 0, "{stderr}");
    scratch.write(file, stdout)
}

const FIVE: &str = "quota-observed-primary-five_hour";

fn replay_observed(scratch: &Scratch, fixture_path: &str) -> Out {
    let data = observed(scratch, "five_hour", fixture_path, "o.json");
    replay_at(
        QUOTA_PROFILE,
        "2026-11-16T15:00:00Z",
        &[format!("{FIVE}={data}")],
        &["--card", FIVE, "--details", FIVE],
    )
}

// jq '.groups|length' series-empty.json  => 0: no series, three empty buckets.
#[test]
fn series_empty_is_no_observations_and_exit_zero() {
    let scratch = Scratch::new("compat-empty");
    let config = money_config(
        &scratch,
        false,
        &ledger_card("c", "time-series", "cost", ""),
    );
    let out = replay_at(
        &config,
        AT,
        &[format!("a={}", upstream("series-empty.json"))],
        &[],
    );
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(out.card("c")["outcome"]["kind"], "no-observations");
}

// jq '.groups[0].columns' series-zero-only.json => {}: a series with no
// columns is zero under omitted = "zero"; zero is observed, not missing.
#[test]
fn series_zero_only_is_plotted_with_an_exact_zero() {
    let scratch = Scratch::new("compat-zero");
    let config = money_config(
        &scratch,
        false,
        &ledger_card("c", "time-series", "cost", ""),
    );
    let out = replay_at(
        &config,
        AT,
        &[format!("a={}", upstream("series-zero-only.json"))],
        &[],
    );
    assert_eq!(out.code, 0, "{}", out.stdout);
    let card = out.card("c");
    assert_eq!(card["outcome"]["kind"], "plotted");
    assert_eq!(card["summary"]["exact"], "0");
    assert_eq!(card["summary"]["quality"], "complete");
}

// jq '[.groups[].columns.subscriptionKnownCostUSD//empty|.[]|tonumber]|add' series-unpriced.json
// => 0.02131 (the third group has no cost column and counts one unpriced response).
#[test]
fn series_unpriced_is_partial_with_a_lower_bound() {
    let scratch = Scratch::new("compat-unpriced");
    let config = money_config(
        &scratch,
        true,
        &ledger_card("c", "time-series", "copilot", "group_by = []"),
    );
    let out = replay_at(
        &config,
        AT,
        &[format!("a={}", upstream("series-unpriced.json"))],
        &[],
    );
    assert_eq!(out.code, 0, "{}", out.stdout);
    let summary = &out.card("c")["summary"];
    assert_eq!(summary["quality"], "partial");
    assert_eq!(summary["bound"], "at-least");
    assert_eq!(summary["known_subtotal"], "0.02131");
}

// jq '.groups[0].columns.knownCostUSD' series-subscription.json => ["0.0005","0","0"];
// the three subscription groups have no knownCostUSD, so they add zero by omission.
#[test]
fn series_subscription_metered_total_is_exact_and_channels_stack_unaffected() {
    let scratch = Scratch::new("compat-subscription");
    let cards = [
        ledger_card("metered", "time-series", "cost", "group_by = []"),
        ledger_card(
            "channels",
            "time-bars",
            "responses",
            "stack_by = \"channel\"\ngroup_by = [\"channel\"]",
        ),
    ]
    .concat();
    let config = money_config(&scratch, true, &cards);
    let out = replay_at(
        &config,
        AT,
        &[format!("a={}", upstream("series-subscription.json"))],
        &["--details", "channels"],
    );
    assert_eq!(out.code, 0, "{}", out.stdout);
    let metered = out.card("metered");
    assert_eq!(metered["summary"]["exact"], "0.0005");
    assert_eq!(metered["summary"]["quality"], "complete");
    let channels = out.card("channels");
    assert_eq!(channels["outcome"]["kind"], "plotted");
    assert_eq!(channels["groups"]["total"], 4);
}

// jq '.buckets[]|.state' series-open-bucket.json => null, "open", "open".
#[test]
fn series_open_bucket_downgrades_the_open_right_edge() {
    let scratch = Scratch::new("compat-open");
    let config = money_config(
        &scratch,
        false,
        &ledger_card("c", "time-series", "cost", ""),
    );
    let out = replay_at(
        &config,
        AT,
        &[format!("a={}", upstream("series-open-bucket.json"))],
        &[],
    );
    assert_eq!(out.code, 0, "{}", out.stdout);
    let intervals = &out.card("c")["intervals"];
    assert_eq!(intervals["open_right_edge"], true);
    assert_eq!(intervals["partial"], 2);
    assert_eq!(intervals["complete"], 1);
}

// jq '.buckets[]|[.start,.end]' series-dst-23-hour-day.json: 2026-03-08T05:00Z
// to 2026-03-09T04:00Z is the 23-hour day (82,800 s); the 25-hour fixture's
// second day, 2026-11-01T04:00Z to 2026-11-02T05:00Z, is 90,000 s.
#[test]
fn dst_days_keep_their_real_length_in_details() {
    for (name, seconds) in [
        ("series-dst-23-hour-day.json", 82_800),
        ("series-dst-25-hour-day.json", 90_000),
    ] {
        let scratch = Scratch::new(&format!("compat-{name}"));
        let config = money_config(&scratch, false, &ledger_card("c", "time-bars", "cost", ""));
        let out = replay_at(
            &config,
            AT,
            &[format!("a={}", upstream(name))],
            &["--details", "c"],
        );
        assert_eq!(out.code, 0, "{}", out.stdout);
        let rows = rows(out.card("c"));
        let start = rows[1]["interval"]["start"].as_str().unwrap();
        let end = rows[1]["interval"]["end"].as_str().unwrap();
        let length = parse_seconds(end) - parse_seconds(start);
        assert_eq!(length, seconds, "{name}: {start} to {end}");
        assert_eq!(rows[1]["interval"]["start"], rows[0]["interval"]["end"]);
    }
}

fn parse_seconds(instant: &str) -> i64 {
    // "2026-03-08T05:00:00Z" as seconds since the epoch, by civil-calendar arithmetic.
    let year: i64 = instant[0..4].parse().unwrap();
    let month: i64 = instant[5..7].parse().unwrap();
    let day: i64 = instant[8..10].parse().unwrap();
    let hour: i64 = instant[11..13].parse().unwrap();
    let minute: i64 = instant[14..16].parse().unwrap();
    let second: i64 = instant[17..19].parse().unwrap();
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + hour * 3600 + minute * 60 + second
}

// jq '.groups[0].keys' series-objects.json => ["claude","subscription"], and
// `.groups[0].columns` is absent: the objects layout has no columns array to read.
#[test]
fn series_objects_is_refused_never_drawn_as_zero() {
    let scratch = Scratch::new("compat-objects");
    let config = money_config(&scratch, true, &ledger_card("c", "time-series", "cost", ""));
    let out = replay_at(
        &config,
        AT,
        &[format!("a={}", upstream("series-objects.json"))],
        &[],
    );
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.source("a")["dataset"]["status"], "refused");
    assert_eq!(out.source("a")["error"]["kind"], "invalid-mapping");
    assert_ne!(out.card("c")["outcome"]["kind"], "plotted");
}

// The producer's error body, saved as stdout, is not a ledger answer; the same
// text on stderr of a producer that exits 2 is a failed command.
#[test]
fn too_many_buckets_is_invalid_mapping_as_stdout_and_nonzero_exit_as_a_failure() {
    let scratch = Scratch::new("compat-too-many");
    let config = money_config(
        &scratch,
        false,
        &ledger_card("c", "time-series", "cost", ""),
    );
    let out = replay_at(
        &config,
        AT,
        &[format!("a={}", upstream("error-too-many-buckets.json"))],
        &[],
    );
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.source("a")["error"]["kind"], "invalid-mapping");

    let failing = scratch.producer(
        "fail.sh",
        &format!(
            "cat '{}' >&2\nexit 2",
            upstream("error-too-many-buckets.json")
        ),
    );
    let config = scratch.config(&format!(
        "{}{}",
        ledger_source(&failing, false),
        ledger_card("c", "time-series", "cost", "")
    ));
    let run_out = run(&["charts", "run", "--config", &config, "--source", "a"]);
    assert_eq!(run_out.code, 1, "{}", run_out.stdout);
    assert_eq!(run_out.source("a")["error"]["kind"], "nonzero-exit");
    assert_eq!(run_out.card("c")["outcome"]["kind"], "unavailable");
    assert!(
        !run_out.stdout.contains("too many buckets"),
        "stderr text must not reach the report"
    );
}

fn jq_check(filter: &str, file: &str) -> bool {
    Command::new("jq")
        .args(["-e", filter, file])
        .stdout(Stdio::null())
        .status()
        .expect("jq must be installed (it is in the dev shell)")
        .success()
}

const QUOTA_CAPABILITY_CHECK: &str = r#"["subscriptionWeeklyPercent","subscriptionWeeklyUnweighted","subscriptionFiveHourPercent","subscriptionFiveHourUnweighted"] - (.series.columns|keys) | length == 0"#;

#[test]
fn the_documented_capability_check_passes_with_quota_columns_and_fails_without() {
    assert!(jq_check(
        QUOTA_CAPABILITY_CHECK,
        &upstream("capabilities.json")
    ));
    assert!(!jq_check(
        QUOTA_CAPABILITY_CHECK,
        &upstream("capabilities-without-quota.json")
    ));
}

// jq '.groups[0].columns.subscriptionWeeklyPercent' series-quota-percent.json => [0,0.5,0]
// and subscriptionFiveHourPercent => [0,2,0].
#[test]
fn quota_percent_through_the_estimate_wrapper_keeps_exact_points() {
    let scratch = Scratch::new("compat-percent");
    let weekly = estimate(
        &scratch,
        "weekly",
        "model",
        "upstream/series-quota-percent.json",
        "w.json",
    );
    let five = estimate(
        &scratch,
        "five-hour",
        "model",
        "upstream/series-quota-percent.json",
        "f.json",
    );
    let producer = scratch.producer("p.sh", "exit 0");
    let weekly_source =
        source_toml("w", &[&producer], "").replace("window = \"2m\"", "window = \"15m\"");
    let five_source =
        source_toml("f", &[&producer], "").replace("window = \"2m\"", "window = \"15m\"");
    let cards = [
        card_toml(
            "weekly",
            "w",
            "time-series",
            "weekly_quota_points",
            "group_by = [\"model\"]",
        ),
        card_toml(
            "five",
            "f",
            "time-series",
            "five_hour_quota_points",
            "group_by = [\"model\"]",
        ),
        card_toml(
            "bounded",
            "w",
            "time-series",
            "weekly_quota_points",
            "group_by = [\"model\"]\nbounded_percent = true",
        ),
    ]
    .concat();
    let config = scratch.config(&format!("{weekly_source}{five_source}{cards}"));
    let out = replay_at(
        &config,
        "2026-11-16T12:15:00Z",
        &[format!("w={weekly}"), format!("f={five}")],
        &["--details", "weekly"],
    );
    assert_eq!(out.code, 1, "the bounded card is a finding: {}", out.stdout);
    let exact: Vec<String> = rows(out.card("weekly"))
        .iter()
        .map(|row| row["exact"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(exact, ["0", "0.5", "0"]);
    assert_eq!(out.card("five")["summary"]["exact"], "2");
    assert!(
        out.source("w")["dataset"]["notices_count"]
            .as_u64()
            .unwrap()
            >= 1
    );
    let bounded = &out.card("bounded")["outcome"];
    assert_eq!(bounded["kind"], "refused");
    assert_eq!(bounded["reason"], "card-mismatch");
    assert_eq!(bounded["subject"], "bounded-percent");
}

// jq '.percentPerUSD' estimate-no-weight.json: claude-opus-5-5 has fiveHour null.
#[test]
fn a_model_with_no_weight_is_partial_and_at_least() {
    let scratch = Scratch::new("compat-no-weight");
    let data = estimate(
        &scratch,
        "five-hour",
        "model",
        "quota/estimate-no-weight.json",
        "e.json",
    );
    let out = replay_at(
        QUOTA_PROFILE,
        "2026-11-16T12:15:00Z",
        &[format!("quota-estimate-recent={data}")],
        &["--card", "quota-five-hour-recent"],
    );
    let card = out.card("quota-five-hour-recent");
    assert_eq!(card["outcome"]["kind"], "plotted", "{}", out.stdout);
    assert_eq!(card["summary"]["quality"], "partial");
    assert_eq!(card["summary"]["bound"], "at-least");
}

// jq '[.groups[].columns.subscriptionWeeklyPercent]' estimate-over-100.json:
// 60 + 70.5 + 40.25 = 170.75 > 100 in one window, and no 0 to 100 axis is implied.
#[test]
fn a_window_sum_above_one_hundred_is_plotted_without_a_percent_axis() {
    let scratch = Scratch::new("compat-over-100");
    let data = estimate(
        &scratch,
        "weekly",
        "none",
        "quota/estimate-over-100.json",
        "e.json",
    );
    let producer = scratch.producer("p.sh", "exit 0");
    let source = source_toml("w", &[&producer], "").replace("window = \"2m\"", "window = \"15m\"");
    let card = card_toml(
        "sum",
        "w",
        "time-series",
        "weekly_quota_points",
        "group_by = []",
    );
    let config = scratch.config(&format!("{source}{card}"));
    let out = replay_at(
        &config,
        "2026-11-16T12:15:00Z",
        &[format!("w={data}")],
        &["--details", "sum"],
    );
    assert_eq!(out.code, 0, "{}", out.stdout);
    let card = out.card("sum");
    assert_eq!(card["outcome"]["kind"], "plotted");
    assert_eq!(card["intent"]["bounded_percent"], false);
    assert_eq!(rows(card)[0]["unit"], "percentage points");
    let total: f64 = rows(card)
        .iter()
        .map(|row| row["exact"].as_str().unwrap().parse::<f64>().unwrap())
        .sum();
    assert_eq!(total, 170.75);
}

#[test]
fn missing_quota_weights_is_a_nonzero_wrapper_exit_with_no_output() {
    let input = std::fs::read(fixture("quota/estimate-missing-weights.json")).unwrap();
    let (code, stdout, _) = convert(
        "quota-estimate.py",
        &[
            "--quota",
            "weekly",
            "--bucket",
            "5min",
            "--group-by",
            "model",
            "--from",
            "2026-11-16T12:00:00Z",
            "--to",
            "2026-11-16T12:15:00Z",
            "--timezone",
            "UTC",
        ],
        &input,
    );
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
}

// Observed selector cases; every window is the five_hour window of acct-a.
// jq '.accounts[0].windows[0]|[.percent,.resets]' observed-resets.json:
// percent [90,95,3,4,5], resets at indexes 0, 2 and 4: a reset breaks the
// line before the reading it precedes and no delta is drawn.
#[test]
fn observed_resets_break_the_line_and_notice_their_count() {
    let scratch = Scratch::new("compat-resets");
    let out = replay_observed(&scratch, "quota/observed-resets.json");
    assert_eq!(out.code, 0, "{}", out.stdout);
    let breaks: Vec<bool> = rows(out.card(FIVE))
        .iter()
        .map(|row| row["break_before"].as_bool().unwrap())
        .collect();
    assert_eq!(breaks, [false, false, true, false, true]);
    assert_eq!(out.card(FIVE)["summary"]["exact"], "5");
}

// percent [10,11,12] read at 12:00, 12:46 and 13:01: the 46-minute gap exceeds
// the default 45-minute limit, the 15-minute one does not.
#[test]
fn observed_long_gap_breaks_the_line() {
    let scratch = Scratch::new("compat-gap");
    let out = replay_observed(&scratch, "quota/observed-gap.json");
    let breaks: Vec<bool> = rows(out.card(FIVE))
        .iter()
        .map(|row| row["break_before"].as_bool().unwrap())
        .collect();
    assert_eq!(breaks, [false, true, false]);
}

// percent [10,20.5,null]: the newest reading is missing, never zero.
#[test]
fn observed_newest_null_keeps_the_last_reading_and_says_so() {
    let scratch = Scratch::new("compat-newest-null");
    let out = replay_observed(&scratch, "quota/observed-newest-null.json");
    let card = out.card(FIVE);
    assert_eq!(card["summary"]["exact"], "20.5");
    assert_eq!(card["summary"]["newest_is_null"], true);
    assert_eq!(rows(card)[2]["exact"], Value::Null);
}

#[test]
fn observed_all_null_is_no_known_values() {
    let scratch = Scratch::new("compat-all-null");
    let out = replay_observed(&scratch, "quota/observed-all-null.json");
    assert_eq!(out.card(FIVE)["outcome"]["kind"], "no-known-values");
    assert_eq!(out.card(FIVE)["summary"]["exact"], Value::Null);
}

#[test]
fn observed_empty_window_empty_journal_and_absent_window_are_distinct() {
    let scratch = Scratch::new("compat-empties");
    let kinds: Vec<String> = [
        "quota/observed-empty-at.json",
        "quota/observed-empty-journal.json",
        "quota/observed-window-absent.json",
    ]
    .iter()
    .map(|name| {
        replay_observed(&scratch, name).card(FIVE)["outcome"]["kind"]
            .as_str()
            .unwrap()
            .to_string()
    })
    .collect();
    assert_eq!(
        kinds,
        ["no-intervals", "no-observations", "no-observations"]
    );
}

#[test]
fn observed_truncated_adds_a_notice_and_two_accounts_never_merge() {
    let scratch = Scratch::new("compat-truncated");
    let out = replay_observed(&scratch, "quota/observed-truncated.json");
    assert_eq!(out.source(FIVE)["dataset"]["notices_count"], 1);
    assert_eq!(out.card(FIVE)["outcome"]["kind"], "plotted");

    // Two accounts × two windows: the wrapper keeps exactly one (account, window).
    let two = replay_observed(&scratch, "quota/observed-two-accounts.json");
    assert_eq!(two.card(FIVE)["summary"]["exact"], "30");
    assert_eq!(two.source(FIVE)["dataset"]["series"], 1);
}

// The late-correction pair, published in sequence through the runtime's shared
// path, leaves the consumer snapshot equal to the second answer alone.
fn corrected_runtime(scratch: &Scratch, name: &str, answers: [&str; 2]) -> SourceSnapshot {
    let before = answers[0];
    let after = answers[1];
    let counter = scratch.path(&format!("{name}.count"));
    let producer = scratch.producer(
        &format!("{name}.sh"),
        &format!(
            "if [ -e '{c}' ]; then cat '{after}'; else : > '{c}'; cat '{before}'; fi",
            c = counter.display()
        ),
    );
    let toml = format!(
        "{}{}",
        ledger_source(&producer, false),
        ledger_card("c", "time-series", "cost", "")
    );
    let config = parse_config(&toml).unwrap();
    let runtime = ChartRuntime::start(&config.chart_sources, normalize_output);
    let wait = |ready: &dyn Fn(&SourceSnapshot) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let snapshot = runtime.snapshots().remove(0);
            if ready(&snapshot) {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "source never reached the state");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let first = wait(&|s| s.dataset.is_some());
    if answers[0] != answers[1] {
        runtime.refresh_now("a");
        return wait(&|s| s.dataset.is_some() && s.info.last_success != first.info.last_success);
    }
    first
}

#[test]
fn a_late_correction_replaces_the_earlier_answer_whole() {
    let scratch = Scratch::new("compat-late");
    let before = upstream("series-late-correction-before.json");
    let after = upstream("series-late-correction-after.json");
    let corrected = corrected_runtime(&scratch, "twice", [&before, &after]);
    let alone = corrected_runtime(&scratch, "once", [&after, &after]);
    let (c, a) = (corrected.dataset.unwrap(), alone.dataset.unwrap());
    assert_eq!(c.series(), a.series());
    assert_eq!(c.intervals(), a.intervals());
    assert_eq!(c.range(), a.range());
    assert_eq!(c.metrics(), a.metrics());
    assert_eq!(c.revision(), a.revision());
    assert_ne!(
        corrected_first(&scratch, &before).series(),
        c.series(),
        "the two answers must differ for the proof to mean anything"
    );
}

fn corrected_first(
    scratch: &Scratch,
    before: &str,
) -> std::sync::Arc<sidescope::chart::dataset::ChartDataset> {
    corrected_runtime(scratch, "first", [before, before])
        .dataset
        .unwrap()
}

// The filtered `recent` card reads a group-none answer: one group, plotted.
#[test]
fn filtered_recent_replays_a_group_none_answer_and_both_profiles_pass_check() {
    let out = run(&[
        "charts",
        "replay",
        "--config",
        FILTERED_PROFILE,
        "--at",
        AT,
        "--data",
        &format!("recent={}", upstream("series-zero-only.json")),
        "--card",
        "recent-cost",
    ]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    let card = out.card("recent-cost");
    assert_eq!(card["outcome"]["kind"], "plotted");
    assert_eq!(card["groups"]["total"], 1);
    assert_eq!(card["summary"]["exact"], "0");
    for profile in [LEGACY_PROFILE, FILTERED_PROFILE, QUOTA_PROFILE] {
        let checked = run(&["charts", "check", "--config", profile]);
        assert_eq!(checked.code, 0, "{profile}: {}", checked.stdout);
        assert_eq!(checked.json["status"], "ok");
    }
}

// The legacy profile's mapping reads `/keys/0` as the project, so a group-none
// answer cannot be its input; the filtered profile is the one made for it.
#[test]
fn the_legacy_recent_source_refuses_a_group_none_answer_and_says_why() {
    let out = run(&[
        "charts",
        "replay",
        "--config",
        LEGACY_PROFILE,
        "--at",
        AT,
        "--data",
        &format!("recent={}", upstream("series-zero-only.json")),
        "--card",
        "recent-cost",
    ]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.source("recent")["error"]["kind"], "invalid-mapping");
}
