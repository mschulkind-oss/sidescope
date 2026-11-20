//! The two bounded quota wrappers, driven as real executables with `python3`
//! on synthetic stdin and fake child executables. No real producer, account,
//! credential or collector is involved; every value in a fixture is invented.
//! Expected values are written by hand from the fixture text.

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const FROM: &str = "2026-11-16T12:00:00Z";
const TO: &str = "2026-11-16T12:15:00Z";
const OBS_FROM: &str = "2026-11-16T12:00:00Z";
const OBS_TO: &str = "2026-11-16T15:00:00Z";
const LIMIT: usize = 1_048_576;

struct Output {
    status: i32,
    stdout: String,
    stderr: String,
}

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!("{ROOT}/tests/fixtures/charts/{path}")).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "chart-quota-wrappers-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn producer(name: &str) -> String {
    format!("{ROOT}/tests/fixtures/charts/producers/{name}")
}

/// Runs a wrapper under `python3`; a missing interpreter fails the test.
fn run_wrapper(script: &str, args: &[&str], stdin: &[u8], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new("python3");
    command
        .arg(format!("{ROOT}/examples/charts/{script}"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .expect("python3 must be installed to test the quota wrappers");
    let mut input = child.stdin.take().unwrap();
    let data = stdin.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = input.write_all(&data);
    });
    let mut stdout = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    let mut stderr = Vec::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut stderr)
        .unwrap();
    let status = child
        .wait()
        .unwrap()
        .code()
        .expect("wrapper exited by code");
    writer.join().unwrap();
    Output {
        status,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}

fn estimate_args<'a>(quota: &'a str, group: &'a str) -> Vec<&'a str> {
    vec![
        "--quota",
        quota,
        "--bucket",
        "5min",
        "--group-by",
        group,
        "--from",
        FROM,
        "--to",
        TO,
        "--timezone",
        "UTC",
    ]
}

fn convert_estimate(quota: &str, group: &str, input: &[u8]) -> Output {
    let mut args = estimate_args(quota, group);
    args.push("--convert-stdin");
    run_wrapper("quota-estimate.py", &args, input, &[])
}

fn ok_estimate(quota: &str, group: &str, fixture_path: &str) -> Value {
    let out = convert_estimate(quota, group, &fixture(fixture_path));
    assert_eq!(out.status, 0, "{}", out.stderr);
    parse_one_line(&out.stdout)
}

fn parse_one_line(text: &str) -> Value {
    assert!(
        text.ends_with('\n') && text.matches('\n').count() == 1,
        "one line"
    );
    serde_json::from_str(text).unwrap()
}

fn observed_args<'a>(selector: [&'a str; 2], window: &'a str) -> Vec<&'a str> {
    vec![
        selector[0],
        selector[1],
        "--window",
        window,
        "--from",
        OBS_FROM,
        "--to",
        OBS_TO,
        "--timezone",
        "UTC",
    ]
}

fn convert_observed(selector: [&str; 2], window: &str, extra: &[&str], input: &[u8]) -> Output {
    let mut args = observed_args(selector, window);
    args.extend_from_slice(extra);
    args.push("--convert-stdin");
    run_wrapper("quota-observed.py", &args, input, &[])
}

fn ok_observed(selector: [&str; 2], window: &str, fixture_path: &str) -> Value {
    let out = convert_observed(selector, window, &[], &fixture(fixture_path));
    assert_eq!(out.status, 0, "{}", out.stderr);
    parse_one_line(&out.stdout)
}

fn key(account: &str) -> [&str; 2] {
    ["--account-key", account]
}

fn values(v: &Value, metric: &str, series: usize) -> Value {
    v["series"][series]["metrics"][metric]["values"].clone()
}

fn quality(v: &Value, metric: &str, series: usize) -> Value {
    v["series"][series]["metrics"][metric]["quality"].clone()
}

fn base_answer() -> Value {
    serde_json::from_slice(&fixture("upstream/series-quota-percent.json")).unwrap()
}

/// A producer answer with `groups` group objects and `buckets` minute buckets
/// from 2026-11-16T00:00:00Z; groups carry no columns (absent means zero).
fn sized_answer(groups: usize, buckets: usize) -> Value {
    let mut answer = base_answer();
    let stamp = |minute: usize| {
        format!(
            "2026-11-{:02}T{:02}:{:02}:00Z",
            16 + minute / 1440,
            minute % 1440 / 60,
            minute % 60
        )
    };
    answer["range"] = json!({"from": stamp(0), "to": stamp(buckets)});
    answer["buckets"] = (0..buckets)
        .map(|i| json!({"start": stamp(i), "end": stamp(i + 1)}))
        .collect();
    answer["groups"] = (0..groups)
        .map(|i| json!({"key": format!("m{i}"), "keys": [format!("m{i}")], "columns": {}}))
        .collect();
    answer
}

fn convert_value(group: &str, answer: &Value) -> Output {
    convert_estimate("weekly", group, answer.to_string().as_bytes())
}

// ---- estimate: exact decimals -------------------------------------------

#[test]
fn estimate_conversion_keeps_exact_decimal_text() {
    let out = run_wrapper(
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
            "--convert-stdin",
        ],
        &fixture("upstream/series-quota-percent.json"),
        &[],
    );
    assert_eq!(out.status, 0, "{}", out.stderr);
    let v = parse_one_line(&out.stdout);
    assert_eq!(v["version"], "sidescope-chart-v1");
    let cells = &v["series"][0]["metrics"]["weekly_quota_points"];
    assert_eq!(cells["values"], json!(["0", "0.5", "0"]));
    assert_eq!(
        cells["quality"],
        json!(["complete", "complete", "complete"])
    );
    assert_eq!(
        v["metrics"]["weekly_quota_points"]["unit"],
        "percentage points"
    );
    assert_eq!(v["notices"].as_array().unwrap().len(), 3);
}

