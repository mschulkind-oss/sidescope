//! `sidescope charts run`: explicit, once-only execution of named sources
//! through the existing runtime. Producers are fake executables whose marker
//! files show exactly which processes were spawned.

mod common {
    pub mod charts;
}

use common::charts::*;
use serde_json::Value;
use std::time::Instant;

fn names(scratch: &Scratch) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(&scratch.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn ok_producer(scratch: &Scratch, name: &str) -> String {
    scratch.producer(name, &cat(&fixture("native/minimal.json")))
}

#[test]
fn run_spawns_exactly_the_named_source_once() {
    let scratch = Scratch::new("named");
    let a = ok_producer(&scratch, "a.sh");
    let b = ok_producer(&scratch, "b.sh");
    let config = scratch.config(
        &[
            source_toml("a", &[&a], ""),
            source_toml("b", &[&b], ""),
            card_toml("card-a", "a", "time-series", "amount", ""),
            card_toml("card-b", "b", "time-series", "amount", ""),
        ]
        .concat(),
    );
    let out = run(&["charts", "run", "--config", &config, "--source", "a"]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(scratch.marker_lines("a.sh"), 1);
    assert_eq!(scratch.marker_lines("b.sh"), 0);
    assert_eq!(out.json["command"], "run");
    assert_eq!(out.json["clock"]["origin"], "wall");
    assert_eq!(out.json["sources"].as_array().unwrap().len(), 1);
    let source = out.source("a");
    assert_eq!(source["data_origin"], "run");
    assert_eq!(source["attempt"]["exit"], 0);
    assert!(source["attempt"]["latency_ms"].is_u64());
    // The request clock is the resolved window end, whole seconds.
    assert_eq!(source["clocks"]["request_at"], source["request"]["to"]);
    assert_eq!(out.card("card-a")["outcome"]["kind"], "plotted");
    assert_eq!(
        out.card("card-a")["display"]["banner"],
        Value::Null,
        "live data has no replay banner"
    );
    for (id, status) in [
        ("execution", "pass"),
        ("dataset", "pass"),
        ("freshness", "pass"),
        ("cards", "pass"),
        ("coverage", "unknown"),
    ] {
        assert_eq!(out.check(id)["status"], status, "{id}");
    }
    // The producer asserted no watermark: unknown, never required, still exit 0.
    assert_eq!(out.check("coverage")["required"], false);
    assert_eq!(out.json["status"], "ok");

    let both = run(&["charts", "run", "--config", &config, "--all-sources"]);
    assert_eq!(both.code, 0, "{}", both.stdout);
    assert_eq!(scratch.marker_lines("a.sh"), 2);
    assert_eq!(scratch.marker_lines("b.sh"), 1);
    // --card narrows output but never what runs.
    let narrowed = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--all-sources",
        "--card",
        "card-a",
    ]);
    assert_eq!(narrowed.json["cards"].as_array().unwrap().len(), 1);
    assert_eq!(narrowed.json["sources"].as_array().unwrap().len(), 2);
    assert_eq!(scratch.marker_lines("b.sh"), 2);
}

#[test]
fn a_fast_and_a_slow_source_each_run_once_even_past_their_refresh_interval() {
    let scratch = Scratch::new("fast-slow");
    let minimal = fixture("native/minimal.json");
    let fast = scratch.producer("fast.sh", &cat(&minimal));
    let slow = scratch.producer("slow.sh", &format!("sleep 6.5\n{}", cat(&minimal)));
    let config = scratch.config(
        &[
            source_toml("fast", &[&fast], "refresh_seconds = 5"),
            source_toml("slow", &[&slow], "refresh_seconds = 5"),
            card_toml("fast-card", "fast", "time-series", "amount", ""),
            card_toml("slow-card", "slow", "time-series", "amount", ""),
        ]
        .concat(),
    );
    let started = Instant::now();
    let out = run(&["charts", "run", "--config", &config, "--all-sources"]);
    let elapsed = started.elapsed();
    println!("fast+slow wall time: {elapsed:?}");
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(
        scratch.marker_lines("fast.sh"),
        1,
        "fast ran again after its refresh interval"
    );
    assert_eq!(scratch.marker_lines("slow.sh"), 1);
    assert!(
        elapsed.as_secs_f64() >= 6.5,
        "the slow source was waited for"
    );
    assert!(
        elapsed.as_secs_f64() < 12.0,
        "the run ended when the last source did: {elapsed:?}"
    );
    assert!(
        out.source("slow")["attempt"]["latency_ms"]
            .as_u64()
            .unwrap()
            >= 6_500
    );
}

