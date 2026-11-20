//! The immutable, validated chart dataset and the `sidescope-chart-v1` parser.

use super::config::ResolvedRequest;
use super::decimal::ChartDecimal;
use super::error::{ChartError, ChartErrorKind};
use super::time::{ChartZone, Interval, Timestamp, parse_instant};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub const MAX_INTERVALS: usize = 2_048;
pub const MAX_SERIES: usize = 256;
pub const MAX_METRICS: usize = 32;
pub const MAX_CELLS: usize = 100_000;
pub const MAX_VISIBLE_SERIES: usize = 16;
const MAX_TEXT_CHARS: usize = 512;
const MAX_NOTICES: usize = 32;
const NATIVE_VERSION: &str = "sidescope-chart-v1";
const NATIVE_VERSION_1_1: &str = "sidescope-chart-v1.1";

/// One accepted payload version and the optional field names it adds over v1.
/// A payload labeled with a version that does not add a name is refused when
/// it carries that name, even as `null`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadVersion {
    pub id: &'static str,
    pub adds: &'static [&'static str],
}

pub const NATIVE_PAYLOAD_VERSIONS: [PayloadVersion; 2] = [
    PayloadVersion {
        id: NATIVE_VERSION,
        adds: &[],
    },
    PayloadVersion {
        id: NATIVE_VERSION_1_1,
        adds: &["sampled", "breaks"],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quantity {
    IntervalTotal,
    Gauge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownContribution {
    Nonnegative,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Quality {
    Complete,
    Partial,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricDescriptor {
    pub id: String,
    pub label: String,
    pub unit: String,
    pub quantity: Quantity,
    pub estimate: bool,
    pub unknown_contribution: UnknownContribution,
    /// A gauge whose every value is a reading taken at its interval's start
    /// instant (`sidescope-chart-v1.1`); the interval end only orders samples.
    pub sampled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub value: Option<ChartDecimal>,
    pub quality: Quality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    pub id: String,
    pub dimensions: BTreeMap<String, String>,
    pub metrics: BTreeMap<String, Vec<Cell>>,
    /// Per sampled metric: interval indices where the line must not connect
    /// to the previous sample; strictly increasing, each in `1..intervals`.
    pub breaks: BTreeMap<String, Vec<usize>>,
}

pub struct DatasetParts {
    pub generated_at: Timestamp,
    pub observed_through: Option<Timestamp>,
    pub revision: Option<String>,
    pub range: Interval,
    pub timezone: ChartZone,
    pub intervals: Vec<Interval>,
    pub metrics: Vec<MetricDescriptor>,
    pub series: Vec<Series>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartDataset {
    generated_at: Timestamp,
    observed_through: Option<Timestamp>,
    revision: Option<String>,
    range: Interval,
    timezone: ChartZone,
    request_window: Interval,
    intervals: Vec<Interval>,
    metrics: Vec<MetricDescriptor>,
    series: Vec<Series>,
    notices: Vec<String>,
}

fn invalid(message: &str) -> ChartError {
    ChartError::new(ChartErrorKind::InvalidDataset, message)
}

fn check_text(text: &str, what: &str, allow_empty: bool) -> Result<(), ChartError> {
    if (!allow_empty && text.is_empty())
        || text.chars().count() > MAX_TEXT_CHARS
        || text.chars().any(char::is_control)
    {
        return Err(invalid(&format!(
            "{what} must be plain text of 1 to {MAX_TEXT_CHARS} characters"
        )));
    }
    Ok(())
}

impl ChartDataset {
    /// Atomic validation of every design invariant (counts first, before any
    /// derived allocation), then: cells of intervals ending after
    /// `request.window.end` that are Complete become Partial.
    pub fn new(parts: DatasetParts, request: &ResolvedRequest) -> Result<Self, ChartError> {
        let DatasetParts {
            generated_at,
            observed_through,
            revision,
            range,
            timezone,
            intervals,
            metrics,
            mut series,
            notices,
        } = parts;
        if intervals.len() > MAX_INTERVALS {
            return Err(invalid("too many intervals"));
        }
        if series.len() > MAX_SERIES {
            return Err(invalid("too many series"));
        }
        if metrics.len() > MAX_METRICS {
            return Err(invalid("too many metrics"));
        }
        if series
            .len()
            .saturating_mul(metrics.len())
            .saturating_mul(intervals.len())
            > MAX_CELLS
        {
            return Err(invalid("too many values"));
        }
        if notices.len() > MAX_NOTICES {
            return Err(invalid("too many notices"));
        }
        if range.end <= range.start {
            return Err(invalid("range must have positive duration"));
        }
        let mut previous_end: Option<Timestamp> = None;
        for interval in &intervals {
            if interval.end <= interval.start {
                return Err(invalid("interval must have positive duration"));
            }
            if interval.start < range.start || interval.end > range.end {
                return Err(invalid("interval lies outside the range"));
            }
            if previous_end.is_some_and(|end| interval.start < end) {
                return Err(invalid("intervals must be ordered and non-overlapping"));
            }
            previous_end = Some(interval.end);
        }
        if let Some(revision) = &revision {
            check_text(revision, "revision", false)?;
        }
        for notice in &notices {
            check_text(notice, "notice", false)?;
        }
        let mut metric_ids = HashSet::new();
        for metric in &metrics {
            check_text(&metric.id, "metric id", false)?;
            check_text(&metric.label, "metric label", false)?;
            check_text(&metric.unit, "metric unit", false)?;
            if !metric_ids.insert(metric.id.as_str()) {
                return Err(invalid("duplicate metric id"));
            }
        }
        let mut series_ids = HashSet::new();
        let mut tuples = HashSet::new();
        let dimension_names: Option<Vec<&String>> = series
            .first()
            .map(|first| first.dimensions.keys().collect());
        for item in &series {
            check_text(&item.id, "series id", false)?;
            if !series_ids.insert(item.id.as_str()) {
                return Err(invalid("duplicate series id"));
            }
            for (name, value) in &item.dimensions {
                check_text(name, "dimension name", false)?;
                check_text(value, "dimension value", true)?;
            }
            if dimension_names
                .as_ref()
                .is_some_and(|names| !names.iter().copied().eq(item.dimensions.keys()))
            {
                return Err(invalid("series must share the same dimension names"));
            }
            if !tuples.insert(item.dimensions.iter().collect::<Vec<_>>()) {
                return Err(invalid("two series share one dimension tuple"));
            }
            if item.metrics.len() != metrics.len()
                || metrics.iter().any(|m| !item.metrics.contains_key(&m.id))
            {
                return Err(invalid("series must carry exactly the declared metrics"));
            }
            for (metric_id, breaks) in &item.breaks {
                let sampled = metrics.iter().any(|m| &m.id == metric_id && m.sampled);
                if !sampled {
                    return Err(invalid("breaks belong only to a sampled metric"));
                }
                let ordered = breaks.windows(2).all(|pair| pair[0] < pair[1]);
                if !ordered
                    || breaks
                        .iter()
                        .any(|index| *index == 0 || *index >= intervals.len())
                {
                    return Err(invalid(
                        "breaks must be strictly increasing interval indices from 1 to the count minus 1",
                    ));
                }
            }
            for cells in item.metrics.values() {
                if cells.len() != intervals.len() {
                    return Err(invalid("metric values must align with the intervals"));
                }
                for cell in cells {
                    let consistent = match cell.quality {
                        Quality::Missing => cell.value.is_none(),
                        Quality::Complete | Quality::Partial => cell.value.is_some(),
                    };
                    if !consistent {
                        return Err(invalid("cell value does not match its quality"));
                    }
                }
            }
        }
        let sampled_ids: HashSet<&str> = metrics
            .iter()
            .filter(|metric| metric.sampled)
            .map(|metric| metric.id.as_str())
            .collect();
        for (index, interval) in intervals.iter().enumerate() {
            if interval.end > request.window.end {
                for item in &mut series {
                    for (id, cells) in item.metrics.iter_mut() {
                        // A reading at an instant is not an open bucket.
                        if sampled_ids.contains(id.as_str()) {
                            continue;
                        }
                        if cells[index].quality == Quality::Complete {
                            cells[index].quality = Quality::Partial;
                        }
                    }
                }
            }
        }
        Ok(Self {
            generated_at,
            observed_through,
            revision,
            range,
            timezone,
            request_window: request.window,
            intervals,
            metrics,
            series,
            notices,
        })
    }

    pub fn generated_at(&self) -> Timestamp {
        self.generated_at
    }

    pub fn observed_through(&self) -> Option<Timestamp> {
        self.observed_through
    }

    pub fn revision(&self) -> Option<&str> {
        self.revision.as_deref()
    }

    pub fn range(&self) -> Interval {
        self.range
    }

    pub fn timezone(&self) -> &ChartZone {
        &self.timezone
    }

    /// Retained for Info comparison with the returned range.
    pub fn request_window(&self) -> Interval {
        self.request_window
    }

    pub fn intervals(&self) -> &[Interval] {
        &self.intervals
    }

    pub fn metrics(&self) -> &[MetricDescriptor] {
        &self.metrics
    }

    pub fn metric(&self, id: &str) -> Option<&MetricDescriptor> {
        self.metrics.iter().find(|metric| metric.id == id)
    }

    pub fn series(&self) -> &[Series] {
        &self.series
    }

    pub fn dimension_names(&self) -> Vec<&str> {
        self.series
            .first()
            .map(|series| series.dimensions.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    pub fn notices(&self) -> &[String] {
        &self.notices
    }
}

/// A JSON object read as ordered entries so duplicate keys can be refused.
struct Entries<T>(Vec<(String, T)>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Entries<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EntryVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for EntryVisitor<T> {
            type Value = Entries<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, T>()? {
                    entries.push(entry);
                }
                Ok(Entries(entries))
            }
        }
        deserializer.deserialize_map(EntryVisitor(std::marker::PhantomData))
    }
}

#[derive(Deserialize)]
struct VersionProbe {
    version: Option<String>,
}

#[derive(Deserialize)]
struct RawSnapshot {
    generated_at: String,
    #[serde(default)]
    observed_through: Option<String>,
    #[serde(default)]
    revision: Option<String>,
    range: RawRange,
    intervals: Vec<RawInterval>,
    metrics: Entries<RawMetric>,
    series: Vec<RawSeries>,
    #[serde(default)]
    notices: Vec<String>,
}

#[derive(Deserialize)]
struct RawRange {
    start: String,
    end: String,
    timezone: String,
}

#[derive(Deserialize)]
struct RawInterval {
    start: String,
    end: String,
}

#[derive(Deserialize)]
struct RawMetric {
    label: String,
    unit: String,
    quantity: Quantity,
    estimate: bool,
    #[serde(default)]
    unknown_contribution: Option<UnknownContribution>,
    #[serde(default, deserialize_with = "present")]
    sampled: Option<Value>,
}

#[derive(Deserialize)]
struct RawSeries {
    id: String,
    dimensions: Entries<String>,
    metrics: Entries<RawValues>,
}

#[derive(Deserialize)]
struct RawValues {
    values: Vec<Option<String>>,
    quality: Vec<String>,
    #[serde(default, deserialize_with = "present")]
    breaks: Option<Value>,
}

/// Keeps a present `null` apart from an absent key, so reserved names can be
/// refused whatever their value.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

fn instant(text: &str, what: &str) -> Result<Timestamp, ChartError> {
    parse_instant(text).map_err(|_| invalid(&format!("{what} is not a valid RFC 3339 instant")))
}

fn data_error(error: &serde_json::Error) -> ChartError {
    if error.is_data() {
        invalid("snapshot is missing a required field or has a field of the wrong type")
    } else {
        ChartError::new(ChartErrorKind::ParseFailure, "output is not valid JSON")
    }
}

fn breaks_of(raw: &Value) -> Result<Vec<usize>, ChartError> {
    raw.as_array()
        .ok_or_else(|| invalid("breaks must be an array of interval indices"))?
        .iter()
        .map(|item| {
            item.as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .ok_or_else(|| invalid("breaks must be nonnegative integers"))
        })
        .collect()
}

/// Parses and validates `sidescope-chart-v1` and `sidescope-chart-v1.1`; any failure refuses the whole
/// snapshot.
pub fn parse_native_snapshot(
    json: &str,
    request: &ResolvedRequest,
) -> Result<ChartDataset, ChartError> {
    let probe: VersionProbe =
        serde_json::from_str(json).map_err(|error| match error.is_data() {
            true => invalid("snapshot must be a JSON object"),
            false => data_error(&error),
        })?;
    let sampling_allowed = match probe.version.as_deref() {
        Some(NATIVE_VERSION) => false,
        Some(NATIVE_VERSION_1_1) => true,
        Some(_) => {
            return Err(ChartError::new(
                ChartErrorKind::UnsupportedVersion,
                "snapshot version is not sidescope-chart-v1 or sidescope-chart-v1.1",
            ));
        }
        None => return Err(invalid("snapshot has no version")),
    };
    let raw: RawSnapshot = serde_json::from_str(json).map_err(|error| data_error(&error))?;
    if raw.intervals.len() > MAX_INTERVALS
        || raw.series.len() > MAX_SERIES
        || raw.metrics.0.len() > MAX_METRICS
        || raw
            .series
            .len()
            .saturating_mul(raw.metrics.0.len())
            .saturating_mul(raw.intervals.len())
            > MAX_CELLS
    {
        return Err(invalid(
            "snapshot exceeds the interval, series, metric or value limits",
        ));
    }
    let timezone = ChartZone::parse(&raw.range.timezone)
        .map_err(|_| invalid("range timezone is not a known IANA zone"))?;
    let intervals = raw
        .intervals
        .iter()
        .map(|interval| {
            Ok(Interval {
                start: instant(&interval.start, "interval start")?,
                end: instant(&interval.end, "interval end")?,
            })
        })
        .collect::<Result<Vec<_>, ChartError>>()?;
    let mut seen_metrics = BTreeSet::new();
    let mut metrics = Vec::with_capacity(raw.metrics.0.len());
    for (id, metric) in raw.metrics.0 {
        if !seen_metrics.insert(id.clone()) {
            return Err(invalid("duplicate metric id"));
        }
        let sampled = match (&metric.sampled, sampling_allowed) {
            (None, _) => false,
            (Some(_), false) => {
                return Err(invalid("sampled is reserved for sidescope-chart-v1.1"));
            }
            (Some(Value::Bool(true)), true) if metric.quantity == Quantity::Gauge => true,
            (Some(Value::Bool(true)), true) => {
                return Err(invalid("only a gauge metric can be sampled"));
            }
            (Some(Value::Bool(false)), true) => false,
            (Some(_), true) => return Err(invalid("sampled must be a boolean")),
        };
        metrics.push(MetricDescriptor {
            id,
            label: metric.label,
            unit: metric.unit,
            quantity: metric.quantity,
            estimate: metric.estimate,
            unknown_contribution: metric
                .unknown_contribution
                .unwrap_or(UnknownContribution::Unknown),
            sampled,
        });
    }
    let mut series = Vec::with_capacity(raw.series.len());
    for item in raw.series {
        let mut dimensions = BTreeMap::new();
        for (name, value) in item.dimensions.0 {
            if dimensions.insert(name, value).is_some() {
                return Err(invalid("duplicate dimension name"));
            }
        }
        let mut cells_by_metric = BTreeMap::new();
        let mut breaks_by_metric = BTreeMap::new();
        for (id, values) in item.metrics.0 {
            if let Some(raw_breaks) = &values.breaks {
                if !sampling_allowed {
                    return Err(invalid("breaks is reserved for sidescope-chart-v1.1"));
                }
                breaks_by_metric.insert(id.clone(), breaks_of(raw_breaks)?);
            }
            if values.values.len() != values.quality.len() {
                return Err(invalid("values and quality must have equal lengths"));
            }
            let cells = values
                .values
                .iter()
                .zip(&values.quality)
                .map(|(value, quality)| {
                    let quality = match quality.as_str() {
                        "complete" => Quality::Complete,
                        "partial" => Quality::Partial,
                        "missing" => Quality::Missing,
                        _ => return Err(invalid("unknown quality")),
                    };
                    let value = value
                        .as_deref()
                        .map(ChartDecimal::parse)
                        .transpose()
                        .map_err(|_| invalid("value is not a valid bounded decimal"))?;
                    Ok(Cell { value, quality })
                })
                .collect::<Result<Vec<_>, ChartError>>()?;
            if cells_by_metric.insert(id, cells).is_some() {
                return Err(invalid("duplicate metric entry in a series"));
            }
        }
        series.push(Series {
            id: item.id,
            dimensions,
            metrics: cells_by_metric,
            breaks: breaks_by_metric,
        });
    }
    ChartDataset::new(
        DatasetParts {
            generated_at: instant(&raw.generated_at, "generated_at")?,
            observed_through: raw
                .observed_through
                .as_deref()
                .map(|text| instant(text, "observed_through"))
                .transpose()?,
            revision: raw.revision,
            range: Interval {
                start: instant(&raw.range.start, "range start")?,
                end: instant(&raw.range.end, "range end")?,
            },
            timezone,
            intervals,
            metrics,
            series,
            notices: raw.notices,
        },
        request,
    )
}
