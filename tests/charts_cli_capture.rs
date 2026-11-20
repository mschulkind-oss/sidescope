//! `charts run --capture` and `charts replay --from-capture`: byte-exact
//! evidence, exclusive owner-only files, and replay equal over what was kept.

mod common {
    pub mod charts;
}

use common::charts::*;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;

fn mode(path: &std::path::Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn manifest(dir: &std::path::Path) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap()
}

fn entry<'a>(manifest: &'a Value, id: &str) -> &'a Value {
    manifest["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["id"] == id)
        .unwrap()
}

/// One source and one card per `(id, producer, extra)`.
fn config_for(scratch: &Scratch, sources: &[(&str, &str, &str)]) -> String {
    let mut toml = String::new();
    for (id, producer, extra) in sources {
        toml.push_str(&source_toml(id, &[producer], extra));
        toml.push_str(&card_toml(
            &format!("{id}-card"),
            id,
            "time-series",
            "amount",
            "",
        ));
    }
    scratch.config(&toml)
}

#[test]
fn output_that_fails_to_parse_is_still_kept_byte_for_byte() {
    let scratch = Scratch::new("bytes");
    let garbage = scratch.producer("garbage.sh", "printf '%s' '{not json'");
    let binary = scratch.producer("binary.sh", "printf '\\377\\376{\\000'");
    let config = config_for(
        &scratch,
        &[("garbage", &garbage, ""), ("binary", &binary, "")],
    );
    let dir = scratch.path("capture");
    let out = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--all-sources",
        "--capture",
        &dir.to_string_lossy(),
    ]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.source("garbage")["error"]["kind"], "parse");
    assert_eq!(out.source("binary")["error"]["kind"], "parse");
    assert_eq!(
        std::fs::read(dir.join("garbage.stdout")).unwrap(),
        b"{not json"
    );
    assert_eq!(
        std::fs::read(dir.join("binary.stdout")).unwrap(),
        [0xff, 0xfe, b'{', 0]
    );
    let manifest = manifest(&dir);
    assert_eq!(manifest["schema"], "sidescope-chart-capture-v1");
    assert_eq!(manifest["complete"], true);
    for (id, bytes) in [("garbage", 9), ("binary", 4)] {
        let source = entry(&manifest, id);
        assert_eq!(source["captured"], true, "{id}");
        assert_eq!(source["outcome"], "parse", "{id}");
        assert_eq!(source["bytes"], bytes, "{id}");
    }
    assert_eq!(
        (
            mode(&dir),
            mode(&dir.join("garbage.stdout")),
            mode(&dir.join("manifest.json"))
        ),
        (0o700, 0o600, 0o600)
    );
    // No argv, stderr or environment is recorded.
    let text = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    assert!(
        !text.contains("garbage.sh") && !text.contains("argv"),
        "{text}"
    );
    // Raw bytes are not echoed by the report either.
    assert!(!out.stdout.contains("not json"));
}

#[test]
fn a_command_that_did_not_succeed_keeps_no_stdout_and_the_manifest_says_why() {
    let scratch = Scratch::new("failures");
    let plain = scratch.write("plain.txt", "#!/bin/sh\nexit 0\n");
    let exit2 = scratch.producer("exit2.sh", "echo partial; exit 2");
    let sleeper = scratch.producer("sleeper.sh", "echo partial; sleep 5");
    let flood = scratch.producer("flood.sh", "head -c 1048577 /dev/zero");
    let missing = scratch.text("no-such-program");
    let config = config_for(
        &scratch,
        &[
            ("exit2", &exit2, ""),
            ("slow", &sleeper, "timeout_seconds = 1"),
            ("flood", &flood, ""),
            ("plain", &plain, ""),
            ("missing", &missing, ""),
        ],
    );
    let dir = scratch.path("capture");
    let out = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--all-sources",
        "--capture",
        &dir.to_string_lossy(),
    ]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    let manifest = manifest(&dir);
    for (id, outcome) in [
        ("exit2", "nonzero-exit"),
        ("slow", "timeout"),
        ("flood", "output-bound"),
        ("plain", "spawn"),
        ("missing", "missing-executable"),
    ] {
        assert!(
            !dir.join(format!("{id}.stdout")).exists(),
            "{id} kept output"
        );
        let source = entry(&manifest, id);
        assert_eq!(
            (
                source["captured"].clone(),
                source["outcome"].clone(),
                source["bytes"].clone()
            ),
            (json!(false), json!(outcome), json!(0)),
            "{id}"
        );
    }
    assert_eq!(manifest["complete"], true, "nothing failed to be written");
    let files: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, ["manifest.json"]);
}