#[test]
fn estimate_declares_the_frozen_metric_series_and_range() {
    let v = ok_estimate("weekly", "model", "upstream/series-quota-percent.json");
    let metric = &v["metrics"]["weekly_quota_points"];
    assert_eq!(metric["label"], "Estimated weekly quota contribution");
    assert_eq!(metric["quantity"], "interval_total");
    assert_eq!(metric["estimate"], true);
    assert_eq!(metric["unknown_contribution"], "nonnegative");
    assert_eq!(v["generated_at"], "2026-11-16T15:04:05.123Z");
    assert_eq!(v["revision"], "4");
    assert_eq!(
        v["range"],
        json!({"start": "2026-11-16T12:00:00.000Z", "end": "2026-11-16T12:15:00.000Z",
               "timezone": "UTC"})
    );
    assert_eq!(v["series"][0]["id"], "[\"claude-fable-5-2\"]");
    assert_eq!(
        v["series"][0]["dimensions"],
        json!({"model": "claude-fable-5-2"})
    );
    assert!(v.get("observed_through").is_none());
}

#[test]
fn estimate_bucket_bounds_are_copied_unchanged() {
    let mut answer = base_answer();
    answer["buckets"] = json!([
        {"start": "2026-11-16T12:00:00.123456789Z", "end": "2026-11-16T12:07:30.5Z"},
        {"start": "2026-11-16T12:07:30.5Z", "end": "2026-11-16T12:15:00.000Z"}
    ]);
    answer["groups"][0]["columns"] = json!({"subscriptionWeeklyPercent": [1, 2]});
    let out = convert_value("model", &answer);
    assert_eq!(out.status, 0, "{}", out.stderr);
    let v = parse_one_line(&out.stdout);
    assert_eq!(v["intervals"][0]["start"], "2026-11-16T12:00:00.123456789Z");
    assert_eq!(v["intervals"][0]["end"], "2026-11-16T12:07:30.5Z");
    assert_eq!(v["intervals"][1]["end"], "2026-11-16T12:15:00.000Z");
}

#[test]
fn estimate_five_hour_gives_two_exactly() {
    let v = ok_estimate("five-hour", "model", "upstream/series-quota-percent.json");
    assert_eq!(
        values(&v, "five_hour_quota_points", 0),
        json!(["0", "2", "0"])
    );
    assert_eq!(
        v["metrics"]["five_hour_quota_points"]["label"],
        "Estimated five-hour quota contribution"
    );
}

#[test]
fn estimate_model_without_weight_is_partial_and_noticed() {
    // estimate-no-weight.json: claude-opus-5-5 has fiveHour weight null and
    // three unweighted responses in the middle bucket; its percent column is omitted.
    let v = ok_estimate("five-hour", "model", "quota/estimate-no-weight.json");
    assert_eq!(
        values(&v, "five_hour_quota_points", 0),
        json!(["0", "2", "0"])
    );
    assert_eq!(
        values(&v, "five_hour_quota_points", 1),
        json!(["0", "0", "0"])
    );
    assert_eq!(
        quality(&v, "five_hour_quota_points", 1),
        json!(["complete", "partial", "complete"])
    );
    let notices = v["notices"].as_array().unwrap();
    assert_eq!(notices.len(), 4);
    assert_eq!(
        notices[0],
        "Quota weights generated 2026-11-16T15:00:00Z; fitted from 2026-11-01T00:00:00Z \
         through 2026-11-16T14:45:00Z."
    );
    assert_eq!(
        notices[1],
        "Weights come from a plan priced at 100 USD per month."
    );
    assert_eq!(
        notices[2],
        "1 grouped model(s) have no fitted five-hour weight; their usage is a lower bound."
    );
    assert_eq!(
        notices[3],
        "Estimated points are fitted, not published by the provider, and exclude resets \
         and other clients."
    );
}

#[test]
fn estimate_weekly_notice_only_counts_models_without_a_weekly_weight() {
    // The same answer: opus has a weekly weight of 500, so weekly has no such notice.
    let v = ok_estimate("weekly", "model", "quota/estimate-no-weight.json");
    assert_eq!(v["notices"].as_array().unwrap().len(), 3);
    // Grouping by project never names models, so it never counts them.
    let v = ok_estimate("five-hour", "project", "quota/estimate-no-weight.json");
    assert_eq!(v["notices"].as_array().unwrap().len(), 3);
    assert_eq!(
        v["series"][0]["dimensions"],
        json!({"project": "claude-fable-5-2"})
    );
}

#[test]
fn estimate_window_sum_above_one_hundred_converts_without_a_percent_unit() {
    // 60 + 70.5 + 40.25 = 170.75 across the three buckets.
    let v = ok_estimate("weekly", "none", "quota/estimate-over-100.json");
    assert_eq!(
        values(&v, "weekly_quota_points", 0),
        json!(["60", "70.5", "40.25"])
    );
    assert_eq!(
        v["metrics"]["weekly_quota_points"]["unit"],
        "percentage points"
    );
    assert_ne!(v["metrics"]["weekly_quota_points"]["unit"], "percent");
    assert_eq!(v["series"][0]["id"], "[]");
    assert_eq!(v["series"][0]["dimensions"], json!({}));
}

#[test]
fn estimate_without_quota_weights_exits_one_and_prints_nothing() {
    let out = convert_estimate(
        "weekly",
        "model",
        &fixture("quota/estimate-missing-weights.json"),
    );
    assert_eq!(out.status, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr.lines().count(), 1);
}

#[test]
fn estimate_prints_an_old_fitted_through_date_as_given() {
    let v = ok_estimate("weekly", "model", "quota/estimate-old-fit.json");
    assert_eq!(
        v["notices"][0],
        "Quota weights generated 2026-11-16T15:00:00Z; fitted from unknown through \
         2025-01-01T00:00:00Z."
    );
    assert_eq!(v["notices"].as_array().unwrap().len(), 3);
}

#[test]
fn estimate_refuses_a_notice_over_512_characters() {
    let mut answer = base_answer();
    answer["quotaWeights"]["generated"] = json!("g".repeat(513));
    let out = convert_value("model", &answer);
    assert_eq!((out.status, out.stdout.as_str()), (1, ""));
    answer["quotaWeights"]["generated"] = json!("g".repeat(400));
    assert_eq!(convert_value("model", &answer).status, 0);
}

