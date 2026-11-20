//! One description of every chart configuration key, payload version, bound
//! and report enumeration. `charts schema`, chart help and the key matrix read
//! these tables; chart validation reads the same constants. Nothing here
//! validates anything: it is the catalog a person or an agent reads.

use super::config::{
    DEFAULT_REFRESH_SECONDS, DEFAULT_STALE_FLOOR_SECONDS, DEFAULT_TIMEOUT_SECONDS,
    MAX_CONCURRENT_COMMANDS, MAX_TIMEOUT_SECONDS, MAX_TOP_N, MIN_REFRESH_SECONDS,
    STDERR_LIMIT_BYTES, STDOUT_LIMIT_BYTES,
};
use super::dataset::{
    MAX_CELLS, MAX_INTERVALS, MAX_METRICS, MAX_SERIES, MAX_VISIBLE_SERIES, NATIVE_PAYLOAD_VERSIONS,
};
use super::error::ChartErrorKind;
use super::time::{MAX_CALENDAR_DAYS, MAX_FIXED_WINDOW_MINUTES};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::{Value, json};

pub const SCHEMA_ID: &str = "sidescope-chart-schema-v1";
pub const REPORT_SCHEMA_ID: &str = "sidescope-chart-report-v1";
pub const CAPTURE_SCHEMA_ID: &str = "sidescope-chart-capture-v1";
/// A serialized report never exceeds this many bytes.
pub const REPORT_LIMIT_BYTES: usize = 1_048_576;
pub const DETAILS_DEFAULT_LIMIT: usize = 500;
pub const DETAILS_MAX_LIMIT: usize = 5_000;

/// Check ids in report order, each with whether an `unknown` makes the status
/// `undetermined`.
pub const CHECKS: [(&str, bool); 8] = [
    ("config", true),
    ("request", true),
    ("execution", true),
    ("dataset", true),
    ("cards", true),
    ("freshness", true),
    ("coverage", false),
    ("appearance", false),
];
pub const STATUSES: [&str; 4] = ["ok", "findings", "invalid", "undetermined"];
pub const CHECK_STATUSES: [&str; 4] = ["pass", "fail", "not-run", "unknown"];
pub const DATA_ORIGINS: [&str; 3] = ["none", "replay", "run"];
pub const REPORT_ERROR_IDS: [&str; 11] = [
    "usage",
    "config-unreadable",
    "invalid-config",
    "unknown-source",
    "unknown-card",
    "conflicting-selection",
    "empty-scope",
    "data-unreadable",
    "capture-path",
    "capture-write",
    "report-too-large",
];
pub const OUTCOME_KINDS: [&str; 7] = [
    "not-evaluated",
    "plotted",
    "no-observations",
    "no-intervals",
    "no-known-values",
    "refused",
    "unavailable",
];
pub const REFUSAL_REASONS: [&str; 2] = ["card-mismatch", "capacity"];
pub const MISMATCH_SUBJECTS: [&str; 8] = [
    "metric",
    "dimension",
    "summary",
    "bounded-percent",
    "grouping",
    "stacking",
    "sampled",
    "gauge-merge",
];
pub const CAPACITY_LIMITS: [&str; 3] = ["unknown-series", "gauge-series", "numeric-range"];
pub const QUALITIES: [&str; 3] = ["complete", "partial", "missing"];
pub const BOUNDS: [&str; 3] = ["exact", "at-least", "incomplete"];
pub const ROW_ROLES: [&str; 3] = ["named", "other", "unknown"];
pub const DETAIL_GRAINS: [&str; 2] = ["interval-group", "group"];
pub const EXIT_CODES: [(i32, &str); 4] = [
    (0, "ok"),
    (1, "findings"),
    (2, "invalid"),
    (3, "undetermined"),
];

/// A key's pointer scope: how its JSON pointer is resolved.
pub const SCOPE_ABSOLUTE: &str = "absolute";
pub const SCOPE_BUCKET: &str = "bucket";
pub const SCOPE_GROUP: &str = "group";