#[test]
fn reports_never_hold_raw_output_or_argv_and_no_file_appears_unasked() {
    let scratch = Scratch::new("privacy");
    let leak = scratch.producer(
        "leak.sh",
        "printf '%s' '{not json SECRET-RAW-OUTPUT'\necho SECRET-STDERR-TEXT >&2",
    );
    let fail = scratch.producer("fail.sh", "echo SECRET-FAILURE-STDERR >&2\nexit 3");
    let config = scratch.config(
        &[
            source_toml("leak", &[&leak, "--token=SECRET-ARGUMENT"], ""),
            source_toml("fail", &[&fail], "capture_stderr = true"),
            card_toml("leak-card", "leak", "time-series", "amount", ""),
            card_toml("fail-card", "fail", "time-series", "amount", ""),
        ]
        .concat(),
    );
    let before = names(&scratch);
    let out = run(&["charts", "run", "--config", &config, "--all-sources"]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    for secret in ["SECRET-RAW-OUTPUT", "SECRET-STDERR-TEXT", "SECRET-ARGUMENT"] {
        assert!(!out.stdout.contains(secret), "{secret} leaked");
    }
    assert!(
        !out.stderr.contains("SECRET"),
        "stderr leaked: {}",
        out.stderr
    );
    // stderr is shown only for the source that opted in, and only as an excerpt.
    assert_eq!(
        out.source("fail")["error"]["stderr_excerpt"],
        "SECRET-FAILURE-STDERR"
    );
    assert_eq!(out.source("leak")["error"]["stderr_excerpt"], Value::Null);
    assert_eq!(out.source("leak")["request"]["argc"], 2);
    assert_eq!(out.source("leak")["request"]["executable"], "leak.sh");
    assert!(out.source("leak")["request"].get("argv").is_none());
    let after = names(&scratch);
    let created: Vec<&String> = after.iter().filter(|name| !before.contains(name)).collect();
    assert_eq!(
        created.len(),
        2,
        "only the producers' marker files: {created:?}"
    );
    assert!(
        created.iter().all(|name| name.ends_with(".marker")),
        "{created:?}"
    );
    let shown = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--source",
        "leak",
        "--show-argv",
    ]);
    assert!(shown.stdout.contains("--token=SECRET-ARGUMENT"));
    assert!(!shown.stdout.contains("SECRET-RAW-OUTPUT"));
}