#[test]
fn estimate_refuses_more_than_the_series_interval_and_cell_limits() {
    assert_eq!(convert_value("model", &sized_answer(256, 1)).status, 0);
    assert_eq!(convert_value("model", &sized_answer(257, 1)).status, 1);
    assert_eq!(
        convert_value("none", &sized_answer(1, 2048)).status,
        1,
        "none expects no keys"
    );
    let mut answer = sized_answer(1, 2048);
    answer["groups"] = json!([{"key": "", "keys": [], "columns": {}}]);
    assert_eq!(convert_value("none", &answer).status, 0);
    let mut answer = sized_answer(1, 2049);
    answer["groups"] = json!([{"key": "", "keys": [], "columns": {}}]);
    assert_eq!(convert_value("none", &answer).status, 1);
    // 100 x 300 = 30,000 cells is accepted. 256 x 391 = 100,096 cells is refused by
    // the cell bound itself (not merely by the 1 MiB output bound, which a full
    // 100,000-cell answer would also exceed).
    assert_eq!(convert_value("model", &sized_answer(100, 300)).status, 0);
    let out = convert_value("model", &sized_answer(256, 391));
    assert_eq!((out.status, out.stdout.as_str()), (1, ""));
    assert!(out.stderr.contains("100000 cells"), "{}", out.stderr);
}

#[test]
fn estimate_refuses_nineteen_fraction_digits_but_trims_trailing_zeros() {
    let out = convert_estimate(
        "weekly",
        "model",
        &fixture("quota/estimate-fraction-19.json"),
    );
    assert_eq!((out.status, out.stdout.as_str()), (1, ""));
    let raw = String::from_utf8(fixture("quota/estimate-fraction-19.json"))
        .unwrap()
        .replace("0.1234567890123456789", "0.5000000000000000000");
    let out = convert_estimate("weekly", "model", raw.as_bytes());
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert_eq!(
        values(&parse_one_line(&out.stdout), "weekly_quota_points", 0)[0],
        "0.5"
    );
}

#[test]
fn estimate_keeps_forty_two_significant_digits_and_never_rounds_thirty() {
    // Digits written by hand from estimate-precision.json: 24 integer + 18
    // fraction digits; 23 + 7 = 30 significant digits; 1e-18.
    let v = ok_estimate("weekly", "model", "quota/estimate-precision.json");
    assert_eq!(
        values(&v, "weekly_quota_points", 0),
        json!([
            "123456789012345678901234.123456789012345678",
            "12345678901234567890123.1234567",
            "0.000000000000000001"
        ])
    );
}

#[test]
fn estimate_refuses_twenty_five_integer_digits() {
    let raw = String::from_utf8(fixture("quota/estimate-precision.json"))
        .unwrap()
        .replace(
            "123456789012345678901234.123456789012345678",
            "1234567890123456789012345",
        );
    assert_eq!(
        convert_estimate("weekly", "model", raw.as_bytes()).status,
        1
    );
}

#[test]
fn estimate_nulls_are_missing_and_the_unknown_model_is_the_empty_string() {
    let v = ok_estimate("weekly", "model", "quota/estimate-nulls-and-unknown.json");
    assert_eq!(
        values(&v, "weekly_quota_points", 0),
        json!(["1.25", null, null])
    );
    assert_eq!(
        quality(&v, "weekly_quota_points", 0),
        json!(["complete", "missing", "missing"])
    );
    assert_eq!(v["series"][1]["id"], "[\"(unknown)\"]");
    assert_eq!(v["series"][1]["dimensions"], json!({"model": ""}));
    assert_eq!(
        quality(&v, "weekly_quota_points", 1),
        json!(["complete", "partial", "complete"])
    );
    // An unknown model has no weight, so the lower-bound notice counts it.
    assert!(
        v["notices"][2]
            .as_str()
            .unwrap()
            .starts_with("1 grouped model(s)")
    );
}

#[test]
fn estimate_exponent_and_negative_zero_literals_become_plain_text() {
    let mut answer = base_answer();
    answer["groups"][0]["columns"] = json!({});
    let text = answer.to_string().replace(
        "\"columns\":{}",
        "\"columns\":{\"subscriptionWeeklyPercent\":[1E+2,-0.0,5e-1]}",
    );
    let out = convert_estimate("weekly", "model", text.as_bytes());
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert_eq!(
        values(&parse_one_line(&out.stdout), "weekly_quota_points", 0),
        json!(["100", "0", "0.5"])
    );
}

#[test]
fn estimate_refuses_invalid_input_without_output() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("not json", b"{not json".to_vec()),
        ("not utf-8", vec![0xff, 0xfe]),
        ("array root", b"[]".to_vec()),
        ("NaN literal", br#"{"a":NaN}"#.to_vec()),
        ("duplicate key", br#"{"a":1,"a":2}"#.to_vec()),
        (
            "negative percent",
            String::from_utf8(fixture("upstream/series-quota-percent.json"))
                .unwrap()
                .replace("0.5", "-0.5")
                .into_bytes(),
        ),
        (
            "misaligned column",
            String::from_utf8(fixture("quota/estimate-over-100.json"))
                .unwrap()
                .replace("40.25", "40.25, 1")
                .into_bytes(),
        ),
    ];
    for (name, input) in cases {
        let out = convert_estimate("weekly", "none", &input);
        assert_eq!(out.status, 1, "{name}");
        assert_eq!(out.stdout, "", "{name}");
        assert_eq!(out.stderr.lines().count(), 1, "{name}");
    }
    // Fractional counts and a bucket that is not positive are refused too.
    let mut answer = base_answer();
    answer["groups"][0]["columns"]["subscriptionWeeklyUnweighted"] = json!([0, 1.5, 0]);
    assert_eq!(convert_value("model", &answer).status, 1);
    let mut answer = base_answer();
    answer["buckets"][1]["end"] = answer["buckets"][1]["start"].clone();
    assert_eq!(convert_value("model", &answer).status, 1);
}

