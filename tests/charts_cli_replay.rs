//! `sidescope charts replay` through the real binary: saved output is
//! normalized and every card is built from the same facts the GUI uses.

mod common {
    pub mod charts;
}

use common::charts::*;
use serde_json::{Value, json};

const AT: &str = "2026-11-20T12:02:00Z";

fn cli_fixture(name: &str) -> String {
    fixture(&format!("cli/{name}"))
}

/// One native source `a` (window 2m, UTC) with the given cards.
fn one_source(scratch: &Scratch, cards: &[String]) -> String {
    let producer = scratch.producer("a.sh", "exit 0");
    scratch.config(
        &[vec![source_toml("a", &[&producer], "")], cards.to_vec()]
            .concat()
            .concat(),
    )
}

fn replay(config: &str, data: &str, extra: &[&str]) -> Out {
    let pair = format!("a={data}");
    let mut args = vec![
        "charts", "replay", "--config", config, "--at", AT, "--data", &pair,
    ];
    args.extend_from_slice(extra);
    run(&args)
}

fn micros(text: &str) -> i128 {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let negative = whole.starts_with('-');
    let whole: i128 = whole.trim_start_matches('-').parse().unwrap();
    let fraction: i128 = format!("{fraction:0<6}")[..6].parse().unwrap();
    let value = whole * 1_000_000 + fraction;
    if negative { -value } else { value }
}

#[test]
fn four_distinct_empty_outcomes_and_all_zero_data() {
    let scratch = Scratch::new("empties");
    let config = one_source(
        &scratch,
        &[card_toml("c", "a", "time-series", "amount", "")],
    );
    for (data, kind) in [
        ("native/empty-series.json", "no-observations"),
        ("cli/no-intervals.json", "no-intervals"),
        ("native/all-null.json", "no-known-values"),
        ("cli/all-zero.json", "plotted"),
    ] {
        let out = replay(&config, &fixture(data), &[]);
        assert_eq!(out.code, 0, "{data}: {}", out.stdout);
        assert_eq!(out.json["status"], "ok", "{data}");
        assert_eq!(out.card("c")["outcome"]["kind"], kind, "{data}");
        assert_eq!(out.source("a")["dataset"]["status"], "valid", "{data}");
        assert_eq!(out.check("freshness")["status"], "not-run", "{data}");
    }
    let zero = replay(&config, &fixture("cli/all-zero.json"), &[]);
    let summary = &zero.card("c")["summary"];
    assert_eq!(summary["exact"], "0");
    assert_eq!(summary["known_subtotal"], "0");
    assert_eq!(summary["quality"], "complete");
    assert_eq!(summary["bound"], "exact");
    // A zero is a value, and none of the empties is a refusal.
    let none = replay(&config, &fixture("native/empty-series.json"), &[]);
    assert_eq!(none.card("c")["summary"], Value::Null);
    assert_eq!(none.card("c")["rows"], json!([]));
}

