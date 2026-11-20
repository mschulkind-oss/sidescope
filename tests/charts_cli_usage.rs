//! Public `sidescope charts` contract: usage, selection, preconditions, check,
//! schema and help. Every test drives the real binary; producers are fake
//! executables that record their pid, so a spawn is visible as a marker line.

mod common {
    pub mod charts;
}

use common::charts::*;
use serde_json::{Value, json};
use std::os::unix::net::UnixListener;

const AT: &str = "2026-11-20T15:00:30Z";

/// Two sources, each with a card, producing the minimal native fixture.
fn two_sources(scratch: &Scratch) -> String {
    let minimal = fixture("native/minimal.json");
    let a = scratch.producer("a.sh", &cat(&minimal));
    let b = scratch.producer("b.sh", &cat(&minimal));
    let toml = [
        source_toml("a", &[&a], ""),
        source_toml("b", &[&b], ""),
        card_toml("card-a", "a", "time-series", "amount", ""),
        card_toml("card-b", "b", "time-series", "amount", ""),
    ]
    .concat();
    scratch.config(&toml)
}

fn assert_nothing_ran(scratch: &Scratch) {
    assert_eq!(scratch.marker_lines("a.sh"), 0, "a spawned");
    assert_eq!(scratch.marker_lines("b.sh"), 0, "b spawned");
}