#[test]
fn estimate_stderr_never_carries_data_values() {
    let out = convert_estimate(
        "weekly",
        "model",
        &fixture("quota/estimate-missing-weights.json"),
    );
    for value in ["claude-fable-5-2", "2026-11-16", "0.5"] {
        assert!(!out.stderr.contains(value), "{}", out.stderr);
    }
}

#[test]
fn estimate_usage_errors_exit_two() {
    let good = estimate_args("weekly", "model");
    let without = |flag: &str| -> Vec<&str> {
        let at = good.iter().position(|a| *a == flag).unwrap();
        let mut copy = good.clone();
        copy.drain(at..at + 2);
        copy
    };
    for flag in [
        "--quota",
        "--bucket",
        "--group-by",
        "--from",
        "--to",
        "--timezone",
    ] {
        let out = run_wrapper("quota-estimate.py", &without(flag), b"", &[]);
        assert_eq!((out.status, out.stdout.as_str()), (2, ""), "missing {flag}");
    }
    let swapped = |flag: &str, value: &str| -> Vec<String> {
        let mut copy: Vec<String> = good.iter().map(ToString::to_string).collect();
        let at = copy.iter().position(|a| a == flag).unwrap();
        copy[at + 1] = value.to_string();
        copy
    };
    for (flag, value) in [
        ("--quota", "monthly"),
        ("--bucket", "week"),
        ("--group-by", "harness"),
        ("--from", "yesterday"),
        ("--to", "2026-11-16T12:00:00Z"),
        ("--timezone", ""),
    ] {
        let args = swapped(flag, value);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = run_wrapper("quota-estimate.py", &refs, b"", &[]);
        assert_eq!((out.status, out.stdout.as_str()), (2, ""), "{flag} {value}");
    }
    let mut extra = good.clone();
    extra.push("--surprise");
    assert_eq!(run_wrapper("quota-estimate.py", &extra, b"", &[]).status, 2);
}

// ---- observed -------------------------------------------------------------

#[test]
fn observed_declares_a_sampled_gauge_without_a_watermark() {
    let v = ok_observed(
        key("acct-a"),
        "five_hour",
        "quota/observed-two-accounts.json",
    );
    assert_eq!(v["version"], "sidescope-chart-v1.1");
    assert_eq!(v["generated_at"], "2026-11-16T15:04:05.123Z");
    assert_eq!(
        v["range"],
        json!({"start": OBS_FROM, "end": OBS_TO, "timezone": "UTC"})
    );
    assert_eq!(
        v["metrics"]["observed_percent"],
        json!({"label": "Observed quota utilization", "unit": "percent", "quantity": "gauge",
               "estimate": false, "sampled": true})
    );
    assert!(v.get("observed_through").is_none());
    assert!(v.get("revision").is_none());
    assert_eq!(v["notices"], json!([]));
}

#[test]
fn observed_accounts_and_windows_never_merge() {
    let a5 = ok_observed(
        key("acct-a"),
        "five_hour",
        "quota/observed-two-accounts.json",
    );
    let a7 = ok_observed(
        key("acct-a"),
        "seven_day",
        "quota/observed-two-accounts.json",
    );
    let b5 = ok_observed(
        key("acct-b"),
        "five_hour",
        "quota/observed-two-accounts.json",
    );
    let b7 = ok_observed(
        key("acct-b"),
        "seven_day",
        "quota/observed-two-accounts.json",
    );
    for (v, id, want) in [
        (
            &a5,
            "[\"acct-a\",\"five_hour\"]",
            json!(["10", "20.5", "30"]),
        ),
        (&a7, "[\"acct-a\",\"seven_day\"]", json!(["41", "42", "43"])),
        (&b5, "[\"acct-b\",\"five_hour\"]", json!(["1", "2", "3"])),
        (&b7, "[\"acct-b\",\"seven_day\"]", json!(["7", "8", "9"])),
    ] {
        assert_eq!(v["series"].as_array().unwrap().len(), 1);
        assert_eq!(v["series"][0]["id"], id);
        assert_eq!(v["series"][0]["dimensions"], json!({}));
        assert_eq!(values(v, "observed_percent", 0), want);
        assert_eq!(
            quality(v, "observed_percent", 0),
            json!(["complete", "complete", "complete"])
        );
    }
    let by_label = ok_observed(
        ["--account-label", "Home"],
        "five_hour",
        "quota/observed-two-accounts.json",
    );
    assert_eq!(by_label["series"][0]["id"], "[\"acct-b\",\"five_hour\"]");
}

#[test]
fn observed_intervals_start_at_each_reading_and_the_last_ends_at_the_range() {
    let v = ok_observed(
        key("acct-a"),
        "five_hour",
        "quota/observed-two-accounts.json",
    );
    assert_eq!(
        v["intervals"],
        json!([
            {"start": "2026-11-16T12:00:00Z", "end": "2026-11-16T12:15:00Z"},
            {"start": "2026-11-16T12:15:00Z", "end": "2026-11-16T12:30:00Z"},
            {"start": "2026-11-16T12:30:00Z", "end": "2026-11-16T15:00:00Z"}
        ])
    );
    assert_eq!(
        v["series"][0]["metrics"]["observed_percent"]["breaks"],
        json!([])
    );
}