#[derive(Debug, Clone, Copy)]
pub struct KeyDescriptor {
    pub name: &'static str,
    /// `string`, `integer`, `boolean`, `string-array`, `table`, `table-array`
    /// or `enum`.
    pub kind: &'static str,
    pub required: bool,
    pub default: Option<&'static str>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub choices: &'static [&'static str],
    /// For a JSON pointer key: `absolute`, `bucket` or `group`.
    pub scope: Option<&'static str>,
    /// A TOML value that is valid for this key on its own.
    pub example: &'static str,
    pub help: &'static str,
}

const fn key(
    name: &'static str,
    kind: &'static str,
    required: bool,
    example: &'static str,
    help: &'static str,
) -> KeyDescriptor {
    KeyDescriptor {
        name,
        kind,
        required,
        default: None,
        min: None,
        max: None,
        choices: &[],
        scope: None,
        example,
        help,
    }
}

const fn pointer(
    name: &'static str,
    required: bool,
    scope: &'static str,
    example: &'static str,
    help: &'static str,
) -> KeyDescriptor {
    KeyDescriptor {
        scope: Some(scope),
        ..key(name, "string", required, example, help)
    }
}

pub const SOURCE_KEYS: [KeyDescriptor; 11] = [
    key(
        "id",
        "string",
        true,
        "\"recent\"",
        "Unique source id, [a-z0-9][a-z0-9_-]{0,63}; cards name it.",
    ),
    key(
        "command",
        "string-array",
        true,
        "[\"example-chart-data\", \"--from\", \"{from}\", \"--to\", \"{to}\", \"--timezone\", \"{timezone}\"]",
        "One argv, no shell. {from}, {to} and {timezone} must each be a whole argument.",
    ),
    KeyDescriptor {
        choices: &["sidescope-chart-v1", "columns-v1"],
        ..key(
            "format",
            "enum",
            true,
            "\"columns-v1\"",
            "Native payload, or producer JSON read through a mapping table.",
        )
    },
    key(
        "window",
        "string",
        false,
        "\"3h\"",
        "Whole-minute duration such as 90m, 3h or 2d, at most 30 days (43200 minutes). Exclusive with calendar_days.",
    ),
    KeyDescriptor {
        min: Some(1),
        max: Some(MAX_CALENDAR_DAYS as i64),
        ..key(
            "calendar_days",
            "integer",
            false,
            "5",
            "Calendar days including today in the source timezone. Exclusive with window.",
        )
    },
    key(
        "timezone",
        "string",
        true,
        "\"UTC\"",
        "IANA zone; defines calendar days and axis labels.",
    ),
    KeyDescriptor {
        default: Some("30"),
        min: Some(MIN_REFRESH_SECONDS as i64),
        ..key(
            "refresh_seconds",
            "integer",
            false,
            "30",
            "Seconds between scheduled runs; charts run executes once and ignores it.",
        )
    },
    KeyDescriptor {
        default: Some("10"),
        min: Some(1),
        max: Some(MAX_TIMEOUT_SECONDS as i64),
        ..key(
            "timeout_seconds",
            "integer",
            false,
            "10",
            "Seconds before the command's process group is stopped.",
        )
    },
    KeyDescriptor {
        default: Some("max(90, 2 * refresh_seconds)"),
        ..key(
            "stale_after_seconds",
            "integer",
            false,
            "90",
            "Seconds after the last success before data counts as stale; at least twice refresh_seconds.",
        )
    },
    KeyDescriptor {
        default: Some("false"),
        ..key(
            "capture_stderr",
            "boolean",
            false,
            "true",
            "Keep a sanitized 8 KiB stderr excerpt; the only way stderr text reaches a report.",
        )
    },
    key(
        "mapping",
        "table",
        false,
        "see the mapping section",
        "The [chart_sources.mapping] table; required for columns-v1, refused for sidescope-chart-v1.",
    ),
];

pub const SOURCE_RULES: [&str; 5] = [
    "Set exactly one of window and calendar_days.",
    "mapping is required when format is columns-v1 and refused when it is sidescope-chart-v1.",
    "stale_after_seconds must be at least twice the effective refresh_seconds.",
    "command is one argv with a literal executable (no ~, $, glob, pipe, ; or \\); braces appear only as the whole arguments {from}, {to} and {timezone}; no argument contains NUL.",
    "Source ids are unique.",
];

pub const CARD_KEYS: [KeyDescriptor; 10] = [
    key(
        "id",
        "string",
        true,
        "\"recent-cost\"",
        "Unique card id, [a-z0-9][a-z0-9_-]{0,63}.",
    ),
    key(
        "source",
        "string",
        true,
        "\"recent\"",
        "The id of the chart source that feeds this card.",
    ),
    KeyDescriptor {
        choices: &["time-series", "time-bars", "ranked-bars"],
        ..key(
            "kind",
            "enum",
            true,
            "\"time-series\"",
            "Line over time, bars over time, or a ranking of groups.",
        )
    },
    key(
        "metric",
        "string",
        true,
        "\"metered_cost\"",
        "A metric id of the source: declared by the mapping, or named in the native payload.",
    ),
    key(
        "title",
        "string",
        false,
        "\"Recent cost\"",
        "1 to 128 plain characters; replaces the metric label as the card title.",
    ),
    KeyDescriptor {
        choices: &["sum", "last"],
        default: Some("sum for interval_total metrics, last for gauges"),
        ..key(
            "summary",
            "enum",
            false,
            "\"sum\"",
            "How the card reduces the range to one figure.",
        )
    },
    KeyDescriptor {
        default: Some("[]"),
        ..key(
            "group_by",
            "string-array",
            false,
            "[\"project\"]",
            "Dimension names that split the metric into groups; no duplicates.",
        )
    },
    KeyDescriptor {
        min: Some(1),
        max: Some(MAX_TOP_N as i64),
        default: Some("5"),
        ..key(
            "top_n",
            "integer",
            false,
            "5",
            "Ranked-bars only: named rows kept before the rest fold into Other.",
        )
    },
    key(
        "stack_by",
        "string",
        false,
        "\"model\"",
        "Time-bars only: one dimension to stack; a non-empty group_by must equal it.",
    ),
    KeyDescriptor {
        default: Some("false"),
        ..key(
            "bounded_percent",
            "boolean",
            false,
            "true",
            "Draw the axis 0 to 100; only for metrics whose unit is \"percent\".",
        )
    },
];

pub const CARD_RULES: [&str; 6] = [
    "top_n is only for ranked-bars cards; stack_by is only for time-bars cards.",
    "summary sum needs an interval_total metric and last needs a gauge.",
    "bounded_percent needs a metric whose unit is exactly \"percent\".",
    "group_by names are unique.",
    "For columns-v1 sources the metric, summary, group_by and stack_by are checked against the mapping at load; for native sources they are checked against the first answer and a mismatch is a refused card.",
    "Card ids are unique and every card names an existing source.",
];

pub const MAPPING_KEYS: [KeyDescriptor; 12] = [
    pointer(
        "buckets",
        true,
        SCOPE_ABSOLUTE,
        "\"/buckets\"",
        "Array of interval objects.",
    ),
    pointer(
        "interval_start",
        true,
        SCOPE_BUCKET,
        "\"/start\"",
        "Interval start instant, relative to one bucket.",
    ),
    pointer(
        "interval_end",
        true,
        SCOPE_BUCKET,
        "\"/end\"",
        "Interval end instant, relative to one bucket.",
    ),
    pointer(
        "groups",
        true,
        SCOPE_ABSOLUTE,
        "\"/groups\"",
        "Array of group objects.",
    ),
    KeyDescriptor {
        default: Some("[]"),
        ..key(
            "dimensions",
            "table-array",
            false,
            "see the mapping_dimension section",
            "[[chart_sources.mapping.dimensions]] tables naming dimensions read from each group.",
        )
    },
    pointer(
        "range_start",
        false,
        SCOPE_ABSOLUTE,
        "\"/range/start\"",
        "Effective range start of the answer.",
    ),
    pointer(
        "range_end",
        false,
        SCOPE_ABSOLUTE,
        "\"/range/end\"",
        "Effective range end of the answer.",
    ),
    pointer(
        "generated_at",
        false,
        SCOPE_ABSOLUTE,
        "\"/generatedAt\"",
        "When the producer made the answer.",
    ),
    pointer(
        "observed_through",
        false,
        SCOPE_ABSOLUTE,
        "\"/observedThrough\"",
        "A collection watermark; absent means coverage stays unknown.",
    ),
    pointer(
        "revision",
        false,
        SCOPE_ABSOLUTE,
        "\"/revision\"",
        "A string or integer revision, reported as a diagnostic.",
    ),
    KeyDescriptor {
        default: Some("[]"),
        scope: Some(SCOPE_ABSOLUTE),
        ..key(
            "notices",
            "string-array",
            false,
            "[\"/notice\"]",
            "Pointers to plain-text notices.",
        )
    },
    key(
        "metrics",
        "table-array",
        true,
        "see the mapping_metric section",
        "[[chart_sources.mapping.metrics]] tables, one per metric; 1 to 32.",
    ),
];

pub const MAPPING_RULES: [&str; 4] = [
    "Pointers are RFC 6901; a non-empty pointer starts with '/' and '~' is followed by 0 or 1.",
    "Pointer scope is absolute (whole answer), bucket (one element of buckets) or group (one element of groups).",
    "Dimension names and metric ids are unique; declare 1 to 32 metrics.",
    "Under omitted = \"zero\" a missing leaf is zero only when its immediate parent resolves to an object or array; an explicit null stays missing.",
];

pub const DIMENSION_KEYS: [KeyDescriptor; 3] = [
    key(
        "name",
        "string",
        true,
        "\"project\"",
        "Dimension name used by group_by and stack_by; 1 to 128 plain characters.",
    ),
    pointer(
        "pointer",
        true,
        SCOPE_GROUP,
        "\"/project\"",
        "Where the dimension value sits in one group.",
    ),
    key(
        "unknown_value",
        "string",
        false,
        "\"(unknown)\"",
        "A producer placeholder meaning no value; shown as Unknown while the series keeps the raw text.",
    ),
];

pub const METRIC_KEYS: [KeyDescriptor; 9] = [
    key(
        "id",
        "string",
        true,
        "\"metered_cost\"",
        "Metric id a card names; unique within the mapping.",
    ),
    key(
        "label",
        "string",
        true,
        "\"Metered cost\"",
        "Human label, 1 to 128 plain characters.",
    ),
    key(
        "unit",
        "string",
        true,
        "\"USD\"",
        "Unit text; \"percent\" is the only unit bounded_percent accepts.",
    ),
    KeyDescriptor {
        choices: &["interval_total", "gauge"],
        ..key(
            "quantity",
            "enum",
            true,
            "\"interval_total\"",
            "interval_total reduces by sum, gauge by last.",
        )
    },
    KeyDescriptor {
        default: Some("false"),
        ..key(
            "estimate",
            "boolean",
            false,
            "true",
            "The figure is an estimate, not a measurement.",
        )
    },
    KeyDescriptor {
        choices: &["nonnegative", "unknown"],
        ..key(
            "unknown_contribution",
            "enum",
            false,
            "\"nonnegative\"",
            "What missing inputs may add; nonnegative makes a partial sum a lower bound.",
        )
    },
    pointer(
        "values",
        true,
        SCOPE_GROUP,
        "\"/columns/cost\"",
        "Array aligned with the buckets, relative to one group.",
    ),
    KeyDescriptor {
        choices: &["missing", "zero"],
        default: Some("missing"),
        ..key(
            "omitted",
            "enum",
            false,
            "\"zero\"",
            "zero: an absent values or counter column means all zeros; missing: it is missing.",
        )
    },
    pointer(
        "quality_counter",
        false,
        SCOPE_GROUP,
        "\"/columns/incomplete_count\"",
        "Aligned integers: 0 is complete, above 0 partial; relative to one group.",
    ),
];

pub const METRIC_RULES: [&str; 3] = [
    "quantity interval_total reduces by sum; quantity gauge reduces by last.",
    "With omitted = \"zero\", an absent values column or counter column counts as zero for every bucket; an explicit null is still missing.",
    "A quality counter above zero makes the cell partial; with unknown_contribution = \"nonnegative\" a partial sum is an at-least bound.",
];

pub struct Section {
    pub name: &'static str,
    pub title: &'static str,
    pub keys: Vec<KeyDescriptor>,
    pub rules: &'static [&'static str],
}