#[test]
fn a_sum_with_a_missing_cell_never_claims_an_exact_total() {
    let scratch = Scratch::new("missing");
    let config = one_source(
        &scratch,
        &[card_toml("c", "a", "time-series", "amount", "")],
    );
    let out = replay(&config, &cli_fixture("sum-missing-cell.json"), &[]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    let summary = &out.card("c")["summary"];
    assert_eq!(summary["quality"], "missing");
    assert_eq!(summary["exact"], Value::Null);
    // The unreported cell is neither zero nor absent from the arithmetic.
    assert_eq!(summary["known_subtotal"], "0.12");
    assert_eq!(summary["bound"], "incomplete");
    assert_eq!(out.card("c")["intervals"]["missing"], 1);
    assert_eq!(out.card("c")["rows"][0]["exact"], Value::Null);
    assert_eq!(out.card("c")["rows"][0]["known_subtotal"], "0.12");
    // A partial cell under a nonnegative contribution is a lower bound.
    let partial = replay(&config, &cli_fixture("partial-nonnegative.json"), &[]);
    let summary = &partial.card("c")["summary"];
    assert_eq!(
        (
            summary["quality"].clone(),
            summary["bound"].clone(),
            summary["exact"].clone()
        ),
        (json!("partial"), json!("at-least"), json!("0.75"))
    );
    let only_missing = replay(&config, &fixture("native/all-null.json"), &[]);
    assert_eq!(only_missing.card("c")["outcome"]["kind"], "no-known-values");
    assert_eq!(
        only_missing.card("c")["summary"]["known_subtotal"],
        Value::Null
    );
}

#[test]
fn a_gauge_whose_newest_reading_is_null_keeps_the_last_known_value() {
    let scratch = Scratch::new("gauge");
    let config = one_source(
        &scratch,
        &[card_toml(
            "g",
            "a",
            "time-series",
            "latency",
            "summary = \"last\"",
        )],
    );
    let out = replay(&config, &cli_fixture("gauge-newest-null.json"), &[]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    let summary = &out.card("g")["summary"];
    assert_eq!(summary["reduction"], "last");
    assert_eq!(summary["exact"], "5");
    assert_eq!(
        summary["known_subtotal"],
        Value::Null,
        "a gauge is never a period sum"
    );
    assert_eq!(summary["newest_is_null"], true);
    assert_eq!(summary["observed"]["at"], "2026-11-20T12:00:00Z");
}

#[test]
fn one_mismatched_card_is_refused_while_its_sibling_is_plotted() {
    let scratch = Scratch::new("mismatch");
    let config = one_source(
        &scratch,
        &[
            card_toml("good", "a", "time-series", "amount", ""),
            card_toml("bad", "a", "time-series", "p95_ms", ""),
        ],
    );
    let out = replay(&config, &fixture("native/minimal.json"), &[]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.json["status"], "findings");
    assert_eq!(out.card("good")["outcome"]["kind"], "plotted");
    let bad = &out.card("bad")["outcome"];
    assert_eq!(bad["kind"], "refused");
    assert_eq!(bad["reason"], "card-mismatch");
    assert_eq!(bad["subject"], "metric");
    assert_eq!(bad["wanted"], "p95_ms");
    assert_eq!(bad["available"], json!(["amount"]));
    assert_eq!(out.source("a")["dataset"]["status"], "valid");
    assert_eq!(out.check("cards")["status"], "fail");
    assert_eq!(out.check("dataset")["status"], "pass");
}

#[test]
fn bounded_percent_on_a_non_percent_metric_is_a_card_mismatch() {
    let scratch = Scratch::new("bounded");
    let config = one_source(
        &scratch,
        &[card_toml(
            "c",
            "a",
            "time-series",
            "amount",
            "bounded_percent = true",
        )],
    );
    let out = replay(&config, &fixture("native/minimal.json"), &[]);
    // Native sources are checked against the first answer, so this is a refusal.
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.card("c")["outcome"]["subject"], "bounded-percent");
    assert_eq!(out.card("c")["outcome"]["reason"], "card-mismatch");
}

#[test]
fn too_many_gauge_series_is_a_capacity_refusal_not_an_empty_state() {
    let scratch = Scratch::new("capacity");
    let config = one_source(
        &scratch,
        &[card_toml(
            "g",
            "a",
            "time-series",
            "latency",
            "group_by = [\"service\"]",
        )],
    );
    let out = replay(&config, &cli_fixture("gauge-17-series.json"), &[]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    let outcome = &out.card("g")["outcome"];
    assert_eq!(outcome["kind"], "refused");
    assert_eq!(outcome["reason"], "capacity");
    assert_eq!(outcome["limit"], "gauge-series");
    assert_eq!(outcome["count"], 17);
    assert_eq!(outcome["maximum"], 16);
}

#[test]
fn ranked_rows_keep_unknown_apart_from_other_and_subtotals_add_up() {
    let scratch = Scratch::new("ranked");
    let config = one_source(
        &scratch,
        &[card_toml(
            "rank",
            "a",
            "ranked-bars",
            "amount",
            "group_by = [\"project\"]\ntop_n = 3",
        )],
    );
    let out = replay(
        &config,
        &cli_fixture("ranked-unknown.json"),
        &["--details", "rank"],
    );
    assert_eq!(out.code, 0, "{}", out.stdout);
    let card = out.card("rank");
    assert_eq!(card["intent"]["top_n"], 3);
    assert_eq!(card["intent"]["group_by"], json!(["project"]));
    assert_eq!(card["summary"]["exact"], "36");
    let groups = &card["groups"];
    assert_eq!(
        (
            groups["total"].as_i64(),
            groups["named_shown"].as_i64(),
            groups["other_members"].as_i64(),
            groups["unknown_shown"].as_i64()
        ),
        (Some(7), Some(3), Some(3), Some(1))
    );
    let roles: Vec<(&str, i64, &str)> = card["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["role"].as_str().unwrap(),
                row["members"].as_i64().unwrap(),
                row["exact"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        roles,
        [
            ("named", 1, "10"),
            ("named", 1, "8"),
            ("named", 1, "6"),
            ("other", 3, "9"),
            ("unknown", 1, "3")
        ]
    );
    assert_eq!(
        card["rows"][3]["group"],
        Value::Null,
        "Other has no group tuple"
    );
    let total: i128 = card["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| micros(row["known_subtotal"].as_str().unwrap()))
        .sum();
    assert_eq!(
        total,
        micros(card["summary"]["known_subtotal"].as_str().unwrap())
    );
    // Details are the full set before folding: members appear individually,
    // Unknown is exactly the empty-valued tuple, and nothing is added twice.
    let details = &card["details"];
    assert_eq!(details["grain"], "group");
    assert_eq!(details["rows_total"], 7);
    let rows = details["rows"].as_array().unwrap();
    for row in rows {
        assert_eq!(
            row["group"] == json!([""]),
            row["shown_as"] == "unknown",
            "{row}"
        );
    }
    let other_members: Vec<&str> = rows
        .iter()
        .filter(|row| row["shown_as"] == "other")
        .map(|row| row["group"][0].as_str().unwrap())
        .collect();
    assert_eq!(other_members.len(), 3);
    assert!(
        ["P4", "P5", "P6"]
            .iter()
            .all(|name| other_members.contains(name))
    );
    let details_total: i128 = rows
        .iter()
        .map(|row| micros(row["known_subtotal"].as_str().unwrap()))
        .sum();
    assert_eq!(details_total, micros("36"));
}

#[test]
fn details_pages_are_consistent_and_sum_to_the_card_subtotal() {
    let scratch = Scratch::new("paging");
    let config = one_source(
        &scratch,
        &[card_toml(
            "c",
            "a",
            "time-series",
            "amount",
            "group_by = [\"project\"]",
        )],
    );
    let data = cli_fixture("many-groups.json");
    let whole = replay(&config, &data, &["--details", "c", "--limit", "5000"]);
    assert_eq!(whole.code, 0, "{}", whole.stdout);
    let card = whole.card("c");
    assert_eq!(card["outcome"]["kind"], "plotted");
    assert_eq!(card["details"]["grain"], "interval-group");
    assert_eq!(card["details"]["rows_total"], 40);
    assert_eq!(card["details"]["complete"], true);
    assert_eq!(card["details"]["offset"], 0);
    assert_eq!(card["details"]["rows_returned"], 40);
    // Sixteen plotted rows at most, Other and Unknown included.
    assert!(card["rows"].as_array().unwrap().len() <= 16);
    assert_eq!(card["groups"]["total"], 20);
    let subtotal = micros(card["summary"]["known_subtotal"].as_str().unwrap());

    let mut seen = Vec::new();
    let mut sum = 0_i128;
    for offset in [0, 15, 30, 45] {
        let page = replay(
            &config,
            &data,
            &[
                "--details",
                "c",
                "--offset",
                &offset.to_string(),
                "--limit",
                "15",
            ],
        );
        let details = &page.card("c")["details"];
        let rows = details["rows"].as_array().unwrap();
        assert_eq!(details["rows_total"], 40);
        assert_eq!(details["offset"], offset);
        assert_eq!(details["rows_returned"], rows.len());
        assert_eq!(rows.len(), (40usize.saturating_sub(offset)).min(15));
        assert_eq!(
            details["complete"], false,
            "a partial page never says complete"
        );
        for row in rows {
            sum += micros(row["known_subtotal"].as_str().unwrap());
            seen.push(row.to_string());
        }
    }
    assert_eq!(seen.len(), 40);
    assert_eq!(sum, subtotal, "pages add up to the card's known subtotal");
    let all: Vec<String> = whole.card("c")["details"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(Value::to_string)
        .collect();
    assert_eq!(seen, all, "paging is stable and loses nothing");
    // Details are ordered by interval start, then by the retained group tuple.
    let starts: Vec<&str> = whole.card("c")["details"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["interval"]["start"].as_str().unwrap())
        .collect();
    assert!(starts.windows(2).all(|pair| pair[0] <= pair[1]));
    // No details block without --details.
    let plain = replay(&config, &data, &[]);
    assert_eq!(plain.card("c")["details"]["included"], false);
    assert!(plain.card("c")["details"].get("rows").is_none());
}

#[test]
fn replay_is_byte_identical_and_has_no_live_provenance() {
    let scratch = Scratch::new("determinism");
    let config = one_source(
        &scratch,
        &[card_toml("c", "a", "time-series", "amount", "")],
    );
    let first = replay(&config, &fixture("native/minimal.json"), &[]);
    let second = replay(&config, &fixture("native/minimal.json"), &[]);
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.json["clock"], json!({"at": AT, "origin": "explicit"}));
    let source = first.source("a");
    assert_eq!(source["data_origin"], "replay");
    assert_eq!(source["attempt"], Value::Null);
    assert_eq!(first.check("execution")["status"], "not-run");
    assert_eq!(first.check("freshness")["status"], "not-run");
    assert_eq!(first.check("coverage")["status"], "not-run");
    assert_eq!(first.check("appearance")["status"], "not-run");
    let banner = first.card("c")["display"]["banner"].as_str().unwrap();
    assert!(banner.contains("not live"), "{banner}");
    assert_eq!(first.card("c")["drawing"]["authoritative"], false);
    assert_eq!(scratch.marker_lines("a.sh"), 0, "replay ran the producer");
    // Raw output is not echoed: a distinctive notice is data, the file bytes are not.
    assert!(!first.stdout.contains("\"version\": \"sidescope-chart-v1\""));
}

#[test]
fn data_files_are_bounded_and_unreadable_ones_stop_before_evaluation() {
    let scratch = Scratch::new("data-files");
    let config = one_source(
        &scratch,
        &[card_toml("c", "a", "time-series", "amount", "")],
    );
    let valid = std::fs::read_to_string(fixture("native/minimal.json")).unwrap();
    let exactly = format!("{valid}{}", " ".repeat(1_048_576 - valid.len()));
    assert_eq!(exactly.len(), 1_048_576);
    let at_limit = scratch.write("at-limit.json", &exactly);
    let out = replay(&config, &at_limit, &[]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(out.card("c")["outcome"]["kind"], "plotted");
    let over = scratch.write("over.json", format!("{exactly} "));
    let out = replay(&config, &over, &[]);
    assert_eq!(
        (out.code, out.error_ids()),
        (2, vec!["data-unreadable".to_string()])
    );
    let missing = scratch.text("absent.json");
    let out = replay(&config, &missing, &[]);
    assert_eq!(
        (out.code, out.error_ids()),
        (2, vec!["data-unreadable".to_string()])
    );
    let out = replay(&config, &scratch.text(""), &[]);
    assert_eq!(
        out.error_ids(),
        ["data-unreadable"],
        "a directory is not data"
    );
    // Unparseable bytes are a normal source finding, not an invocation error.
    let binary = scratch.write("binary.json", [0xff, 0xfe, 0x00, 0x7b]);
    let out = replay(&config, &binary, &[]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.source("a")["dataset"]["status"], "refused");
    assert_eq!(out.source("a")["error"]["kind"], "parse");
    assert_eq!(out.card("c")["outcome"]["kind"], "unavailable");
    assert_eq!(out.card("c")["outcome"]["source_error"], "parse");
    assert_eq!(out.check("dataset")["status"], "fail");
    assert_eq!(out.source("a")["result"][2]["status"], "not-run");
}

#[test]
fn replay_selectors_name_only_what_has_data() {
    let scratch = Scratch::new("selectors");
    let a = scratch.producer("a.sh", "exit 0");
    let b = scratch.producer("b.sh", "exit 0");
    let config = scratch.config(
        &[
            source_toml("a", &[&a], ""),
            source_toml("b", &[&b], ""),
            card_toml("card-a", "a", "time-series", "amount", ""),
            card_toml("card-b", "b", "time-series", "amount", ""),
        ]
        .concat(),
    );
    let data = fixture("native/minimal.json");
    let pair = format!("a={data}");
    let base = [
        "charts", "replay", "--config", &config, "--at", AT, "--data", &pair,
    ];
    let with = |extra: &[&str]| {
        let mut args = base.to_vec();
        args.extend_from_slice(extra);
        run(&args)
    };
    let only_a = with(&[]);
    assert_eq!(only_a.code, 0);
    assert_eq!(only_a.json["sources"].as_array().unwrap().len(), 1);
    assert_eq!(only_a.json["cards"].as_array().unwrap().len(), 1);
    for (extra, id) in [
        (vec!["--card", "card-b"], "conflicting-selection"),
        (vec!["--card", "nope"], "unknown-card"),
        (vec!["--data", "zzz=x"], "unknown-source"),
        (vec!["--details", "card-b"], "conflicting-selection"),
        (vec!["--data", &pair], "usage"),
        (vec!["--from-capture", "x"], "conflicting-selection"),
    ] {
        let out = with(&extra);
        assert_eq!(
            (out.code, out.error_ids()[0].as_str()),
            (2, id),
            "{extra:?}"
        );
    }
    let no_clock = run(&["charts", "replay", "--config", &config, "--data", &pair]);
    assert_eq!(
        no_clock.error_ids()[0],
        "usage",
        "replay has no default clock"
    );
    let capture_and_at = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--at",
        AT,
        "--from-capture",
        "x",
    ]);
    assert_eq!(capture_and_at.error_ids()[0], "conflicting-selection");
    let nothing = run(&["charts", "replay", "--config", &config, "--at", AT]);
    assert_eq!(nothing.error_ids()[0], "usage");
    assert_eq!(
        scratch.marker_lines("a.sh") + scratch.marker_lines("b.sh"),
        0
    );
}

#[test]
fn an_oversized_report_becomes_one_small_error_and_no_card_content() {
    let scratch = Scratch::new("too-large");
    let config = one_source(
        &scratch,
        &[
            card_toml(
                "small",
                "a",
                "time-series",
                "amount",
                "group_by = [\"project\"]",
            ),
            card_toml(
                "huge",
                "a",
                "time-series",
                "amount",
                "group_by = [\"project\"]",
            ),
        ],
    );
    let intervals: Vec<Value> = (0..20)
        .map(|i| json!({"start": format!("2026-11-20T12:{i:02}:00Z"), "end": format!("2026-11-20T12:{:02}:00Z", i + 1)}))
        .collect();
    let series: Vec<Value> = (0..250)
        .map(|i| {
            json!({
                "id": format!("s{i:03}"),
                "dimensions": {"project": format!("{i:03}{}", "x".repeat(497))},
                "metrics": {"amount": {"values": vec!["1"; 20], "quality": vec!["complete"; 20]}},
            })
        })
        .collect();
    let data = scratch.write(
        "big.json",
        json!({
            "version": "sidescope-chart-v1", "generated_at": "2026-11-20T12:21:00Z",
            "range": {"start": "2026-11-20T12:00:00Z", "end": "2026-11-20T12:20:00Z", "timezone": "UTC"},
            "intervals": intervals,
            "metrics": {"amount": {"label": "Amount", "unit": "USD", "quantity": "interval_total", "estimate": false}},
            "series": series, "notices": [],
        })
        .to_string(),
    );
    let pair = format!("a={data}");
    let out = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--at",
        "2026-11-20T12:20:00Z",
        "--data",
        &pair,
        "--details",
        "huge",
        "--limit",
        "5000",
    ]);
    assert_eq!(out.code, 2, "{}", &out.stdout[..out.stdout.len().min(300)]);
    assert_eq!(out.error_ids(), ["report-too-large"]);
    assert_eq!(out.json["errors"][0]["card"], "huge");
    assert_eq!(out.json["cards"], json!([]));
    assert_eq!(out.json["status"], "invalid");
    assert!(out.stdout.len() < 4096, "the replacement report is small");
    // Without the heavy page the same data reports normally.
    let small = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--at",
        "2026-11-20T12:20:00Z",
        "--data",
        &pair,
        "--card",
        "small",
    ]);
    assert_eq!(small.code, 0);
}