#[test]
fn observed_selector_outcomes() {
    let miss = convert_observed(
        key("acct-zzz"),
        "five_hour",
        &[],
        &fixture("quota/observed-two-accounts.json"),
    );
    assert_eq!((miss.status, miss.stdout.as_str()), (1, ""));
    let ambiguous = convert_observed(
        ["--account-label", "Shared"],
        "five_hour",
        &[],
        &fixture("quota/observed-duplicate-label.json"),
    );
    assert_eq!((ambiguous.status, ambiguous.stdout.as_str()), (1, ""));
    let by_key = ok_observed(
        key("acct-b"),
        "five_hour",
        "quota/observed-duplicate-label.json",
    );
    assert_eq!(
        values(&by_key, "observed_percent", 0),
        json!(["1", "2", "3"])
    );

    let empty = ok_observed(
        key("anything"),
        "five_hour",
        "quota/observed-empty-journal.json",
    );
    assert_eq!(empty["series"], json!([]));
    assert_eq!(empty["intervals"], json!([]));
    assert_eq!(empty["notices"], json!([]));

    let absent = ok_observed(
        key("acct-a"),
        "five_hour",
        "quota/observed-window-absent.json",
    );
    assert_eq!(absent["series"], json!([]));
    assert_eq!(
        absent["notices"],
        json!(["The account reported no readings for window five_hour."])
    );

    let no_readings = ok_observed(key("acct-a"), "five_hour", "quota/observed-empty-at.json");
    assert_eq!(no_readings["series"].as_array().unwrap().len(), 1);
    assert_eq!(no_readings["intervals"], json!([]));
    assert_eq!(values(&no_readings, "observed_percent", 0), json!([]));

    let nulls = ok_observed(key("acct-a"), "five_hour", "quota/observed-all-null.json");
    assert_eq!(
        values(&nulls, "observed_percent", 0),
        json!([null, null, null])
    );
    assert_eq!(
        quality(&nulls, "observed_percent", 0),
        json!(["missing", "missing", "missing"])
    );
}

#[test]
fn observed_newest_null_stays_missing_at_the_last_index() {
    let v = ok_observed(
        key("acct-a"),
        "five_hour",
        "quota/observed-newest-null.json",
    );
    assert_eq!(
        values(&v, "observed_percent", 0),
        json!(["10", "20.5", null])
    );
    assert_eq!(
        quality(&v, "observed_percent", 0),
        json!(["complete", "complete", "missing"])
    );
}

#[test]
fn observed_resets_break_the_line_and_only_counts_become_notices() {
    // Readings 12:00 ... 13:00 every 15 min; resets at indexes 0, 2 and 4: the
    // first is not a break, the later two are. Edges: two limit_changed and one
    // window_added are counted; usage_changed and reset_changed are not.
    let v = ok_observed(key("acct-a"), "five_hour", "quota/observed-resets.json");
    assert_eq!(
        v["series"][0]["metrics"]["observed_percent"]["breaks"],
        json!([2, 4])
    );
    assert_eq!(
        v["notices"],
        json!([
            "2 reset-time change(s) in this range; the line is broken there and no change is \
             computed across one.",
            "2 recorded limit_changed edge(s) in this range.",
            "1 recorded window_added edge(s) in this range."
        ])
    );
}

#[test]
fn observed_long_gap_breaks_at_the_default_and_not_at_sixty_minutes() {
    // 12:00 -> 12:46 is 46 minutes; 12:46 -> 13:01 is 15.
    let v = ok_observed(key("acct-a"), "five_hour", "quota/observed-gap.json");
    assert_eq!(
        v["series"][0]["metrics"]["observed_percent"]["breaks"],
        json!([1])
    );
    assert_eq!(
        v["notices"],
        json!([
            "1 gap break(s): readings more than 45 minutes apart are not joined; that \
                spacing rule is this wrapper's setting, not producer data."
        ])
    );
    let out = convert_observed(
        key("acct-a"),
        "five_hour",
        &["--max-gap-minutes", "60"],
        &fixture("quota/observed-gap.json"),
    );
    assert_eq!(out.status, 0, "{}", out.stderr);
    let v = parse_one_line(&out.stdout);
    assert_eq!(
        v["series"][0]["metrics"]["observed_percent"]["breaks"],
        json!([])
    );
    assert_eq!(v["notices"], json!([]));
}

fn observed_answer(at: &[&str], percent: Value) -> Value {
    json!({"schemaVersion": 1, "generatedAt": "2026-11-16T15:04:05Z", "truncated": false,
           "accounts": [{"provider": "claude", "accountKey": "k", "accountLabel": "L",
                         "at": at,
                         "windows": [{"id": "five_hour", "percent": percent,
                                      "resets": [], "edges": []}]}]})
}

fn convert_observed_value(answer: &Value, extra: &[&str]) -> Output {
    convert_observed(key("k"), "five_hour", extra, answer.to_string().as_bytes())
}

#[test]
fn observed_gap_of_exactly_the_limit_does_not_break_and_one_nanosecond_more_does() {
    let exact = observed_answer(
        &["2026-11-16T12:00:00Z", "2026-11-16T12:45:00Z"],
        json!([1, 2]),
    );
    let out = convert_observed_value(&exact, &[]);
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert_eq!(
        parse_one_line(&out.stdout)["series"][0]["metrics"]["observed_percent"]["breaks"],
        json!([])
    );
    let over = observed_answer(
        &["2026-11-16T12:00:00Z", "2026-11-16T12:45:00.000000001Z"],
        json!([1, 2]),
    );
    let out = convert_observed_value(&over, &[]);
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert_eq!(
        parse_one_line(&out.stdout)["series"][0]["metrics"]["observed_percent"]["breaks"],
        json!([1])
    );
    // The same nanosecond holds with an offset spelling of the second reading.
    let offset = observed_answer(
        &[
            "2026-11-16T12:00:00Z",
            "2026-11-16T07:45:00.000000001-05:00",
        ],
        json!([1, 2]),
    );
    let out = convert_observed_value(&offset, &[]);
    assert_eq!(
        parse_one_line(&out.stdout)["series"][0]["metrics"]["observed_percent"]["breaks"],
        json!([1])
    );
}

#[test]
fn observed_instants_one_nanosecond_apart_stay_distinct() {
    let v = ok_observed(key("acct-a"), "five_hour", "quota/observed-nanos.json");
    assert_eq!(
        v["intervals"],
        json!([
            {"start": "2026-11-16T12:00:00.000000001Z", "end": "2026-11-16T12:00:00.000000002Z"},
            {"start": "2026-11-16T12:00:00.000000002Z", "end": "2026-11-16T15:00:00Z"}
        ])
    );
    assert_eq!(values(&v, "observed_percent", 0), json!(["5", "6"]));
    let same = observed_answer(
        &[
            "2026-11-16T12:00:00.000000001Z",
            "2026-11-16T12:00:00.000000001Z",
        ],
        json!([5, 6]),
    );
    assert_eq!(convert_observed_value(&same, &[]).status, 1);
}