#[test]
fn a_failed_source_is_a_finding_and_its_dependents_say_why_they_did_not_run() {
    let scratch = Scratch::new("failed");
    let ok = ok_producer(&scratch, "ok.sh");
    let fail = scratch.producer("fail.sh", "exit 3");
    let config = scratch.config(
        &[
            source_toml("ok", &[&ok], ""),
            source_toml("bad", &[&fail], ""),
            card_toml("ok-card", "ok", "time-series", "amount", ""),
            card_toml("bad-card", "bad", "time-series", "amount", ""),
        ]
        .concat(),
    );
    let out = run(&["charts", "run", "--config", &config, "--all-sources"]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(
        out.json["status"], "findings",
        "a failed source is never undetermined"
    );
    let bad = out.source("bad");
    assert_eq!(bad["attempt"]["exit"], 3);
    assert_eq!(bad["dataset"]["status"], "absent");
    assert_eq!(bad["error"]["kind"], "nonzero-exit");
    let result: Vec<(&str, &str)> = bad["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["id"].as_str().unwrap(), c["status"].as_str().unwrap()))
        .collect();
    assert_eq!(
        result,
        [
            ("config", "pass"),
            ("request", "pass"),
            ("execution", "fail"),
            ("dataset", "not-run"),
            ("cards", "fail"),
            ("freshness", "not-run"),
            ("coverage", "not-run"),
            ("appearance", "not-run"),
        ]
    );
    for index in [3, 5] {
        let why = bad["result"][index]["why"].as_str().unwrap();
        assert!(why.starts_with("dependency failed: execution"), "{why}");
    }
    // The aggregate cannot read as a pass while a source failed.
    assert_eq!(out.check("execution")["status"], "fail");
    assert_ne!(out.check("freshness")["status"], "pass");
    assert_ne!(out.check("dataset")["status"], "pass");
    assert_eq!(out.check("cards")["status"], "fail");
    assert_eq!(out.card("bad-card")["outcome"]["kind"], "unavailable");
    assert_eq!(
        out.card("bad-card")["outcome"]["source_error"],
        "nonzero-exit"
    );
    assert_eq!(out.card("ok-card")["outcome"]["kind"], "plotted");
    assert_eq!(out.source("ok")["result"][5]["status"], "pass");
    assert_eq!(scratch.marker_lines("fail.sh"), 1);
}

#[test]
fn every_way_a_command_can_fail_has_its_stable_error_kind() {
    let scratch = Scratch::new("kinds");
    let not_executable = scratch.write("plain.txt", "#!/bin/sh\nexit 0\n");
    let sleeper = scratch.producer("sleeper.sh", "sleep 5");
    let flood = scratch.producer("flood.sh", "head -c 1048577 /dev/zero");
    let garbage = scratch.producer("garbage.sh", "printf '%s' '{not json'");
    let binary = scratch.producer("binary.sh", "printf '\\377\\376{'");
    let cases: Vec<(&str, Vec<String>, &str, &str, &str)> = vec![
        (
            "missing",
            vec![scratch.text("nope")],
            "",
            "missing-executable",
            "absent",
        ),
        ("plain", vec![not_executable], "", "spawn", "absent"),
        (
            "slow",
            vec![sleeper],
            "timeout_seconds = 1",
            "timeout",
            "absent",
        ),
        ("flood", vec![flood], "", "output-bound", "absent"),
        ("garbage", vec![garbage], "", "parse", "refused"),
        ("binary", vec![binary], "", "parse", "refused"),
    ];
    for (id, command, extra, kind, dataset) in cases {
        let argv: Vec<&str> = command.iter().map(String::as_str).collect();
        let config = scratch.config(
            &[
                source_toml(id, &argv, extra),
                card_toml("c", id, "time-series", "amount", ""),
            ]
            .concat(),
        );
        let out = run(&["charts", "run", "--config", &config, "--source", id]);
        assert_eq!(out.code, 1, "{id}: {}", out.stdout);
        assert_eq!(out.source(id)["error"]["kind"], kind, "{id}");
        assert_eq!(out.source(id)["dataset"]["status"], dataset, "{id}");
        assert_eq!(out.card("c")["outcome"]["kind"], "unavailable", "{id}");
        assert_eq!(out.card("c")["outcome"]["source_error"], kind, "{id}");
        if dataset == "absent" {
            assert_eq!(out.check("execution")["status"], "fail", "{id}");
            assert_eq!(out.check("freshness")["status"], "not-run", "{id}");
        } else {
            // The command ran and exited 0; its output was refused.
            assert_eq!(out.check("execution")["status"], "pass", "{id}");
            assert_eq!(out.check("dataset")["status"], "fail", "{id}");
            assert_eq!(out.source(id)["attempt"]["exit"], 0, "{id}");
        }
    }
    assert_eq!(scratch.marker_lines("sleeper.sh"), 1);
}

#[test]
fn details_and_selectors_work_on_a_live_run() {
    let scratch = Scratch::new("details");
    let a = ok_producer(&scratch, "a.sh");
    let config = scratch.config(
        &[
            source_toml("a", &[&a], ""),
            card_toml("c", "a", "time-series", "amount", ""),
        ]
        .concat(),
    );
    let out = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--source",
        "a",
        "--details",
        "c",
        "--offset",
        "1",
        "--limit",
        "1",
    ]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    let details = &out.card("c")["details"];
    assert_eq!(details["rows_total"], 2);
    assert_eq!(details["offset"], 1);
    assert_eq!(details["rows_returned"], 1);
    assert_eq!(details["complete"], false);
    assert_eq!(scratch.marker_lines("a.sh"), 1);
    let (code, text, _) = run_text(&[
        "charts", "run", "--config", &config, "--source", "a", "--format", "text",
    ]);
    assert_eq!(code, 0);
    assert_eq!(
        text.lines().next().unwrap(),
        "sidescope charts run: ok (exit 0)"
    );
    assert!(text.contains("card c: plotted"), "{text}");
}