/// Every configuration table, in the order `charts schema` lists them.
pub fn config_sections() -> Vec<Section> {
    vec![
        Section {
            name: "source",
            title: "[[chart_sources]]",
            keys: SOURCE_KEYS.to_vec(),
            rules: &SOURCE_RULES,
        },
        Section {
            name: "card",
            title: "[[chart_cards]]",
            keys: CARD_KEYS.to_vec(),
            rules: &CARD_RULES,
        },
        Section {
            name: "mapping",
            title: "[chart_sources.mapping]",
            keys: MAPPING_KEYS.to_vec(),
            rules: &MAPPING_RULES,
        },
        Section {
            name: "mapping_dimension",
            title: "[[chart_sources.mapping.dimensions]]",
            keys: DIMENSION_KEYS.to_vec(),
            rules: &[],
        },
        Section {
            name: "mapping_metric",
            title: "[[chart_sources.mapping.metrics]]",
            keys: METRIC_KEYS.to_vec(),
            rules: &METRIC_RULES,
        },
    ]
}

fn key_json(key: &KeyDescriptor) -> Value {
    json!({
        "name": key.name,
        "type": key.kind,
        "required": key.required,
        "default": key.default,
        "range": (key.min.is_some() || key.max.is_some()).then(|| json!({"min": key.min, "max": key.max})),
        "enum": (!key.choices.is_empty()).then_some(key.choices),
        "pointer_scope": key.scope,
        "example": key.example,
        "help": key.help,
    })
}