#[test]
fn observed_reading_at_from_is_kept_and_at_to_is_outside() {
    let at_from = observed_answer(&[OBS_FROM], json!([5]));
    assert_eq!(convert_observed_value(&at_from, &[]).status, 0);
    let at_to = observed_answer(&["2026-11-16T12:00:00Z", OBS_TO], json!([5, 6]));
    assert_eq!(convert_observed_value(&at_to, &[]).status, 1);
    let before = observed_answer(&["2026-11-16T11:59:59.999999999Z"], json!([5]));
    assert_eq!(convert_observed_value(&before, &[]).status, 1);
    let last_ns = observed_answer(&["2026-11-16T14:59:59.999999999Z"], json!([5]));
    let out = convert_observed_value(&last_ns, &[]);
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert_eq!(
        parse_one_line(&out.stdout)["intervals"][0]["end"],
        "2026-11-16T15:00:00Z"
    );
}

#[test]
fn observed_truncated_adds_a_notice_and_keeps_readings_complete() {
    let v = ok_observed(key("acct-a"), "five_hour", "quota/observed-truncated.json");
    assert_eq!(
        v["notices"],
        json!([
            "Earlier readings exist but were not returned; the journal holds more than was \
                read."
        ])
    );
    assert_eq!(
        quality(&v, "observed_percent", 0),
        json!(["complete", "complete", "complete"])
    );
}

#[test]
fn observed_percent_keeps_exact_decimal_text() {
    let mut answer = observed_answer(
        &["2026-11-16T12:00:00Z", "2026-11-16T12:15:00Z"],
        json!([0]),
    );
    answer["accounts"][0]["windows"][0]["percent"] = json!([0, 0]);
    let text = answer.to_string().replace(
        "\"percent\":[0,0]",
        "\"percent\":[12.345678901234567890,1E+1]",
    );
    let out = convert_observed(key("k"), "five_hour", &[], text.as_bytes());
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert_eq!(
        values(&parse_one_line(&out.stdout), "observed_percent", 0),
        json!(["12.34567890123456789", "10"])
    );
    let too_fine = text.replace("12.345678901234567890", "12.3456789012345678901");
    assert_eq!(
        convert_observed(key("k"), "five_hour", &[], too_fine.as_bytes()).status,
        1
    );
}

#[test]
fn observed_refuses_each_invalid_input() {
    let two = ["2026-11-16T12:00:00Z", "2026-11-16T12:15:00Z"];
    let mut cases: Vec<(&str, Value)> = vec![
        (
            "duplicate instants",
            observed_answer(&[two[0], two[0]], json!([1, 2])),
        ),
        (
            "decreasing instants",
            observed_answer(&[two[1], two[0]], json!([1, 2])),
        ),
        (
            "bad instant text",
            observed_answer(&["noon", two[1]], json!([1, 2])),
        ),
        (
            "impossible date",
            observed_answer(&["2026-02-30T00:00:00Z"], json!([1])),
        ),
        ("negative percent", observed_answer(&two, json!([1, -1]))),
        ("string percent", observed_answer(&two, json!([1, "2"]))),
        ("boolean percent", observed_answer(&two, json!([1, true]))),
        ("percent too short", observed_answer(&two, json!([1]))),
        ("percent too long", observed_answer(&two, json!([1, 2, 3]))),
    ];
    let mut bad = observed_answer(&two, json!([1, 2]));
    bad["accounts"][0]["windows"][0]["resets"] =
        json!([{"index": 2, "resetsAt": "2026-11-16T17:00:00Z"}]);
    cases.push(("reset index out of range", bad));
    let mut bad = observed_answer(&two, json!([1, 2]));
    bad["accounts"][0]["windows"][0]["resets"] = json!([
        {"index": 1, "resetsAt": "2026-11-16T17:00:00Z"},
        {"index": 1, "resetsAt": "2026-11-16T18:00:00Z"}
    ]);
    cases.push(("reset index not increasing", bad));
    let mut bad = observed_answer(&two, json!([1, 2]));
    bad["accounts"][0]["windows"][0]["resets"] = json!([{"index": 0, "resetsAt": "later"}]);
    cases.push(("reset instant invalid", bad));
    let mut bad = observed_answer(&two, json!([1, 2]));
    bad["accounts"][0]["accountKey"] = Value::Null;
    cases.push(("account without a key", bad));
    let mut bad = observed_answer(&two, json!([1, 2]));
    bad["truncated"] = json!("yes");
    cases.push(("truncated not a boolean", bad));
    for (name, answer) in cases {
        let out = convert_observed_value(&answer, &[]);
        assert_eq!(out.status, 1, "{name}");
        assert_eq!(out.stdout, "", "{name}");
        assert_eq!(out.stderr.lines().count(), 1, "{name}");
    }
}

#[test]
fn observed_refuses_more_than_2048_readings() {
    let stamp = |i: usize| {
        format!(
            "2026-11-16T{:02}:{:02}:{:02}Z",
            12 + i / 3600,
            i / 60 % 60,
            i % 60
        )
    };
    let at: Vec<String> = (0..2048).map(stamp).collect();
    let refs: Vec<&str> = at.iter().map(String::as_str).collect();
    let answer = observed_answer(&refs, json!(vec![1; 2048]));
    let out = convert_observed_value(&answer, &["--max-gap-minutes", "1440"]);
    assert_eq!(out.status, 0, "{}", out.stderr);
    let at: Vec<String> = (0..2049).map(stamp).collect();
    let refs: Vec<&str> = at.iter().map(String::as_str).collect();
    let answer = observed_answer(&refs, json!(vec![1; 2049]));
    let out = convert_observed_value(&answer, &["--max-gap-minutes", "1440"]);
    assert_eq!((out.status, out.stdout.as_str()), (1, ""));
}