#[test]
fn no_verb_touches_a_display() {
    let scratch = Scratch::new("nogui");
    let config = two_sources(&scratch);
    let data = fixture("native/minimal.json");
    // The scratch path is longer than a socket path may be, so name the
    // directory through its open descriptor.
    let directory = std::fs::File::open(&scratch.0).unwrap();
    let runtime_dir = format!(
        "/proc/{}/fd/{}",
        std::process::id(),
        std::os::fd::AsRawFd::as_raw_fd(&directory)
    );
    let listener = UnixListener::bind(format!("{runtime_dir}/wayland-0")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let capture = scratch.text("capture");
    let pair = format!("a={data}");
    let verbs: Vec<Vec<&str>> = vec![
        vec!["charts", "schema"],
        vec!["charts", "check", "--config", &config],
        vec![
            "charts",
            "replay",
            "--config",
            &config,
            "--at",
            "2026-11-20T12:02:00Z",
            "--data",
            &pair,
        ],
        vec![
            "charts",
            "run",
            "--config",
            &config,
            "--source",
            "a",
            "--capture",
            &capture,
        ],
    ];
    for args in verbs {
        let out = run_with_env(
            &args,
            &[
                ("WAYLAND_DISPLAY", "wayland-0"),
                ("XDG_RUNTIME_DIR", &runtime_dir),
                ("DISPLAY", ""),
            ],
        );
        assert!(out.json.get("schema").is_some(), "{args:?}: {}", out.stdout);
    }
    assert!(
        matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
        "a verb connected to the display"
    );
}

#[test]
fn invalid_invocations_exit_2_spawn_nothing_and_write_nothing() {
    let scratch = Scratch::new("preconditions");
    let config = two_sources(&scratch);
    let capture = scratch.text("capture");
    std::fs::create_dir(scratch.path("has-a")).unwrap();
    std::fs::write(scratch.path("has-a/a.stdout"), "x").unwrap();
    std::fs::create_dir(scratch.path("has-manifest")).unwrap();
    std::fs::write(scratch.path("has-manifest/manifest.json"), "{}").unwrap();
    let has_a = scratch.text("has-a");
    let has_manifest = scratch.text("has-manifest");
    let missing_parent = scratch.text("no-such-parent/capture");
    let bad_config = scratch.write(
        "bad.toml",
        format!(
            "{}{}",
            source_toml("a", &["/bin/true"], "refresh_seconds = 1"),
            card_toml("card-a", "a", "time-series", "amount", "")
        ),
    );
    let empty_config = scratch.write("empty.toml", "");
    let missing_config = scratch.text("absent.toml");
    let cases: Vec<(&str, Vec<&str>, &str)> = vec![
        ("no selector", vec!["run", "--config", &config], "usage"),
        (
            "source with all-sources",
            vec!["run", "--config", &config, "--source", "a", "--all-sources"],
            "conflicting-selection",
        ),
        (
            "unknown source",
            vec!["run", "--config", &config, "--source", "nope"],
            "unknown-source",
        ),
        (
            "repeated source",
            vec!["run", "--config", &config, "--source", "a", "--source", "a"],
            "usage",
        ),
        (
            "unknown card",
            vec![
                "run",
                "--config",
                &config,
                "--all-sources",
                "--card",
                "nope",
            ],
            "unknown-card",
        ),
        (
            "card of an unselected source",
            vec![
                "run", "--config", &config, "--source", "a", "--card", "card-b",
            ],
            "conflicting-selection",
        ),
        (
            "at on run",
            vec!["run", "--config", &config, "--source", "a", "--at", AT],
            "usage",
        ),
        (
            "capture with a stdout file",
            vec![
                "run",
                "--config",
                &config,
                "--source",
                "a",
                "--capture",
                &has_a,
            ],
            "capture-path",
        ),
        (
            "capture with a manifest",
            vec![
                "run",
                "--config",
                &config,
                "--source",
                "a",
                "--capture",
                &has_manifest,
            ],
            "capture-path",
        ),
        (
            "capture parent missing",
            vec![
                "run",
                "--config",
                &config,
                "--source",
                "a",
                "--capture",
                &missing_parent,
            ],
            "capture-path",
        ),
        (
            "invalid config",
            vec!["run", "--config", &bad_config, "--source", "a"],
            "invalid-config",
        ),
        (
            "missing explicit config",
            vec!["run", "--config", &missing_config, "--all-sources"],
            "config-unreadable",
        ),
        (
            "empty config",
            vec!["run", "--config", &empty_config, "--all-sources"],
            "empty-scope",
        ),
        (
            "details of an unknown card",
            vec![
                "run",
                "--config",
                &config,
                "--all-sources",
                "--details",
                "nope",
            ],
            "unknown-card",
        ),
        (
            "limit of zero",
            vec![
                "run",
                "--config",
                &config,
                "--all-sources",
                "--details",
                "card-a",
                "--limit",
                "0",
            ],
            "usage",
        ),
        (
            "limit over the maximum",
            vec![
                "run",
                "--config",
                &config,
                "--all-sources",
                "--details",
                "card-a",
                "--limit",
                "5001",
            ],
            "usage",
        ),
        (
            "offset without details",
            vec!["run", "--config", &config, "--all-sources", "--offset", "1"],
            "usage",
        ),
        (
            "limit without details",
            vec!["run", "--config", &config, "--all-sources", "--limit", "1"],
            "usage",
        ),
        (
            "data on run",
            vec!["run", "--config", &config, "--all-sources", "--data", "a=x"],
            "usage",
        ),
        (
            "source on check",
            vec!["check", "--config", &config, "--source", "a"],
            "usage",
        ),
        (
            "details on check",
            vec!["check", "--config", &config, "--details", "card-a"],
            "usage",
        ),
        (
            "capture on replay",
            vec!["replay", "--config", &config, "--capture", "x"],
            "usage",
        ),
        ("unknown verb", vec!["frobnicate"], "usage"),
        ("no verb", vec![], "usage"),
        (
            "negative offset",
            vec!["replay", "--details", "x", "--offset", "-1"],
            "usage",
        ),
        (
            "bad at",
            vec!["check", "--config", &config, "--at", "yesterday"],
            "usage",
        ),
        (
            "repeated card",
            vec![
                "check", "--config", &config, "--card", "card-a", "--card", "card-a",
            ],
            "usage",
        ),
        (
            "bad format",
            vec!["check", "--config", &config, "--format", "yaml"],
            "usage",
        ),
        ("flag without value", vec!["check", "--config"], "usage"),
    ];
    for (name, args, expected) in cases {
        let mut full = vec!["charts"];
        full.extend(args);
        let out = run(&full);
        assert_eq!(out.code, 2, "{name}: {}", out.stdout);
        assert_eq!(out.error_ids()[0], expected, "{name}: {}", out.stdout);
        assert_eq!(out.json["status"], "invalid", "{name}");
        assert_eq!(out.json["schema"], "sidescope-chart-report-v1", "{name}");
        for key in ["checks", "sources", "cards"] {
            assert_eq!(out.json[key], json!([]), "{name}: {key}");
        }
        assert_nothing_ran(&scratch);
        assert!(
            !scratch.exists("capture"),
            "{name}: capture directory created"
        );
        assert!(!scratch.exists("no-such-parent"), "{name}: parent created");
    }
    let _ = capture;
}

#[test]
fn invalid_config_names_the_structured_key() {
    let scratch = Scratch::new("badkey");
    let config = scratch.config(&source_toml("a", &["/bin/true"], "refresh_seconds = 1"));
    let out = run(&["charts", "check", "--config", &config]);
    assert_eq!(out.code, 2);
    assert_eq!(out.json["errors"][0]["id"], "invalid-config");
    assert_eq!(
        out.json["errors"][0]["key"],
        "chart_sources[0].refresh_seconds"
    );
    assert_eq!(out.json["config"]["valid"], false);
    assert_eq!(out.json["config"]["origin"], "explicit");
    // A TOML syntax error has no key and a bounded flattened text.
    let broken = scratch.config(&"x = [\n".repeat(400));
    let out = run(&["charts", "check", "--config", &broken]);
    assert_eq!(out.error_ids()[0], "invalid-config");
    assert!(out.json["errors"][0].get("key").is_none_or(Value::is_null));
    let text = out.json["errors"][0]["text"].as_str().unwrap();
    assert!(
        text.chars().count() <= 400 && !text.contains('\n'),
        "{text:?}"
    );
}

#[test]
fn the_default_config_path_may_be_absent_and_is_an_empty_scope() {
    let scratch = Scratch::new("default-config");
    let out = run_with_env(
        &["charts", "check"],
        &[("XDG_CONFIG_HOME", &scratch.text("none"))],
    );
    assert_eq!(
        (out.code, out.error_ids()),
        (2, vec!["empty-scope".to_string()])
    );
    assert_eq!(out.json["config"]["origin"], "default");
    // The default path is the one the live application reads.
    let dir = scratch.path("home/sidescope");
    std::fs::create_dir_all(&dir).unwrap();
    let a = scratch.producer("a.sh", "exit 0");
    std::fs::write(
        dir.join("config.toml"),
        [
            source_toml("a", &[&a], ""),
            card_toml("card-a", "a", "time-series", "amount", ""),
        ]
        .concat(),
    )
    .unwrap();
    let out = run_with_env(
        &["charts", "check"],
        &[("XDG_CONFIG_HOME", &scratch.text("home"))],
    );
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(out.json["config"]["origin"], "default");
}

#[test]
fn check_validates_without_running_or_evaluating() {
    let scratch = Scratch::new("check");
    let config = two_sources(&scratch);
    let out = run(&["charts", "check", "--config", &config, "--at", AT]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(out.json["status"], "ok");
    assert_eq!(out.json["command"], "check");
    assert_eq!(out.json["clock"], json!({"at": AT, "origin": "explicit"}));
    assert_eq!(out.json["config"]["origin"], "explicit");
    assert_eq!(out.json["config"]["valid"], true);
    let ids: Vec<&str> = out.json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "config",
            "request",
            "execution",
            "dataset",
            "cards",
            "freshness",
            "coverage",
            "appearance"
        ]
    );
    for (id, expected) in [
        ("config", "pass"),
        ("request", "pass"),
        ("execution", "not-run"),
        ("dataset", "not-run"),
        ("cards", "not-run"),
        ("freshness", "not-run"),
        ("coverage", "not-run"),
        ("appearance", "not-run"),
    ] {
        assert_eq!(out.check(id)["status"], expected, "{id}");
    }
    for (id, required) in [
        ("config", true),
        ("dataset", true),
        ("coverage", false),
        ("appearance", false),
    ] {
        assert_eq!(out.check(id)["required"], required, "{id}");
    }
    for card in out.json["cards"].as_array().unwrap() {
        assert_eq!(card["outcome"]["kind"], "not-evaluated");
        assert_eq!(card["intent"]["metric"], "amount");
        assert_eq!(card["rows"], json!([]));
    }
    let source = out.source("a");
    assert_eq!(source["data_origin"], "none");
    assert_eq!(source["request"]["from"], "2026-11-20T14:58:30Z");
    assert_eq!(source["request"]["to"], AT);
    assert_eq!(source["request"]["timezone"], "UTC");
    assert_eq!(source["request"]["executable"], "a.sh");
    assert_eq!(source["request"]["argc"], 1);
    assert!(
        source["request"].get("argv").is_none(),
        "argv without --show-argv"
    );
    assert_eq!(source["attempt"], Value::Null);
    assert_eq!(source["clocks"]["model_at"], AT);
    assert_nothing_ran(&scratch);

    let shown = run(&[
        "charts",
        "check",
        "--config",
        &config,
        "--at",
        AT,
        "--show-argv",
    ]);
    assert!(shown.json["sources"][0]["request"]["argv"].is_array());
    let narrowed = run(&[
        "charts", "check", "--config", &config, "--at", AT, "--card", "card-b",
    ]);
    assert_eq!(narrowed.json["cards"].as_array().unwrap().len(), 1);
    assert_eq!(narrowed.json["sources"].as_array().unwrap().len(), 1);
    assert_eq!(narrowed.json["sources"][0]["id"], "b");
    let wall = run(&["charts", "check", "--config", &config]);
    assert_eq!(wall.json["clock"]["origin"], "wall");
}