#[test]
fn a_capture_replays_to_the_same_semantic_facts_as_the_run_that_wrote_it() {
    let scratch = Scratch::new("equality");
    let minimal = fixture("native/minimal.json");
    let groups = fixture("cli/many-groups.json");
    let good = scratch.producer("good.sh", &cat(&minimal));
    let many = scratch.producer("many.sh", &cat(&groups));
    let garbage = scratch.producer("garbage.sh", "printf '%s' '{not json'");
    let broken = scratch.producer("broken.sh", "exit 2");
    let mut toml = String::new();
    toml.push_str(&source_toml("good", &[&good], ""));
    toml.push_str(&source_toml("many", &[&many], ""));
    toml.push_str(&source_toml("garbage", &[&garbage], ""));
    toml.push_str(&source_toml("broken", &[&broken], ""));
    toml.push_str(&card_toml("good-card", "good", "time-series", "amount", ""));
    toml.push_str(&card_toml(
        "many-rank",
        "many",
        "ranked-bars",
        "amount",
        "group_by = [\"project\"]\ntop_n = 4",
    ));
    toml.push_str(&card_toml(
        "many-line",
        "many",
        "time-series",
        "amount",
        "group_by = [\"project\"]",
    ));
    toml.push_str(&card_toml(
        "garbage-card",
        "garbage",
        "time-series",
        "amount",
        "",
    ));
    toml.push_str(&card_toml(
        "broken-card",
        "broken",
        "time-series",
        "amount",
        "",
    ));
    let config = scratch.config(&toml);
    let dir = scratch.path("capture");
    let live = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--all-sources",
        "--capture",
        &dir.to_string_lossy(),
        "--details",
        "many-rank",
    ]);
    assert_eq!(live.code, 1, "{}", live.stdout);
    let replayed = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--from-capture",
        &dir.to_string_lossy(),
        "--details",
        "many-rank",
    ]);
    assert_eq!(replayed.code, 1, "{}", replayed.stdout);
    // Dataset, outcome, intent, summary, intervals, groups, rows and Details agree.
    assert_eq!(semantic_facts(&live.json), semantic_facts(&replayed.json));
    assert_eq!(replayed.card("many-rank")["details"]["included"], true);
    // The recorded failure replays as unavailable with its recorded id, not as no data.
    assert_eq!(
        replayed.card("broken-card")["outcome"],
        json!({"kind": "unavailable", "source_error": "nonzero-exit"})
    );
    assert_eq!(replayed.source("broken")["error"]["kind"], "nonzero-exit");
    assert_eq!(replayed.source("garbage")["error"]["kind"], "parse");
    // Provenance differs, as it must.
    assert_eq!(replayed.json["clock"]["origin"], "recorded");
    assert_eq!(replayed.json["clock"]["at"], live.json["clock"]["at"]);
    for id in ["good", "many", "garbage", "broken"] {
        let source = replayed.source(id);
        assert_eq!(source["data_origin"], "replay", "{id}");
        assert_eq!(source["attempt"], Value::Null, "{id}");
        assert_eq!(
            source["clocks"],
            live.source(id)["clocks"],
            "{id}: recorded clocks reproduce construction"
        );
        assert_eq!(live.source(id)["data_origin"], "run");
    }
    assert_eq!(replayed.check("execution")["status"], "not-run");
    assert_eq!(replayed.check("freshness")["status"], "not-run");
    let banner = replayed.card("good-card")["display"]["banner"]
        .as_str()
        .unwrap();
    assert!(banner.contains("not live"), "{banner}");
    for marker in ["good.sh", "many.sh", "garbage.sh", "broken.sh"] {
        assert_eq!(
            scratch.marker_lines(marker),
            1,
            "{marker} ran again during replay"
        );
    }
    // Narrowing replays only what is asked.
    let one = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--from-capture",
        &dir.to_string_lossy(),
        "--card",
        "good-card",
    ]);
    // --card narrows the cards shown; every source in the capture is still evaluated,
    // so the refused and failed ones keep the exit status at 1.
    assert_eq!(one.code, 1, "{}", one.stdout);
    assert_eq!(one.json["cards"].as_array().unwrap().len(), 1);
    assert_eq!(one.json["sources"].as_array().unwrap().len(), 4);
    assert_eq!(one.check("cards")["status"], "pass");
    assert_eq!(one.check("dataset")["status"], "fail");
}