#[test]
fn observed_usage_errors_exit_two() {
    let input = fixture("quota/observed-two-accounts.json");
    let run = |args: &[&str]| run_wrapper("quota-observed.py", args, &input, &[]);
    let base_tail = [
        "--from",
        OBS_FROM,
        "--to",
        OBS_TO,
        "--timezone",
        "UTC",
        "--convert-stdin",
    ];
    let with = |head: &[&str]| -> Vec<String> {
        head.iter()
            .chain(base_tail.iter())
            .map(ToString::to_string)
            .collect()
    };
    let as_refs = |v: &Vec<String>| -> Vec<String> { v.clone() };
    let cases: Vec<Vec<String>> = vec![
        with(&["--window", "five_hour"]),
        with(&[
            "--account-key",
            "a",
            "--account-label",
            "b",
            "--window",
            "five_hour",
        ]),
        with(&["--account-key", "a"]),
        with(&["--account-key", "a", "--window", "monthly"]),
        with(&["--account-key", "", "--window", "five_hour"]),
        with(&[
            "--account-key",
            "a",
            "--window",
            "five_hour",
            "--max-gap-minutes",
            "0",
        ]),
        with(&[
            "--account-key",
            "a",
            "--window",
            "five_hour",
            "--max-gap-minutes",
            "1441",
        ]),
        with(&[
            "--account-key",
            "a",
            "--window",
            "five_hour",
            "--max-gap-minutes",
            "x",
        ]),
        with(&["--account-key", "a", "--window", "five_hour", "--surprise"]),
    ];
    for case in &cases {
        let args = as_refs(case);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = run(&refs);
        assert_eq!((out.status, out.stdout.as_str()), (2, ""), "{case:?}");
    }
    for minutes in ["1", "60", "1440"] {
        let args = with(&[
            "--account-key",
            "acct-a",
            "--window",
            "five_hour",
            "--max-gap-minutes",
            minutes,
        ]);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(run(&refs).status, 0, "{minutes}");
    }
}

// ---- bounded input and fake child executables ------------------------------

fn record_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn tab(args: &[&str]) -> String {
    args.iter().map(|a| format!("{a}\t")).collect()
}

fn padded_answer(total: usize) -> Vec<u8> {
    let mut data = fixture("upstream/series-quota-percent.json");
    while data.last() == Some(&b'\n') {
        data.pop();
    }
    assert!(data.len() <= total);
    data.resize(total, b' ');
    data
}

#[test]
fn stdin_of_exactly_one_mebibyte_is_accepted_and_one_more_byte_refused() {
    let mut args = estimate_args("weekly", "model");
    args.push("--convert-stdin");
    let ok = run_wrapper("quota-estimate.py", &args, &padded_answer(LIMIT), &[]);
    assert_eq!(ok.status, 0, "{}", ok.stderr);
    let refused = run_wrapper("quota-estimate.py", &args, &padded_answer(LIMIT + 1), &[]);
    assert_eq!((refused.status, refused.stdout.as_str()), (1, ""));

    let observed = |size: usize| {
        let mut data = fixture("quota/observed-two-accounts.json");
        while data.last() == Some(&b'\n') {
            data.pop();
        }
        data.resize(size, b' ');
        convert_observed(key("acct-a"), "five_hour", &[], &data)
    };
    assert_eq!(observed(LIMIT).status, 0);
    assert_eq!(observed(LIMIT + 1).status, 1);
}

#[test]
fn conversion_mode_spawns_nothing() {
    let dir = scratch("convert-spawns-nothing");
    let record = dir.join("record");
    let mut args = estimate_args("weekly", "model");
    args.extend(["--convert-stdin", "--producer"]);
    let script = producer("answer.sh");
    args.push(&script);
    let out = run_wrapper(
        "quota-estimate.py",
        &args,
        &fixture("upstream/series-quota-percent.json"),
        &[
            ("FAKE_RECORD", record.to_str().unwrap()),
            ("FAKE_ANSWER", "/nonexistent"),
        ],
    );
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert!(!record.exists(), "the producer ran in conversion mode");
}

#[test]
fn estimate_runs_one_producer_with_the_frozen_flags() {
    let dir = scratch("estimate-argv");
    let record = dir.join("record");
    let answer = format!("{ROOT}/tests/fixtures/charts/upstream/series-quota-percent.json");
    let script = producer("answer.sh");
    let mut args = estimate_args("weekly", "project");
    args.extend(["--producer", &script]);
    let out = run_wrapper(
        "quota-estimate.py",
        &args,
        b"",
        &[
            ("FAKE_RECORD", record.to_str().unwrap()),
            ("FAKE_ANSWER", &answer),
        ],
    );
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert_eq!(
        record_lines(&record),
        vec![tab(&[
            "ledger",
            "series",
            "--format",
            "json",
            "--layout",
            "columns",
            "--bucket",
            "5min",
            "--group-by",
            "workspace",
            "--channel",
            "subscription",
            "--columns",
            "subscriptionWeeklyPercent,subscriptionWeeklyUnweighted",
            "--from",
            FROM,
            "--to",
            TO,
            "--timezone",
            "UTC",
        ])]
    );
    let v = parse_one_line(&out.stdout);
    assert_eq!(
        v["series"][0]["dimensions"],
        json!({"project": "claude-fable-5-2"})
    );

    let record = dir.join("record-five-hour");
    let mut args = estimate_args("five-hour", "none");
    args.extend(["--producer", &script]);
    let out = run_wrapper(
        "quota-estimate.py",
        &args,
        b"",
        &[
            ("FAKE_RECORD", record.to_str().unwrap()),
            ("FAKE_ANSWER", &answer),
        ],
    );
    assert_eq!(out.status, 1, "a keyed group is not a group-by none answer");
    assert_eq!(
        record_lines(&record),
        vec![tab(&[
            "ledger",
            "series",
            "--format",
            "json",
            "--layout",
            "columns",
            "--bucket",
            "5min",
            "--group-by",
            "none",
            "--channel",
            "subscription",
            "--columns",
            "subscriptionFiveHourPercent,subscriptionFiveHourUnweighted",
            "--from",
            FROM,
            "--to",
            TO,
            "--timezone",
            "UTC",
        ])]
    );
}