#[test]
fn check_of_a_deferred_request_is_undetermined() {
    let scratch = Scratch::new("deferred");
    let a = scratch.producer("a.sh", "exit 0");
    let config = scratch.config(
        &[
            format!(
                "[[chart_sources]]\nid = \"a\"\ncommand = [{a:?}]\nformat = \"sidescope-chart-v1\"\ncalendar_days = 1\ntimezone = \"America/New_York\"\n"
            ),
            card_toml("card-a", "a", "time-series", "amount", ""),
        ]
        .concat(),
    );
    let out = run(&[
        "charts",
        "check",
        "--config",
        &config,
        "--at",
        "2026-11-20T05:00:00Z",
    ]);
    assert_eq!(out.code, 3, "{}", out.stdout);
    assert_eq!(out.json["status"], "undetermined");
    assert_eq!(out.check("request")["status"], "unknown");
    assert_eq!(out.source("a")["result"][1]["status"], "unknown");
    assert_eq!(scratch.marker_lines("a.sh"), 0);
}

#[test]
fn usage_and_text_formats_still_print_one_document_or_one_first_line() {
    let scratch = Scratch::new("format");
    let config = two_sources(&scratch);
    let (code, text, _) = run_text(&[
        "charts", "check", "--config", &config, "--at", AT, "--format", "text",
    ]);
    assert_eq!(code, 0);
    assert_eq!(
        text.lines().next().unwrap(),
        "sidescope charts check: ok (exit 0)"
    );
    assert!(
        text.contains("card-a") && text.contains("not-evaluated"),
        "{text}"
    );
    let (code, text, _) = run_text(&["charts", "run", "--config", &config, "--format", "text"]);
    assert_eq!(code, 2);
    assert_eq!(
        text.lines().next().unwrap(),
        "sidescope charts run: invalid (exit 2)"
    );
    assert!(text.contains("usage"));
}