fn limits() -> Vec<(&'static str, String, &'static str)> {
    vec![
        (
            "max_intervals",
            MAX_INTERVALS.to_string(),
            "Intervals in one answer.",
        ),
        (
            "max_series",
            MAX_SERIES.to_string(),
            "Series in one answer.",
        ),
        (
            "max_metrics",
            MAX_METRICS.to_string(),
            "Metrics in one answer or mapping.",
        ),
        (
            "max_cells",
            MAX_CELLS.to_string(),
            "Cells (intervals x series x metrics) in one answer.",
        ),
        (
            "max_visible_series",
            MAX_VISIBLE_SERIES.to_string(),
            "Plotted groups per card, Other and every Unknown included.",
        ),
        (
            "stdout_bytes",
            STDOUT_LIMIT_BYTES.to_string(),
            "A command's stdout; more is an output-bound failure and nothing is kept.",
        ),
        (
            "stderr_bytes",
            STDERR_LIMIT_BYTES.to_string(),
            "Kept stderr when capture_stderr is set.",
        ),
        (
            "min_refresh_seconds",
            MIN_REFRESH_SECONDS.to_string(),
            "Smallest refresh_seconds.",
        ),
        (
            "default_refresh_seconds",
            DEFAULT_REFRESH_SECONDS.to_string(),
            "refresh_seconds when unset.",
        ),
        (
            "default_timeout_seconds",
            DEFAULT_TIMEOUT_SECONDS.to_string(),
            "timeout_seconds when unset.",
        ),
        (
            "stale_floor_seconds",
            DEFAULT_STALE_FLOOR_SECONDS.to_string(),
            "Floor of the default stale_after_seconds.",
        ),
        (
            "max_window_minutes",
            MAX_FIXED_WINDOW_MINUTES.to_string(),
            "Longest fixed window.",
        ),
        (
            "max_concurrent_commands",
            MAX_CONCURRENT_COMMANDS.to_string(),
            "Commands running at once.",
        ),
        (
            "report_bytes",
            REPORT_LIMIT_BYTES.to_string(),
            "A report larger than this becomes a report-too-large report.",
        ),
        (
            "details_default_limit",
            DETAILS_DEFAULT_LIMIT.to_string(),
            "Details rows per page when --limit is not given.",
        ),
        (
            "details_max_limit",
            DETAILS_MAX_LIMIT.to_string(),
            "Largest --limit.",
        ),
    ]
}