#[test]
fn observed_runs_one_producer_with_the_frozen_flags() {
    let dir = scratch("observed-argv");
    let record = dir.join("record");
    let answer = format!("{ROOT}/tests/fixtures/charts/quota/observed-two-accounts.json");
    let script = producer("answer.sh");
    let mut args = observed_args(key("acct-a"), "seven_day");
    args.extend(["--producer", &script]);
    let out = run_wrapper(
        "quota-observed.py",
        &args,
        b"",
        &[
            ("FAKE_RECORD", record.to_str().unwrap()),
            ("FAKE_ANSWER", &answer),
        ],
    );
    assert_eq!(out.status, 0, "{}", out.stderr);
    assert_eq!(
        record_lines(&record),
        vec![tab(&[
            "quota",
            "series",
            "--format",
            "json",
            "--from",
            OBS_FROM,
            "--to",
            OBS_TO,
            "--window",
            "seven_day",
        ])]
    );
    assert_eq!(
        values(&parse_one_line(&out.stdout), "observed_percent", 0),
        json!(["41", "42", "43"])
    );
}

#[test]
fn producer_failure_and_missing_executable_exit_one_without_output() {
    let dir = scratch("producer-failure");
    let record = dir.join("record");
    let script = producer("fail.sh");
    let mut args = estimate_args("weekly", "model");
    args.extend(["--producer", &script]);
    let out = run_wrapper(
        "quota-estimate.py",
        &args,
        b"",
        &[("FAKE_RECORD", record.to_str().unwrap())],
    );
    assert_eq!((out.status, out.stdout.as_str()), (1, ""));
    assert_eq!(record_lines(&record).len(), 1);

    let record = dir.join("record-observed");
    let mut args = observed_args(key("acct-a"), "five_hour");
    args.extend(["--producer", &script]);
    let out = run_wrapper(
        "quota-observed.py",
        &args,
        b"",
        &[("FAKE_RECORD", record.to_str().unwrap())],
    );
    assert_eq!((out.status, out.stdout.as_str()), (1, ""));
    assert_eq!(record_lines(&record).len(), 1);

    let missing = dir.join("no-such-producer");
    let mut args = estimate_args("weekly", "model");
    args.extend(["--producer", missing.to_str().unwrap()]);
    let out = run_wrapper("quota-estimate.py", &args, b"", &[]);
    assert_eq!((out.status, out.stdout.as_str()), (1, ""));
}

#[test]
fn producer_output_of_one_mebibyte_is_accepted_and_one_more_byte_refused() {
    let dir = scratch("producer-bound");
    let script = producer("answer.sh");
    for (size, status) in [(LIMIT, 0), (LIMIT + 1, 1)] {
        let answer = dir.join(format!("answer-{size}.json"));
        std::fs::write(&answer, padded_answer(size)).unwrap();
        let record = dir.join(format!("record-{size}"));
        let mut args = estimate_args("weekly", "model");
        args.extend(["--producer", &script]);
        let out = run_wrapper(
            "quota-estimate.py",
            &args,
            b"",
            &[
                ("FAKE_RECORD", record.to_str().unwrap()),
                ("FAKE_ANSWER", answer.to_str().unwrap()),
            ],
        );
        assert_eq!(out.status, status, "{size}: {}", out.stderr);
        assert_eq!(record_lines(&record).len(), 1, "exactly one child");
    }
}

fn process_is_running(pid: &str) -> bool {
    Path::new(&format!("/proc/{}", pid.trim())).exists()
}

#[test]
fn flooding_producer_is_killed_and_reaped_within_ten_seconds() {
    let script = producer("flood.sh");
    for wrapper in ["quota-estimate.py", "quota-observed.py"] {
        let dir = scratch(&format!("flood-{wrapper}"));
        let record = dir.join("record");
        let pid_file = dir.join("pid");
        let mut args = if wrapper == "quota-estimate.py" {
            estimate_args("weekly", "model")
        } else {
            observed_args(key("acct-a"), "five_hour")
        };
        args.extend(["--producer", &script]);
        let started = Instant::now();
        let out = run_wrapper(
            wrapper,
            &args,
            b"",
            &[
                ("FAKE_RECORD", record.to_str().unwrap()),
                ("FAKE_PID", pid_file.to_str().unwrap()),
            ],
        );
        let elapsed = started.elapsed();
        eprintln!("{wrapper} flood refused after {elapsed:?}");
        assert!(elapsed < Duration::from_secs(10), "{wrapper}: {elapsed:?}");
        assert_eq!((out.status, out.stdout.as_str()), (1, ""), "{wrapper}");
        assert_eq!(record_lines(&record).len(), 1, "{wrapper}: one child only");
        let pid = std::fs::read_to_string(&pid_file).expect("producer recorded its pid");
        assert!(
            !process_is_running(&pid),
            "{wrapper}: producer {pid} still runs"
        );
    }
}

#[test]
fn wrappers_are_python_standard_library_only_and_executable() {
    use std::os::unix::fs::PermissionsExt;
    for name in ["quota-estimate.py", "quota-observed.py"] {
        let path = format!("{ROOT}/examples/charts/{name}");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("#!/usr/bin/env python3\n"), "{name}");
        assert!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o111 != 0,
            "{name} is not executable"
        );
        for line in text
            .lines()
            .filter(|l| l.starts_with("import ") || l.starts_with("from "))
        {
            let module = line.split_whitespace().nth(1).unwrap();
            assert!(
                [
                    "argparse",
                    "decimal",
                    "json",
                    "os",
                    "re",
                    "subprocess",
                    "sys",
                    "unicodedata"
                ]
                .contains(&module),
                "{name}: {line}"
            );
        }
        assert!(!text.contains("float("), "{name}: binary float");
        assert!(
            !text.contains("import datetime") && !text.contains("from datetime"),
            "{name}: datetime truncates nanoseconds"
        );
        assert!(
            !text.contains(".normalize(") && !text.contains(".quantize("),
            "{name}: context-dependent rounding"
        );
        assert!(!text.contains("shell=True"), "{name}");
        assert!(
            !text.contains("start_new_session") && !text.contains("setsid"),
            "{name}"
        );
    }
}
