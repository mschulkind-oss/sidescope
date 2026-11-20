//! The opt-in agent-activity profiles and the imported synthetic fixtures:
//! parse, separation, and agreement with the producer's capabilities answer.
//! Nothing here runs `agent-activity` or reads an account.

use serde_json::{Value, json};
use sidescope::chart::config::{ChartSourceConfig, SourceFormat};
use sidescope::{AppConfig, parse_config};
use std::process::{Command, Stdio};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const QUOTA_COLUMNS: [&str; 4] = [
    "subscriptionWeeklyPercent",
    "subscriptionWeeklyUnweighted",
    "subscriptionFiveHourPercent",
    "subscriptionFiveHourUnweighted",
];
/// Legacy profile at the commit that introduced the filtered profile.
const LEGACY_LENGTH: usize = 6986;
const LEGACY_FNV1A: u64 = 0xd07b_e417_30c2_9ee1;

fn profile_text(name: &str) -> String {
    std::fs::read_to_string(format!("{ROOT}/examples/charts/{name}.toml")).unwrap()
}

fn profile(name: &str) -> AppConfig {
    parse_config(&profile_text(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!("{ROOT}/tests/fixtures/charts/{path}")).unwrap()
}

fn capabilities(path: &str) -> Value {
    serde_json::from_slice(&fixture(path)).unwrap()
}

fn source<'a>(config: &'a AppConfig, id: &str) -> &'a ChartSourceConfig {
    config
        .chart_sources
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("source {id}"))
}

fn ids(config: &AppConfig) -> (Vec<&str>, Vec<&str>) {
    (
        config.chart_sources.iter().map(|s| s.id.as_str()).collect(),
        config.chart_cards.iter().map(|c| c.id.as_str()).collect(),
    )
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

#[test]
fn legacy_profile_bytes_are_unchanged() {
    let bytes = std::fs::read(format!("{ROOT}/examples/charts/agent-activity.toml")).unwrap();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in &bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
    }
    assert_eq!((bytes.len(), hash), (LEGACY_LENGTH, LEGACY_FNV1A));
    let text = String::from_utf8(bytes).unwrap();
    // The text `tests/chart_integration.rs` rewrites must stay byte-stable.
    assert!(text.contains("\"agent-activity\", \"ledger\", \"series\","));
    for id in ["recent", "daily", "models", "subscription"] {
        assert!(text.contains(&format!("id = \"{id}\"")));
    }
    assert!(text.contains("id = \"subscription-estimate\""));
}

#[test]
fn legacy_and_filtered_profiles_contain_no_quota_vocabulary() {
    for name in ["agent-activity", "agent-activity-filtered"] {
        let text = profile_text(name);
        for forbidden in ["quota", "Percent", "Weighted"] {
            assert!(!text.contains(forbidden), "{name} contains {forbidden}");
        }
        assert!(!text.to_lowercase().contains("quota"), "{name}");
        let config = profile(name);
        for s in &config.chart_sources {
            assert!(!s.id.contains("quota") && !s.command.iter().any(|a| a.contains("quota")));
        }
        for c in &config.chart_cards {
            assert!(!c.id.contains("quota") && !c.metric.contains("quota"));
        }
    }
}

#[test]
fn filtered_profile_has_the_documented_sources_and_cards() {
    let config = profile("agent-activity-filtered");
    let (sources, cards) = ids(&config);
    assert_eq!(
        sources,
        ["recent", "daily", "models", "subscription", "copilot-cost"]
    );
    assert_eq!(
        cards,
        [
            "recent-cost",
            "daily-cost",
            "project-ranking",
            "model-ranking",
            "subscription-activity",
            "copilot-estimate"
        ]
    );
    assert!(!cards.contains(&"subscription-estimate"));
}