fn payload() -> Value {
    json!({
        "versions": NATIVE_PAYLOAD_VERSIONS
            .iter()
            .map(|version| json!({"id": version.id, "adds": version.adds}))
            .collect::<Vec<_>>(),
        "unsupported": "Any other version is refused as unsupported-version.",
        "reserved": "A payload labeled sidescope-chart-v1 that contains sampled or breaks, even as null, is refused as invalid-dataset.",
        "quantities": [
            {"quantity": "interval_total", "reduction": "sum"},
            {"quantity": "gauge", "reduction": "last"},
        ],
    })
}

fn pointer_scopes() -> Value {
    let mut rows = Vec::new();
    for section in config_sections() {
        for key in &section.keys {
            if let Some(scope) = key.scope {
                rows.push(json!({"section": section.name, "key": key.name, "scope": scope}));
            }
        }
    }
    json!(rows)
}

fn report() -> Value {
    json!({
        "schema": REPORT_SCHEMA_ID,
        "capture_schema": CAPTURE_SCHEMA_ID,
        "statuses": STATUSES,
        "exit_codes": EXIT_CODES.iter().map(|(code, status)| json!({"exit": code, "status": status})).collect::<Vec<_>>(),
        "checks": CHECKS.iter().map(|(id, required)| json!({"id": id, "required": required})).collect::<Vec<_>>(),
        "check_statuses": CHECK_STATUSES,
        "data_origins": DATA_ORIGINS,
        "error_ids": REPORT_ERROR_IDS,
        "outcome_kinds": OUTCOME_KINDS,
        "refusal_reasons": REFUSAL_REASONS,
        "mismatch_subjects": MISMATCH_SUBJECTS,
        "capacity_limits": CAPACITY_LIMITS,
        "qualities": QUALITIES,
        "bounds": BOUNDS,
        "row_roles": ROW_ROLES,
        "detail_grains": DETAIL_GRAINS,
    })
}

