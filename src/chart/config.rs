//! Owner-authored chart configuration: validation happens when the config is
//! parsed, long before any command could run.

use super::dataset::{ChartDataset, Quantity, UnknownContribution};
use super::error::{ChartError, ChartErrorKind};
use super::reduce::Reduction;
use super::time::{
    ChartZone, Interval, MAX_CALENDAR_DAYS, Timestamp, WindowError, WindowSpec, format_utc,
    resolve_window_checked,
};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::Duration;

pub const MIN_REFRESH_SECONDS: u64 = 5;
pub const DEFAULT_REFRESH_SECONDS: u64 = 30;
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 10;
pub const STDOUT_LIMIT_BYTES: usize = 1_048_576;
pub const STDERR_LIMIT_BYTES: usize = 8_192;
pub const MAX_CONCURRENT_COMMANDS: usize = 4;
const MAX_TIMEOUT_SECONDS: u64 = 60;
const DEFAULT_STALE_FLOOR_SECONDS: u64 = 90;
const MAX_TOP_N: u8 = 16;
const PLACEHOLDERS: [&str; 3] = ["{from}", "{to}", "{timezone}"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SourceFormat {
    #[serde(rename = "sidescope-chart-v1")]
    NativeV1,
    #[serde(rename = "columns-v1")]
    ColumnsV1,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartSourceConfig {
    /// `[a-z0-9][a-z0-9_-]{0,63}`
    pub id: String,
    pub command: Vec<String>,
    pub format: SourceFormat,
    /// For example `3h`; exclusive with `calendar_days`.
    pub window: Option<String>,
    /// 1..=400, including today.
    pub calendar_days: Option<u16>,
    pub timezone: String,
    pub refresh_seconds: Option<u64>,
    pub timeout_seconds: Option<u64>,
    pub stale_after_seconds: Option<u64>,
    #[serde(default)]
    pub capture_stderr: bool,
    /// Required exactly when `format` is `columns-v1`.
    pub mapping: Option<MappingConfig>,
}

impl ChartSourceConfig {
    pub fn window_spec(&self) -> Result<WindowSpec, String> {
        match (&self.window, self.calendar_days) {
            (Some(_), Some(_)) => Err("window and calendar_days are mutually exclusive".into()),
            (None, None) => Err("set either window or calendar_days".into()),
            (Some(window), None) => {
                WindowSpec::parse_fixed(window).map_err(|error| format!("window: {error}"))
            }
            (None, Some(days)) if (1..=MAX_CALENDAR_DAYS).contains(&days) => {
                Ok(WindowSpec::CalendarDays(days))
            }
            (None, Some(_)) => Err(format!("calendar_days must be 1 to {MAX_CALENDAR_DAYS}")),
        }
    }

    pub fn zone(&self) -> Result<ChartZone, String> {
        ChartZone::parse(&self.timezone).map_err(|error| format!("timezone: {error}"))
    }

    fn refresh_value(&self) -> u64 {
        self.refresh_seconds.unwrap_or(DEFAULT_REFRESH_SECONDS)
    }

    pub fn refresh(&self) -> Duration {
        Duration::from_secs(self.refresh_value())
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS))
    }

    pub fn stale_after(&self) -> Duration {
        let default = DEFAULT_STALE_FLOOR_SECONDS.max(self.refresh_value().saturating_mul(2));
        Duration::from_secs(self.stale_after_seconds.unwrap_or(default))
    }

    pub fn executable(&self) -> &str {
        self.command.first().map_or("", String::as_str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ChartKind {
    #[serde(rename = "time-series")]
    TimeSeries,
    #[serde(rename = "time-bars")]
    TimeBars,
    #[serde(rename = "ranked-bars")]
    RankedBars,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartCardConfig {
    pub id: String,
    pub source: String,
    pub kind: ChartKind,
    pub metric: String,
    pub title: Option<String>,
    /// Default: sum for interval totals, last for gauges.
    pub summary: Option<Reduction>,
    #[serde(default)]
    pub group_by: Vec<String>,
    /// Ranked bars only, 1..=16, default 5.
    pub top_n: Option<u8>,
    /// Time bars only; one dimension.
    pub stack_by: Option<String>,
    /// Draw 0 to 100; only for metrics whose unit is `percent`.
    #[serde(default)]
    pub bounded_percent: bool,
}

/// Mapping tables. Pointers are RFC 6901; each field documents whether it is
/// absolute or relative to one group or bucket.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingConfig {
    /// Absolute: array of interval objects.
    pub buckets: String,
    /// Relative to one bucket.
    pub interval_start: String,
    /// Relative to one bucket.
    pub interval_end: String,
    /// Absolute: array of group objects.
    pub groups: String,
    /// Pointers relative to one group.
    #[serde(default)]
    pub dimensions: Vec<MappingDimension>,
    /// Absolute, optional effective bounds.
    pub range_start: Option<String>,
    pub range_end: Option<String>,
    /// Absolute, optional.
    pub generated_at: Option<String>,
    pub observed_through: Option<String>,
    /// Absolute; string or integer.
    pub revision: Option<String>,
    /// Absolute pointers to string values.
    #[serde(default)]
    pub notices: Vec<String>,
    pub metrics: Vec<MappingMetric>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingDimension {
    pub name: String,
    pub pointer: String,
    /// A producer-specific placeholder meaning "no value". It is shown as an
    /// empty (Unknown) dimension value; the series identity keeps the raw text.
    pub unknown_value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmittedPolicy {
    #[default]
    Missing,
    Zero,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingMetric {
    pub id: String,
    pub label: String,
    pub unit: String,
    pub quantity: Quantity,
    #[serde(default)]
    pub estimate: bool,
    pub unknown_contribution: Option<UnknownContribution>,
    /// Relative to one group: array aligned with the buckets.
    pub values: String,
    /// Relative to one group: aligned integers; 0 complete, above 0 partial.
    pub quality_counter: Option<String>,
    /// `zero`: an absent values or counter column means all zeros.
    #[serde(default)]
    pub omitted: OmittedPolicy,
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn check_pointer(key: &str, pointer: &str) -> Result<(), String> {
    if !pointer.is_empty() && !pointer.starts_with('/') {
        return Err(format!(
            "{key}: must be an RFC 6901 JSON pointer starting with '/'"
        ));
    }
    let mut chars = pointer.chars();
    while let Some(c) = chars.next() {
        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return Err(format!("{key}: '~' must be followed by 0 or 1"));
        }
    }
    Ok(())
}

fn check_label(key: &str, text: &str) -> Result<(), String> {
    if text.is_empty() || text.chars().count() > 128 || text.chars().any(char::is_control) {
        return Err(format!("{key}: must be 1 to 128 plain characters"));
    }
    Ok(())
}

fn validate_command(command: &[String]) -> Result<(), String> {
    let Some(executable) = command.first() else {
        return Err("command: must name an executable".into());
    };
    if executable.is_empty()
        || executable.starts_with('~')
        || executable.contains(['$', '*', '?', '|', ';', '\\', '`'])
    {
        return Err(
            "command: the executable must be a literal path or name (no ~, $, glob, pipe, ; or \\)"
                .into(),
        );
    }
    for argument in command {
        if argument.contains('\0') {
            return Err("command: arguments must not contain NUL".into());
        }
        if argument.contains(['{', '}']) && !PLACEHOLDERS.contains(&argument.as_str()) {
            return Err(
                "command: braces may only form a whole argument {from}, {to} or {timezone}".into(),
            );
        }
    }
    Ok(())
}

fn validate_mapping(mapping: &MappingConfig) -> Result<(), String> {
    check_pointer("mapping.buckets", &mapping.buckets)?;
    check_pointer("mapping.interval_start", &mapping.interval_start)?;
    check_pointer("mapping.interval_end", &mapping.interval_end)?;
    check_pointer("mapping.groups", &mapping.groups)?;
    for (key, pointer) in [
        ("mapping.range_start", &mapping.range_start),
        ("mapping.range_end", &mapping.range_end),
        ("mapping.generated_at", &mapping.generated_at),
        ("mapping.observed_through", &mapping.observed_through),
        ("mapping.revision", &mapping.revision),
    ] {
        if let Some(pointer) = pointer {
            check_pointer(key, pointer)?;
        }
    }
    for pointer in &mapping.notices {
        check_pointer("mapping.notices", pointer)?;
    }
    let mut names = HashSet::new();
    for dimension in &mapping.dimensions {
        check_label("mapping.dimensions.name", &dimension.name)?;
        check_pointer("mapping.dimensions.pointer", &dimension.pointer)?;
        if !names.insert(dimension.name.as_str()) {
            return Err("mapping.dimensions: duplicate dimension name".into());
        }
    }
    if mapping.metrics.is_empty() {
        return Err("mapping.metrics: declare at least one metric".into());
    }
    if mapping.metrics.len() > super::dataset::MAX_METRICS {
        return Err("mapping.metrics: too many metrics".into());
    }
    let mut ids = HashSet::new();
    for metric in &mapping.metrics {
        check_label("mapping.metrics.id", &metric.id)?;
        check_label("mapping.metrics.label", &metric.label)?;
        check_label("mapping.metrics.unit", &metric.unit)?;
        check_pointer("mapping.metrics.values", &metric.values)?;
        if let Some(pointer) = &metric.quality_counter {
            check_pointer("mapping.metrics.quality_counter", pointer)?;
        }
        if !ids.insert(metric.id.as_str()) {
            return Err("mapping.metrics: duplicate metric id".into());
        }
    }
    Ok(())
}

fn validate_source(source: &ChartSourceConfig) -> Result<(), String> {
    if !valid_id(&source.id) {
        return Err("id: must match [a-z0-9][a-z0-9_-]{0,63}".into());
    }
    validate_command(&source.command)?;
    source.window_spec()?;
    source.zone()?;
    let refresh = source.refresh_value();
    if refresh < MIN_REFRESH_SECONDS {
        return Err(format!(
            "refresh_seconds: must be at least {MIN_REFRESH_SECONDS}"
        ));
    }
    if let Some(timeout) = source.timeout_seconds
        && !(1..=MAX_TIMEOUT_SECONDS).contains(&timeout)
    {
        return Err(format!(
            "timeout_seconds: must be 1 to {MAX_TIMEOUT_SECONDS}"
        ));
    }
    if let Some(stale) = source.stale_after_seconds
        && stale < refresh.saturating_mul(2)
    {
        return Err("stale_after_seconds: must be at least twice refresh_seconds".into());
    }
    match (source.format, &source.mapping) {
        (SourceFormat::ColumnsV1, None) => {
            Err("mapping: columns-v1 sources require a [chart_sources.mapping] table".into())
        }
        (SourceFormat::NativeV1, Some(_)) => {
            Err("mapping: sidescope-chart-v1 sources must not set a mapping".into())
        }
        (SourceFormat::ColumnsV1, Some(mapping)) => validate_mapping(mapping),
        (SourceFormat::NativeV1, None) => Ok(()),
    }
}

fn validate_card(card: &ChartCardConfig, source: &ChartSourceConfig) -> Result<(), String> {
    if !valid_id(&card.id) {
        return Err("id: must match [a-z0-9][a-z0-9_-]{0,63}".into());
    }
    check_label("metric", &card.metric)?;
    if let Some(title) = &card.title {
        check_label("title", title)?;
    }
    if let Some(top_n) = card.top_n {
        if card.kind != ChartKind::RankedBars {
            return Err("top_n: only ranked-bars cards take top_n".into());
        }
        if !(1..=MAX_TOP_N).contains(&top_n) {
            return Err(format!("top_n: must be 1 to {MAX_TOP_N}"));
        }
    }
    if card.stack_by.is_some() && card.kind != ChartKind::TimeBars {
        return Err("stack_by: only time-bars cards stack".into());
    }
    let mut seen = HashSet::new();
    if card.group_by.iter().any(|name| !seen.insert(name)) {
        return Err("group_by: duplicate dimension".into());
    }
    // Native sources know their metrics and dimensions only from data (R4);
    // the card is checked when the first answer arrives.
    let Some(mapping) = &source.mapping else {
        return Ok(());
    };
    let metric = mapping
        .metrics
        .iter()
        .find(|metric| metric.id == card.metric)
        .ok_or("metric: not declared by the source mapping")?;
    if let Some(summary) = card.summary
        && !summary.permitted(metric.quantity)
    {
        return Err("summary: sum needs an interval_total metric, last a gauge".into());
    }
    if card.bounded_percent && metric.unit != "percent" {
        return Err("bounded_percent: only metrics with unit \"percent\" may be bounded".into());
    }
    let declared = |name: &String| mapping.dimensions.iter().any(|d| &d.name == name);
    if card.group_by.iter().any(|name| !declared(name)) {
        return Err("group_by: dimension not declared by the source mapping".into());
    }
    if let Some(name) = &card.stack_by
        && !declared(name)
    {
        return Err("stack_by: dimension not declared by the source mapping".into());
    }
    Ok(())
}

/// Validates ids, uniqueness, command argv rules (R14), windows, bounds,
/// mapping pointers and metric declarations, card references, and (for
/// `columns-v1` sources) card metric, dimension and summary choices (R4).
pub fn validate_chart_config(
    sources: &[ChartSourceConfig],
    cards: &[ChartCardConfig],
) -> Result<(), String> {
    let mut source_ids = HashSet::new();
    for (index, source) in sources.iter().enumerate() {
        validate_source(source).map_err(|error| format!("chart_sources[{index}].{error}"))?;
        if !source_ids.insert(source.id.as_str()) {
            return Err(format!("chart_sources[{index}].id: duplicate source id"));
        }
    }
    let mut card_ids = HashSet::new();
    for (index, card) in cards.iter().enumerate() {
        let source = sources
            .iter()
            .find(|source| source.id == card.source)
            .ok_or_else(|| format!("chart_cards[{index}].source: no chart source has this id"))?;
        validate_card(card, source).map_err(|error| format!("chart_cards[{index}].{error}"))?;
        if !card_ids.insert(card.id.as_str()) {
            return Err(format!("chart_cards[{index}].id: duplicate card id"));
        }
    }
    Ok(())
}

/// The inspectable request: computed, never executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRequest {
    pub source_id: String,
    /// Placeholders substituted, one argument each.
    pub argv: Vec<String>,
    /// Requested `[from, to)`.
    pub window: Interval,
    pub timezone: String,
    /// Local request revision; obsolete revisions are discarded.
    pub revision: u64,
}

/// Why a request could not be built: a configuration that cannot work, or a
/// window that is not open yet and will be at the next second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError {
    Invalid(ChartError),
    Deferred { retry_after: Duration },
}

pub fn resolve_request(
    source: &ChartSourceConfig,
    now: Timestamp,
    revision: u64,
) -> Result<ResolvedRequest, ChartError> {
    resolve_request_at(source, now, revision).map_err(|error| match error {
        RequestError::Invalid(error) => error,
        RequestError::Deferred { .. } => ChartError::new(
            ChartErrorKind::ConfigInvalid,
            "window is empty at this instant",
        ),
    })
}

pub fn resolve_request_at(
    source: &ChartSourceConfig,
    now: Timestamp,
    revision: u64,
) -> Result<ResolvedRequest, RequestError> {
    let config_error = |message: String| {
        RequestError::Invalid(ChartError::new(ChartErrorKind::ConfigInvalid, message))
    };
    let spec = source.window_spec().map_err(config_error)?;
    let zone = source.zone().map_err(config_error)?;
    let request_instant = Timestamp::from_second(now.as_second())
        .map_err(|_| config_error("request instant is out of range".into()))?;
    let window =
        resolve_window_checked(&spec, &zone, request_instant).map_err(|error| match error {
            WindowError::Invalid(message) => config_error(message),
            WindowError::NotYetPositive => RequestError::Deferred {
                retry_after: Duration::from_secs(1),
            },
        })?;
    let argv = source
        .command
        .iter()
        .map(|argument| match argument.as_str() {
            "{from}" => format_utc(window.start),
            "{to}" => format_utc(window.end),
            "{timezone}" => zone.name().to_string(),
            other => other.to_string(),
        })
        .collect();
    Ok(ResolvedRequest {
        source_id: source.id.clone(),
        argv,
        window,
        timezone: zone.name().to_string(),
        revision,
    })
}

/// Pure normalizer bound by integration; runtime tests inject a native-only one.
pub type Normalizer =
    fn(&ChartSourceConfig, &str, &ResolvedRequest) -> Result<ChartDataset, ChartError>;