#[test]
fn filtered_recent_source_is_the_designs_argv() {
    let config = profile("agent-activity-filtered");
    let recent = source(&config, "recent");
    assert_eq!(
        recent.command,
        args(&[
            "agent-activity",
            "ledger",
            "series",
            "--format",
            "json",
            "--layout",
            "columns",
            "--bucket",
            "minute",
            "--group-by",
            "none",
            "--channel",
            "metered",
            "--columns",
            "knownCostUSD,unpriced",
            "--from",
            "{from}",
            "--to",
            "{to}",
            "--timezone",
            "{timezone}",
        ])
    );
    assert_eq!(recent.window.as_deref(), Some("3h"));
    assert!(recent.mapping.as_ref().unwrap().dimensions.is_empty());
    assert_eq!(recent.format, SourceFormat::ColumnsV1);
}

#[test]
fn filtered_day_sources_select_the_columns_they_map() {
    let config = profile("agent-activity-filtered");
    let columns_of = |id: &str| {
        let command = &source(&config, id).command;
        let at = command.iter().position(|a| a == "--columns").unwrap();
        command[at + 1].clone()
    };
    assert_eq!(columns_of("daily"), "knownCostUSD,unpriced");
    assert_eq!(columns_of("models"), "knownCostUSD,unpriced");
    assert_eq!(columns_of("subscription"), "subscriptionResponses");
    assert_eq!(
        columns_of("copilot-cost"),
        "subscriptionKnownCostUSD,subscriptionUnpriced"
    );
}

#[test]
fn copilot_estimate_uses_the_copilot_subscription_channel() {
    let config = profile("agent-activity-filtered");
    let card = config
        .chart_cards
        .iter()
        .find(|c| c.id == "copilot-estimate")
        .unwrap();
    assert_eq!(card.source, "copilot-cost");
    assert!(
        card.title
            .as_deref()
            .unwrap()
            .contains("list-price estimate")
    );
    let command = &source(&config, &card.source).command;
    let at = command.iter().position(|a| a == "--channel").unwrap();
    assert_eq!(command[at + 1], "subscription:github-copilot");
    let group = command.iter().position(|a| a == "--group-by").unwrap();
    assert_eq!(command[group + 1], "none");
}

#[test]
fn filtered_money_cards_say_list_price_estimate_and_set_group_by_beside_stack_by() {
    let config = profile("agent-activity-filtered");
    for card in &config.chart_cards {
        if card.metric == "metered_cost" || card.metric == "subscription_cost" {
            assert!(
                card.title
                    .as_deref()
                    .unwrap()
                    .contains("list-price estimate"),
                "{}",
                card.id
            );
        }
        if let Some(stack) = &card.stack_by {
            assert_eq!(
                card.group_by.as_slice(),
                std::slice::from_ref(stack),
                "{}",
                card.id
            );
        }
    }
    let text = profile_text("agent-activity-filtered");
    for caveat in [
        "list-price estimate",
        "not an invoice",
        "direct API key",
        "never probes",
    ] {
        assert!(text.contains(caveat), "{caveat}");
    }
}

#[test]
fn quota_profile_has_the_documented_sources_and_cards() {
    let config = profile("agent-activity-quota");
    let (sources, cards) = ids(&config);
    assert_eq!(
        sources,
        [
            "quota-estimate",
            "quota-estimate-projects",
            "quota-estimate-recent",
            "quota-observed-primary-five_hour",
            "quota-observed-primary-seven_day"
        ]
    );
    assert_eq!(
        cards,
        [
            "quota-weekly-daily",
            "quota-weekly-by-model",
            "quota-weekly-by-project",
            "quota-five-hour-recent",
            "quota-observed-primary-five_hour",
            "quota-observed-primary-seven_day"
        ]
    );
    for s in &config.chart_sources {
        assert_eq!(s.format, SourceFormat::NativeV1, "{}", s.id);
        assert!(s.mapping.is_none(), "{}: wrappers, not mappings", s.id);
        assert!(
            s.command[0].starts_with("/REPLACE/WITH/PATH/quota-"),
            "{}",
            s.id
        );
    }
}