// --- the shipped profiles and wrappers, through the public boundary -----------

#[test]
fn the_legacy_profile_replays_its_ledger_fixtures_with_the_intended_meaning() {
    let profile = format!("{ROOT}/examples/charts/agent-activity.toml");
    let ledger = |name: &str| fixture(&format!("ledger/{name}"));
    let pairs = [
        format!("recent={}", ledger("minute-workspace.json")),
        format!("daily={}", ledger("day-workspace.json")),
        format!("models={}", ledger("day-model.json")),
        format!("subscription={}", ledger("day-channel.json")),
    ];
    let mut args = vec![
        "charts",
        "replay",
        "--config",
        &profile,
        "--at",
        "2026-11-20T12:00:00Z",
    ];
    for pair in &pairs {
        args.extend(["--data", pair]);
    }
    let out = run(&args);
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(out.json["config"]["origin"], "explicit");
    let recent = out.card("recent-cost");
    assert_eq!(recent["outcome"]["kind"], "plotted");
    assert_eq!(recent["intent"]["metric"], "metered_cost");
    assert_eq!(recent["intent"]["group_by"], json!([]));
    assert_eq!(recent["summary"]["exact"], "301.25");
    assert_eq!(recent["summary"]["quality"], "partial");
    assert_eq!(recent["summary"]["bound"], "at-least");
    assert_eq!(out.source("recent")["request"]["window"], "3h");
    assert_eq!(
        out.source("recent")["request"]["from"],
        "2026-11-20T09:00:00Z"
    );
    assert_eq!(
        out.source("recent")["request"]["to"],
        "2026-11-20T12:00:00Z"
    );
    let ranking = out.card("project-ranking");
    assert_eq!(ranking["intent"]["group_by"], json!(["project"]));
    assert_eq!(ranking["kind"], "ranked-bars");
    assert_eq!(out.source("daily")["request"]["calendar_days"], 5);
    assert_eq!(
        out.card("subscription-activity")["summary"]["quality"],
        "complete"
    );
    assert_eq!(
        out.card("subscription-activity")["summary"]["bound"],
        "exact"
    );
    // A card narrowed with --card and a source with no data are different things.
    let narrowed = run(&[
        "charts",
        "replay",
        "--config",
        &profile,
        "--at",
        "2026-11-20T12:00:00Z",
        "--data",
        &pairs[0],
        "--card",
        "daily-cost",
    ]);
    assert_eq!(narrowed.error_ids()[0], "conflicting-selection");
}