#[test]
fn an_existing_file_is_never_overwritten_even_when_it_appears_after_the_preflight() {
    let scratch = Scratch::new("exclusive");
    let dir = scratch.path("capture");
    let minimal = fixture("native/minimal.json");
    let plant = scratch.producer(
        "a.sh",
        &format!(
            "printf SENTINEL > '{}/b.stdout'\n{}",
            dir.display(),
            cat(&minimal)
        ),
    );
    let late = scratch.producer("b.sh", &format!("sleep 1\n{}", cat(&minimal)));
    let config = config_for(&scratch, &[("a", &plant, ""), ("b", &late, "")]);
    let out = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--all-sources",
        "--capture",
        &dir.to_string_lossy(),
    ]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.json["status"], "findings");
    assert_eq!(out.error_ids(), ["capture-write"]);
    assert_eq!(out.json["errors"][0]["source"], "b");
    assert_eq!(
        std::fs::read(dir.join("b.stdout")).unwrap(),
        b"SENTINEL",
        "overwritten"
    );
    let manifest = manifest(&dir);
    assert_eq!(manifest["complete"], false);
    assert_eq!(entry(&manifest, "b")["captured"], false);
    assert_eq!(entry(&manifest, "b")["outcome"], "ok");
    assert_eq!(entry(&manifest, "a")["captured"], true);
    // The full report is still printed and both cards were evaluated.
    assert_eq!(out.card("a-card")["outcome"]["kind"], "plotted");
    assert_eq!(out.card("b-card")["outcome"]["kind"], "plotted");
    // A capture that kept no stdout for an answer it cannot replay is refused.
    let replay = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--from-capture",
        &dir.to_string_lossy(),
    ]);
    assert_eq!(
        (replay.code, replay.error_ids()[0].as_str()),
        (2, "data-unreadable")
    );
}

#[test]
fn a_manifest_that_cannot_be_written_leaves_earlier_files_and_no_replayable_capture() {
    let scratch = Scratch::new("no-manifest");
    let dir = scratch.path("capture");
    let minimal = fixture("native/minimal.json");
    // A dangling symlink stands in for any reason the manifest cannot be created.
    let blocker = scratch.producer(
        "b.sh",
        &format!(
            "sleep 1\nln -s /nonexistent '{}/manifest.json'\n{}",
            dir.display(),
            cat(&minimal)
        ),
    );
    let first = scratch.producer("a.sh", &cat(&minimal));
    let config = config_for(&scratch, &[("a", &first, ""), ("b", &blocker, "")]);
    let out = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--all-sources",
        "--capture",
        &dir.to_string_lossy(),
    ]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.json["status"], "findings");
    assert_eq!(out.error_ids(), ["capture-write"]);
    assert!(
        out.json["errors"][0]["text"]
            .as_str()
            .unwrap()
            .contains("manifest.json")
    );
    assert!(
        dir.join("a.stdout").exists() && dir.join("b.stdout").exists(),
        "earlier files were deleted"
    );
    assert!(
        std::fs::read(dir.join("manifest.json")).is_err(),
        "no manifest was written"
    );
    let replay = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--from-capture",
        &dir.to_string_lossy(),
    ]);
    assert_eq!(
        (replay.code, replay.error_ids()[0].as_str()),
        (2, "data-unreadable")
    );
}