#[test]
fn quota_profile_commands_match_the_wrapper_contracts() {
    let config = profile("agent-activity-quota");
    let tail = [
        "--from",
        "{from}",
        "--to",
        "{to}",
        "--timezone",
        "{timezone}",
    ];
    let expect = |id: &str, exe: &str, middle: &[&str]| {
        let mut want = vec![format!("/REPLACE/WITH/PATH/{exe}")];
        want.extend(args(middle));
        want.extend(args(&tail));
        assert_eq!(source(&config, id).command, want, "{id}");
    };
    expect(
        "quota-estimate",
        "quota-estimate.py",
        &[
            "--quota",
            "weekly",
            "--bucket",
            "day",
            "--group-by",
            "model",
        ],
    );
    expect(
        "quota-estimate-projects",
        "quota-estimate.py",
        &[
            "--quota",
            "weekly",
            "--bucket",
            "day",
            "--group-by",
            "project",
        ],
    );
    expect(
        "quota-estimate-recent",
        "quota-estimate.py",
        &[
            "--quota",
            "five-hour",
            "--bucket",
            "5min",
            "--group-by",
            "none",
        ],
    );
    expect(
        "quota-observed-primary-five_hour",
        "quota-observed.py",
        &[
            "--account-key",
            "REPLACE-WITH-ACCOUNT-KEY",
            "--window",
            "five_hour",
        ],
    );
    expect(
        "quota-observed-primary-seven_day",
        "quota-observed.py",
        &[
            "--account-key",
            "REPLACE-WITH-ACCOUNT-KEY",
            "--window",
            "seven_day",
        ],
    );
    assert_eq!(source(&config, "quota-estimate").calendar_days, Some(5));
    assert_eq!(
        source(&config, "quota-estimate-recent").window.as_deref(),
        Some("5h")
    );
    assert_eq!(
        source(&config, "quota-observed-primary-five_hour")
            .window
            .as_deref(),
        Some("24h")
    );
    assert_eq!(
        source(&config, "quota-observed-primary-seven_day")
            .window
            .as_deref(),
        Some("7d")
    );
}

#[test]
fn quota_cards_never_bound_percent_and_observed_cards_headline_the_last_reading() {
    let config = profile("agent-activity-quota");
    for card in &config.chart_cards {
        assert!(!card.bounded_percent, "{}", card.id);
    }
    let card = |id: &str| config.chart_cards.iter().find(|c| c.id == id).unwrap();
    for id in [
        "quota-observed-primary-five_hour",
        "quota-observed-primary-seven_day",
    ] {
        assert_eq!(card(id).metric, "observed_percent");
        assert_eq!(card(id).source, id);
        assert!(card(id).group_by.is_empty());
    }
    assert_eq!(
        card("quota-weekly-daily").stack_by.as_deref(),
        Some("model")
    );
    assert_eq!(card("quota-weekly-daily").group_by, ["model"]);
    assert_eq!(card("quota-weekly-by-project").group_by, ["project"]);
    assert_eq!(
        card("quota-weekly-by-project").source,
        "quota-estimate-projects"
    );
    assert_eq!(
        card("quota-five-hour-recent").source,
        "quota-estimate-recent"
    );
    let text = profile_text("agent-activity-quota");
    for caveat in [
        "ledger capabilities --format json",
        "quota series --format json --window five_hour",
        "direct API",
        "estimated points are not the bar",
        "No account link is known",
        "not dollars",
    ] {
        assert!(
            text.to_lowercase().contains(&caveat.to_lowercase()),
            "missing: {caveat}"
        );
    }
}

#[test]
fn wrappers_are_executable() {
    use std::os::unix::fs::PermissionsExt;
    for name in ["quota-estimate.py", "quota-observed.py"] {
        let mode = std::fs::metadata(format!("{ROOT}/examples/charts/{name}"))
            .unwrap()
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "{name}");
    }
    for name in ["answer.sh", "flood.sh", "fail.sh"] {
        let mode = std::fs::metadata(format!("{ROOT}/tests/fixtures/charts/producers/{name}"))
            .unwrap()
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "{name}");
    }
}