fn python_convert(script: &str, args: &[&str], input: &[u8]) -> Vec<u8> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("python3")
        .arg(format!("{ROOT}/examples/charts/{script}"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 must be installed to convert quota fixtures");
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn observed(
    scratch: &Scratch,
    account: &str,
    window: &str,
    fixture_name: &str,
    file: &str,
) -> String {
    let input = std::fs::read(fixture(fixture_name)).unwrap();
    let converted = python_convert(
        "quota-observed.py",
        &[
            "--account-key",
            account,
            "--window",
            window,
            "--from",
            "2026-11-16T12:00:00Z",
            "--to",
            "2026-11-16T15:00:00Z",
            "--timezone",
            "UTC",
            "--convert-stdin",
        ],
        &input,
    );
    scratch.write(file, converted)
}

#[test]
fn observed_quota_readings_replay_with_the_account_window_headline() {
    let scratch = Scratch::new("observed");
    let profile = format!("{ROOT}/examples/charts/agent-activity-quota.toml");
    let five = observed(
        &scratch,
        "acct-a",
        "five_hour",
        "quota/observed-newest-null.json",
        "five.json",
    );
    let pair = format!("quota-observed-primary-five_hour={five}");
    let out = run(&[
        "charts",
        "replay",
        "--config",
        &profile,
        "--at",
        "2026-11-16T15:00:00Z",
        "--data",
        &pair,
        "--details",
        "quota-observed-primary-five_hour",
    ]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    let card = out.card("quota-observed-primary-five_hour");
    assert_eq!(card["outcome"]["kind"], "plotted");
    assert_eq!(card["intent"]["metric"], "observed_percent");
    assert_eq!(card["intent"]["summary"], "last");
    assert_eq!(
        out.source("quota-observed-primary-five_hour")["request"]["window"],
        "24h"
    );
    let metric = &out.source("quota-observed-primary-five_hour")["dataset"]["metrics"][0];
    assert_eq!(
        (metric["sampled"].clone(), metric["unit"].clone()),
        (json!(true), json!("percent"))
    );
    // Readings 10, 20.5, null: the headline keeps the last known reading and
    // says it is not the newest one, at that reading's own instant.
    let summary = &card["summary"];
    assert_eq!(summary["exact"], "20.5");
    assert_eq!(summary["quality"], "complete");
    assert_eq!(summary["newest_is_null"], true);
    assert_eq!(summary["observed"]["at"], "2026-11-16T12:15:00Z");
    let headline = &card["display"]["headline"];
    assert_eq!(headline["label"], "Last observed");
    let note = headline["note"].as_str().unwrap();
    assert!(
        note.contains("as of") && note.contains("newest reading was not reported"),
        "{note}"
    );
    assert!(
        !card["display"].to_string().contains("current"),
        "a reading is never called current"
    );
    let rows = card["details"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row["sampled"] == true));
    assert_eq!(rows[0]["interval"]["start"], "2026-11-16T12:00:00Z");
    assert_eq!(rows[2]["quality"], "missing");
    assert_eq!(rows[2]["exact"], Value::Null);

    // Another account and window of the same journal stays its own source.
    let seven = observed(
        &scratch,
        "acct-b",
        "seven_day",
        "quota/observed-two-accounts.json",
        "seven.json",
    );
    let pair = format!("quota-observed-primary-seven_day={seven}");
    let other = run(&[
        "charts",
        "replay",
        "--config",
        &profile,
        "--at",
        "2026-11-16T15:00:00Z",
        "--data",
        &pair,
    ]);
    assert_eq!(other.code, 0, "{}", other.stdout);
    let card = other.card("quota-observed-primary-seven_day");
    assert_eq!(card["summary"]["exact"], "9");
    assert_eq!(card["summary"]["newest_is_null"], false);
    assert_eq!(
        other.source("quota-observed-primary-seven_day")["request"]["window"],
        "7d"
    );
}

#[test]
fn estimated_quota_points_replay_as_points_and_refuse_a_percent_axis() {
    let scratch = Scratch::new("estimate");
    let input = std::fs::read(fixture("upstream/series-quota-percent.json")).unwrap();
    let converted = python_convert(
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
        &input,
    );
    let data = scratch.write("estimate.json", converted);
    let producer = scratch.producer("est.sh", "exit 0");
    let config = scratch.config(
        &[
            source_toml("a", &[&producer], "window = \"15m\"").replace("window = \"2m\"\n", ""),
            card_toml(
                "points",
                "a",
                "time-series",
                "weekly_quota_points",
                "group_by = [\"model\"]",
            ),
            card_toml(
                "bounded",
                "a",
                "time-series",
                "weekly_quota_points",
                "group_by = [\"model\"]\nbounded_percent = true",
            ),
        ]
        .concat(),
    );
    let pair = format!("a={data}");
    let out = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--at",
        "2026-11-16T12:15:00Z",
        "--data",
        &pair,
        "--details",
        "points",
    ]);
    assert_eq!(out.code, 1, "the bounded card is a finding: {}", out.stdout);
    let points = out.card("points");
    assert_eq!(points["outcome"]["kind"], "plotted");
    assert_eq!(points["summary"]["exact"], "0.5");
    assert_eq!(points["summary"]["quality"], "complete");
    let values: Vec<&str> = points["details"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["exact"].as_str().unwrap())
        .collect();
    assert_eq!(values, ["0", "0.5", "0"]);
    assert_eq!(points["details"]["rows"][0]["unit"], "percentage points");
    assert_eq!(out.source("a")["dataset"]["notices_count"], 3);
    let bounded = &out.card("bounded")["outcome"];
    assert_eq!(bounded["reason"], "card-mismatch");
    assert_eq!(bounded["subject"], "bounded-percent");
    assert_eq!(bounded["wanted"], "percent");
    assert_eq!(bounded["available"], json!(["percentage points"]));
}