/// Every section by name, in listing order.
pub fn sections() -> Vec<(&'static str, Value)> {
    let mut out: Vec<(&'static str, Value)> = config_sections()
        .iter()
        .map(|section| {
            (
                section.name,
                json!({
                    "table": section.title,
                    "keys": section.keys.iter().map(key_json).collect::<Vec<_>>(),
                    "rules": section.rules,
                }),
            )
        })
        .collect();
    out.push(("payload", payload()));
    out.push((
        "limits",
        json!(
            limits()
                .into_iter()
                .map(|(name, value, help)| json!({"name": name, "value": value, "help": help}))
                .collect::<Vec<_>>()
        ),
    ));
    out.push(("pointer_scopes", pointer_scopes()));
    out.push((
        "error_kinds",
        json!(
            ChartErrorKind::SOURCE_KINDS
                .iter()
                .map(|kind| kind.id())
                .collect::<Vec<_>>()
        ),
    ));
    out.push(("report", report()));
    out
}

pub fn section_names() -> Vec<&'static str> {
    sections().into_iter().map(|(name, _)| name).collect()
}

/// The schema document; `only` narrows it to one section.
pub struct SchemaDocument {
    pub only: Option<String>,
}

impl Serialize for SchemaDocument {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Sections(Vec<(&'static str, Value)>);
        impl Serialize for Sections {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(self.0.len()))?;
                for (name, value) in &self.0 {
                    map.serialize_entry(name, value)?;
                }
                map.end()
            }
        }
        let chosen = sections()
            .into_iter()
            .filter(|(name, _)| self.only.as_deref().is_none_or(|only| only == *name))
            .collect();
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("schema", SCHEMA_ID)?;
        map.serialize_entry("report_schema", REPORT_SCHEMA_ID)?;
        map.serialize_entry("sections", &Sections(chosen))?;
        map.end()
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

fn range_text(key: &KeyDescriptor) -> String {
    match (key.min, key.max, key.choices) {
        (_, _, choices) if !choices.is_empty() => choices.join(" \\| "),
        (Some(min), Some(max), _) => format!("{min} to {max}"),
        (Some(min), None, _) => format!("at least {min}"),
        _ => String::new(),
    }
}

/// The Markdown key matrix `charts schema --format text` prints.
pub fn key_matrix() -> String {
    let mut out = String::new();
    for section in config_sections() {
        out.push_str(&format!("## `{}`\n\n", section.title));
        out.push_str("| Key | Type | Required | Default | Range or choices | Pointer scope | Example | Meaning |\n");
        out.push_str("| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |\n");
        for key in &section.keys {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | `{}` | {} |\n",
                key.name,
                key.kind,
                if key.required { "yes" } else { "no" },
                cell(key.default.unwrap_or("")),
                cell(&range_text(key)),
                key.scope.unwrap_or(""),
                cell(key.example),
                cell(key.help),
            ));
        }
        if !section.rules.is_empty() {
            out.push_str("\nRules:\n\n");
            for rule in section.rules {
                out.push_str(&format!("- {rule}\n"));
            }
        }
        out.push('\n');
    }
    out.push_str("## Payload versions\n\n| Version | Adds |\n| :--- | :--- |\n");
    for version in &NATIVE_PAYLOAD_VERSIONS {
        let adds = if version.adds.is_empty() {
            "nothing".to_string()
        } else {
            version.adds.join(", ")
        };
        out.push_str(&format!("| {} | {adds} |\n", version.id));
    }
    out.push_str("\nQuantity to reduction: interval_total is summed, gauge is read as last.\n\n");
    out.push_str("## Limits\n\n| Name | Value | Meaning |\n| :--- | :--- | :--- |\n");
    for (name, value, help) in limits() {
        out.push_str(&format!("| {name} | {value} | {} |\n", cell(help)));
    }
    out.push_str("\n## Source error kinds\n\n");
    for kind in ChartErrorKind::SOURCE_KINDS {
        out.push_str(&format!("- {}\n", kind.id()));
    }
    out.push_str("\n## Report\n\n");
    out.push_str(&format!(
        "Schema `{REPORT_SCHEMA_ID}`; statuses {}; exit codes {}.\n",
        STATUSES.join(", "),
        EXIT_CODES
            .iter()
            .map(|(code, status)| format!("{code} {status}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str(&format!(
        "\nChecks: {}.\n",
        CHECKS
            .iter()
            .map(|(id, required)| format!(
                "{id}{}",
                if *required { "" } else { " (never required)" }
            ))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out
}

/// The key tables as plain help lines, from the same descriptors.
pub fn help_tables() -> String {
    let mut out = String::new();
    for section in config_sections() {
        out.push_str(&format!("  {}\n", section.title));
        for key in &section.keys {
            let mut notes = vec![key.kind.to_string()];
            if key.required {
                notes.push("required".into());
            }
            if let Some(default) = key.default {
                notes.push(format!("default {default}"));
            }
            let range = range_text(key).replace("\\|", "|");
            if !range.is_empty() {
                notes.push(range);
            }
            if let Some(scope) = key.scope {
                notes.push(format!("pointer: {scope}"));
            }
            out.push_str(&format!(
                "    {:<22}{} — {}\n",
                key.name,
                notes.join(", "),
                key.help
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_ranges_come_from_the_validation_constants() {
        let source = |name: &str| SOURCE_KEYS.iter().find(|key| key.name == name).unwrap();
        assert_eq!(
            source("refresh_seconds").default,
            Some(DEFAULT_REFRESH_SECONDS.to_string().as_str())
        );
        assert_eq!(
            source("timeout_seconds").default,
            Some(DEFAULT_TIMEOUT_SECONDS.to_string().as_str())
        );
        assert_eq!(
            source("stale_after_seconds").default,
            Some("max(90, 2 * refresh_seconds)")
        );
        assert_eq!(DEFAULT_STALE_FLOOR_SECONDS, 90);
        let top_n = CARD_KEYS.iter().find(|key| key.name == "top_n").unwrap();
        assert_eq!(top_n.max, Some(i64::from(MAX_TOP_N)));
        assert_eq!(top_n.default, Some("5"));
    }

    #[test]
    fn every_section_names_distinct_keys() {
        for section in config_sections() {
            let mut names: Vec<_> = section.keys.iter().map(|key| key.name).collect();
            let total = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), total, "{}", section.name);
        }
    }

    #[test]
    fn the_matrix_has_a_row_for_every_key() {
        let matrix = key_matrix();
        for section in config_sections() {
            for key in &section.keys {
                assert!(
                    matrix.contains(&format!("| {} |", key.name)),
                    "{}",
                    key.name
                );
            }
        }
    }
}