#[test]
fn capabilities_fixture_lists_the_four_quota_descriptors_and_the_copy_does_not() {
    let with = capabilities("upstream/capabilities.json");
    let without = capabilities("upstream/capabilities-without-quota.json");
    for name in QUOTA_COLUMNS {
        assert!(with["series"]["columns"].get(name).is_some(), "{name}");
        assert!(without["series"]["columns"].get(name).is_none(), "{name}");
    }
    // Everything else is unchanged: the copy is the original minus those keys.
    let mut stripped = with.clone();
    for name in QUOTA_COLUMNS {
        stripped["series"]["columns"]
            .as_object_mut()
            .unwrap()
            .remove(name);
    }
    assert_eq!(stripped, without);
    // The documented check, written as the author runs it: all four are keys.
    let passes = |caps: &Value| {
        QUOTA_COLUMNS
            .iter()
            .all(|n| caps["series"]["columns"].get(n).is_some())
    };
    assert!(passes(&with));
    assert!(!passes(&without));
}

#[test]
fn filtered_metrics_agree_with_the_capabilities_descriptors() {
    let descriptors = capabilities("upstream/capabilities.json");
    let columns = &descriptors["series"]["columns"];
    let config = profile("agent-activity-filtered");
    let mut checked = 0;
    for s in &config.chart_sources {
        for metric in &s.mapping.as_ref().unwrap().metrics {
            let name = metric.values.strip_prefix("/columns/").unwrap();
            let descriptor = &columns[name];
            assert!(descriptor.is_object(), "{}: {name}", s.id);
            assert_eq!(
                descriptor["estimate"],
                json!(metric.estimate),
                "{} estimate",
                metric.id
            );
            let counter = metric
                .quality_counter
                .as_deref()
                .map(|p| p.strip_prefix("/columns/").unwrap());
            assert_eq!(
                descriptor.get("completenessColumn").and_then(Value::as_str),
                counter,
                "{} quality counter",
                metric.id
            );
            if metric.unit == "USD" {
                assert_eq!(descriptor["unit"], "USD", "{} unit", metric.id);
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 5);
}

#[test]
fn quota_estimate_unit_deliberately_differs_from_the_descriptors_percent() {
    let descriptors = capabilities("upstream/capabilities.json");
    let out = Command::new("python3")
        .arg(format!("{ROOT}/examples/charts/quota-estimate.py"))
        .args([
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
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("python3 must be installed to test the quota wrappers");
    use std::io::Write;
    let mut child = out;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&fixture("upstream/series-quota-percent.json"))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let answer: Value = serde_json::from_slice(&output.stdout).unwrap();
    let descriptor = &descriptors["series"]["columns"]["subscriptionWeeklyPercent"];
    assert_eq!(descriptor["unit"], "percent");
    assert_eq!(descriptor["estimate"], true);
    assert_eq!(
        answer["metrics"]["weekly_quota_points"]["unit"],
        "percentage points"
    );
    assert_ne!(
        answer["metrics"]["weekly_quota_points"]["unit"], descriptor["unit"],
        "a bounded-percent card must be refused for estimated points"
    );
    assert_eq!(
        answer["metrics"]["weekly_quota_points"]["estimate"],
        descriptor["estimate"]
    );
}

#[test]
fn imported_fixtures_are_listed_with_their_provenance() {
    let dir = format!("{ROOT}/tests/fixtures/charts/upstream");
    let readme = std::fs::read_to_string(format!("{dir}/README.md")).unwrap();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 14);
    for name in &names {
        assert!(
            readme.contains(name),
            "{name} is missing from the provenance README"
        );
        let value: Value = serde_json::from_slice(&std::fs::read(format!("{dir}/{name}")).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(value.is_object(), "{name}");
    }
    for commit in ["9a15507", "73a5063", "49f89cc"] {
        assert!(readme.contains(commit), "{commit}");
    }
    assert!(readme.contains("invented"));
}