#[test]
fn a_directory_made_read_only_mid_run_is_a_capture_write_finding() {
    let scratch = Scratch::new("read-only");
    let dir = scratch.path("capture");
    let minimal = fixture("native/minimal.json");
    let first = scratch.producer("a.sh", &cat(&minimal));
    let lock = scratch.producer(
        "b.sh",
        &format!("sleep 1\nchmod 500 '{}'\n{}", dir.display(), cat(&minimal)),
    );
    let config = config_for(&scratch, &[("a", &first, ""), ("b", &lock, "")]);
    let out = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--all-sources",
        "--capture",
        &dir.to_string_lossy(),
    ]);
    let enforced = {
        // Permission bits do not bind a privileged process; probe instead of guessing.
        let probe = dir.join("probe");
        let writable = std::fs::File::create(&probe).is_ok();
        let _ = std::fs::remove_file(&probe);
        !writable
    };
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(dir.join("a.stdout").exists(), "earlier files were deleted");
    if enforced {
        assert_eq!(out.code, 1, "{}", out.stdout);
        assert_eq!(out.json["status"], "findings");
        assert!(out.error_ids().iter().all(|id| id == "capture-write"));
        assert!(!dir.join("manifest.json").exists());
        let replay = run(&[
            "charts",
            "replay",
            "--config",
            &config,
            "--from-capture",
            &dir.to_string_lossy(),
        ]);
        assert_eq!(
            (replay.code, replay.error_ids()[0].as_str()),
            (2, "data-unreadable")
        );
    } else {
        // Running privileged: every write succeeded, which is also correct.
        assert_eq!(out.code, 0, "{}", out.stdout);
        assert_eq!(manifest(&dir)["complete"], true);
    }
}

#[test]
fn a_capture_that_does_not_match_the_configuration_is_refused_before_replay() {
    let scratch = Scratch::new("mismatch");
    let minimal = fixture("native/minimal.json");
    let a = scratch.producer("a.sh", &cat(&minimal));
    let b = scratch.producer("b.sh", &cat(&minimal));
    let config = config_for(&scratch, &[("a", &a, ""), ("b", &b, "")]);
    let dir = scratch.path("capture");
    let out = run(&[
        "charts",
        "run",
        "--config",
        &config,
        "--all-sources",
        "--capture",
        &dir.to_string_lossy(),
    ]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    let dir_text = dir.to_string_lossy().into_owned();
    // A manifest naming a source this configuration lacks.
    let only_a = config_for(&scratch, &[("a", &a, "")]);
    let unknown = run(&[
        "charts",
        "replay",
        "--config",
        &only_a,
        "--from-capture",
        &dir_text,
    ]);
    assert_eq!(
        (unknown.code, unknown.error_ids()[0].as_str()),
        (2, "unknown-source")
    );
    assert_eq!(unknown.json["errors"][0]["source"], "b");
    // A config that would request another window than the one recorded.
    let other_window = scratch.config(
        &[
            source_toml("a", &[&a], "").replace("window = \"2m\"", "window = \"5m\""),
            source_toml("b", &[&b], ""),
            card_toml("a-card", "a", "time-series", "amount", ""),
            card_toml("b-card", "b", "time-series", "amount", ""),
        ]
        .concat(),
    );
    let moved = run(&[
        "charts",
        "replay",
        "--config",
        &other_window,
        "--from-capture",
        &dir_text,
    ]);
    assert_eq!(
        (moved.code, moved.error_ids()[0].as_str()),
        (2, "data-unreadable")
    );
    // A tampered stdout file no longer matches its recorded size.
    std::fs::write(dir.join("a.stdout"), "x").unwrap();
    let tampered = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--from-capture",
        &dir_text,
    ]);
    assert_eq!(
        (tampered.code, tampered.error_ids()[0].as_str()),
        (2, "data-unreadable")
    );
    // A malformed manifest is unreadable data, not a crash.
    std::fs::write(dir.join("manifest.json"), "{").unwrap();
    let malformed = run(&[
        "charts",
        "replay",
        "--config",
        &config,
        "--from-capture",
        &dir_text,
    ]);
    assert_eq!(
        (malformed.code, malformed.error_ids()[0].as_str()),
        (2, "data-unreadable")
    );
    // A manifest claiming a kept answer for a source that ended without output.
    std::fs::write(
        dir.join("manifest.json"),
        json!({"schema": "sidescope-chart-capture-v1", "complete": true, "invocation_at": "2026-11-20T12:00:00Z",
               "sources": [{"id": "a", "window": null, "timezone": "UTC",
                            "clocks": {"request_at": null, "model_at": "2026-11-20T12:00:00Z"},
                            "bytes": 0, "captured": false, "outcome": "ok"}]}).to_string(),
    )
    .unwrap();
    let claimed = run(&[
        "charts",
        "replay",
        "--config",
        &only_a,
        "--from-capture",
        &dir_text,
    ]);
    assert_eq!(
        (claimed.code, claimed.error_ids()[0].as_str()),
        (2, "data-unreadable")
    );
}