// --- schema -----------------------------------------------------------------

fn unknown_key_list(error: &str) -> Vec<String> {
    let tail = error
        .split("expected one of")
        .nth(1)
        .unwrap_or_else(|| panic!("{error}"));
    tail.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn described(sections: &Value, section: &str) -> Vec<String> {
    sections[section]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn schema_describes_exactly_the_keys_validation_accepts() {
    use sidescope::chart::config::{
        ChartCardConfig, ChartSourceConfig, MappingConfig, MappingDimension, MappingMetric,
    };
    let scratch = Scratch::new("schema");
    // Reads no configuration: an invalid default file does not matter.
    let home = scratch.path("home/sidescope");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "this is not toml [").unwrap();
    let out = run_with_env(
        &["charts", "schema"],
        &[("XDG_CONFIG_HOME", &scratch.text("home"))],
    );
    assert_eq!(out.code, 0);
    assert_eq!(out.json["schema"], "sidescope-chart-schema-v1");
    assert_eq!(out.json["report_schema"], "sidescope-chart-report-v1");
    let sections = &out.json["sections"];
    for name in [
        "source",
        "card",
        "mapping",
        "mapping_dimension",
        "mapping_metric",
        "payload",
        "limits",
        "pointer_scopes",
        "error_kinds",
        "report",
    ] {
        assert!(sections.get(name).is_some(), "section {name}");
    }
    let expected = |error: String| {
        let mut keys = unknown_key_list(&error);
        keys.sort();
        keys
    };
    let sorted = |section: &str| {
        let mut keys = described(sections, section);
        keys.sort();
        keys
    };
    assert_eq!(
        sorted("source"),
        expected(
            toml::from_str::<ChartSourceConfig>("zzz = 1")
                .unwrap_err()
                .to_string()
        )
    );
    assert_eq!(
        sorted("card"),
        expected(
            toml::from_str::<ChartCardConfig>("zzz = 1")
                .unwrap_err()
                .to_string()
        )
    );
    assert_eq!(
        sorted("mapping"),
        expected(
            toml::from_str::<MappingConfig>("zzz = 1")
                .unwrap_err()
                .to_string()
        )
    );
    assert_eq!(
        sorted("mapping_dimension"),
        expected(
            toml::from_str::<MappingDimension>("zzz = 1")
                .unwrap_err()
                .to_string()
        )
    );
    assert_eq!(
        sorted("mapping_metric"),
        expected(
            toml::from_str::<MappingMetric>("zzz = 1")
                .unwrap_err()
                .to_string()
        )
    );
    // Every described key carries its example, and the example parses as the
    // typed key it documents.
    let sources = [
        (
            "source",
            "id = \"a\"\ncommand = [\"x\"]\nformat = \"columns-v1\"\nwindow = \"1h\"\ntimezone = \"UTC\"\n",
        ),
        (
            "card",
            "id = \"a\"\nsource = \"s\"\nkind = \"time-series\"\nmetric = \"m\"\n",
        ),
        (
            "mapping",
            "buckets = \"/b\"\ninterval_start = \"/s\"\ninterval_end = \"/e\"\ngroups = \"/g\"\nmetrics = []\n",
        ),
        ("mapping_dimension", "name = \"n\"\npointer = \"/p\"\n"),
        (
            "mapping_metric",
            "id = \"m\"\nlabel = \"l\"\nunit = \"u\"\nquantity = \"gauge\"\nvalues = \"/v\"\n",
        ),
    ];
    for (section, base) in sources {
        for key in sections[section]["keys"].as_array().unwrap() {
            let name = key["name"].as_str().unwrap();
            for field in ["type", "required", "help", "example"] {
                assert!(
                    key.get(field).is_some_and(|v| !v.is_null()),
                    "{section}.{name} lacks {field}"
                );
            }
            let example = key["example"].as_str().unwrap();
            if matches!(name, "mapping" | "dimensions" | "metrics") {
                continue; // tables are described by their own sections
            }
            let text = format!(
                "{}\n{name} = {example}\n",
                base.lines()
                    .filter(|l| !l.starts_with(&format!("{name} =")))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let parsed = match section {
                "source" => toml::from_str::<ChartSourceConfig>(&text).map(|_| ()),
                "card" => toml::from_str::<ChartCardConfig>(&text).map(|_| ()),
                "mapping" => toml::from_str::<MappingConfig>(&text).map(|_| ()),
                "mapping_dimension" => toml::from_str::<MappingDimension>(&text).map(|_| ()),
                _ => toml::from_str::<MappingMetric>(&text).map(|_| ()),
            };
            parsed.unwrap_or_else(|error| panic!("{section}.{name} = {example}: {error}"));
        }
    }
    // The payload section lists both versions; error kinds are the stable ids.
    let versions: Vec<&str> = sections["payload"]["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap())
        .collect();
    assert_eq!(versions, ["sidescope-chart-v1", "sidescope-chart-v1.1"]);
    let kinds: Vec<&str> = sections["error_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "invalid-config",
            "missing-executable",
            "spawn",
            "nonzero-exit",
            "timeout",
            "output-bound",
            "cancelled",
            "parse",
            "unsupported-version",
            "invalid-mapping",
            "invalid-dataset"
        ]
    );
    let scopes = sections["pointer_scopes"].to_string();
    for word in ["absolute", "bucket", "group"] {
        assert!(scopes.contains(word), "{word}");
    }
}

#[test]
fn schema_sections_select_and_reject_unknown_names() {
    let out = run(&["charts", "schema", "--section", "limits"]);
    assert_eq!(out.code, 0);
    let sections = out.json["sections"].as_object().unwrap();
    assert_eq!(sections.keys().collect::<Vec<_>>(), ["limits"]);
    let bad = run(&["charts", "schema", "--section", "nope"]);
    assert_eq!((bad.code, bad.error_ids()[0].as_str()), (2, "usage"));
    let (code, text, _) = run_text(&["charts", "schema", "--format", "text"]);
    assert_eq!(code, 0);
    assert!(
        text.contains("| refresh_seconds |") && text.contains("sidescope-chart-v1.1"),
        "{text}"
    );
}

#[test]
fn numeric_ranges_accept_the_boundary_and_reject_one_beyond() {
    let scratch = Scratch::new("ranges");
    let card = card_toml("c", "a", "ranked-bars", "amount", "");
    let check = |extra: &str, card_extra: &str| {
        let toml = [
            format!("[[chart_sources]]\nid = \"a\"\ncommand = [\"/bin/true\"]\nformat = \"sidescope-chart-v1\"\ntimezone = \"UTC\"\n{extra}\n"),
            card.replace("ranked-bars\"", &format!("ranked-bars\"\n{card_extra}")),
        ]
        .concat();
        let config = scratch.config(&toml);
        run(&["charts", "check", "--config", &config, "--at", AT]).code
    };
    let window = "window = \"1h\"\n";
    let cases: Vec<(String, String, bool)> = vec![
        (format!("{window}refresh_seconds = 5"), String::new(), true),
        (format!("{window}refresh_seconds = 4"), String::new(), false),
        (format!("{window}timeout_seconds = 1"), String::new(), true),
        (format!("{window}timeout_seconds = 60"), String::new(), true),
        (format!("{window}timeout_seconds = 0"), String::new(), false),
        (
            format!("{window}timeout_seconds = 61"),
            String::new(),
            false,
        ),
        (
            format!("{window}refresh_seconds = 10\nstale_after_seconds = 20"),
            String::new(),
            true,
        ),
        (
            format!("{window}refresh_seconds = 10\nstale_after_seconds = 19"),
            String::new(),
            false,
        ),
        ("calendar_days = 1".into(), String::new(), true),
        ("calendar_days = 400".into(), String::new(), true),
        ("calendar_days = 0".into(), String::new(), false),
        ("calendar_days = 401".into(), String::new(), false),
        ("window = \"30d\"".into(), String::new(), true),
        ("window = \"31d\"".into(), String::new(), false),
        (window.into(), "top_n = 1".into(), true),
        (window.into(), "top_n = 16".into(), true),
        (window.into(), "top_n = 0".into(), false),
        (window.into(), "top_n = 17".into(), false),
    ];
    for (source_extra, card_extra, valid) in cases {
        let code = check(&source_extra, &card_extra);
        assert_eq!(
            code == 0,
            valid,
            "{source_extra} / {card_extra}: exit {code}"
        );
        if !valid {
            assert_eq!(code, 2);
        }
    }
}

// --- help --------------------------------------------------------------------

#[test]
fn help_lists_the_tables_the_authority_warning_and_the_cookbook() {
    let (code, charts, _) = run_text(&["charts", "--help"]);
    assert_eq!(code, 0);
    for needle in [
        "schema",
        "check",
        "replay",
        "run",
        "--show-argv",
        "--format",
        "docs/chart-cookbook.md",
        "refresh_seconds",
        "top_n",
        "interval_start",
        "quality_counter",
    ] {
        assert!(charts.contains(needle), "charts --help lacks {needle}");
    }
    for verb in ["schema", "check", "replay", "run"] {
        let (code, text, _) = run_text(&["charts", verb, "--help"]);
        assert_eq!(code, 0, "{verb}");
        assert!(text.contains(&format!("sidescope charts {verb}")), "{verb}");
        assert!(text.contains("docs/chart-cookbook.md"), "{verb}");
    }
    let (_, run_help, _) = run_text(&["charts", "run", "--help"]);
    assert!(
        run_help.contains("unsandboxed") && run_help.contains("authority"),
        "{run_help}"
    );
    assert!(
        run_help.contains("--source") && run_help.contains("--capture"),
        "{run_help}"
    );
    let (_, top, _) = run_text(&["--help"]);
    assert!(top.contains("sidescope charts"), "{top}");
}

#[test]
fn doctor_and_the_legacy_report_flag_do_not_run_chart_sources() {
    let scratch = Scratch::new("legacy");
    let config = two_sources(&scratch);
    let home = scratch.path("home/sidescope");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::copy(&config, home.join("config.toml")).unwrap();
    let out = std::process::Command::new(binary())
        .arg("doctor")
        .env("XDG_CONFIG_HOME", scratch.text("home"))
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    assert!(out.status.code().is_some());
    assert_nothing_ran(&scratch);
    // `--report` keeps its legacy argument rule: it needs a path.
    let missing = std::process::Command::new(binary())
        .arg("--report")
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--report requires a file path"));
}
