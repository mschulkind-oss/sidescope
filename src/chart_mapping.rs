//! Generic `columns-v1` mapper: aligned JSON arrays selected by RFC 6901
//! pointers become a validated chart dataset.
//!
//! The mapper has no expressions, no arithmetic and no knowledge of any
//! producer's field names. It renames dimensions, converts decimals, applies
//! the declared missing-column policy and reads one quality counter per
//! metric (zero complete, positive partial). Error messages name mapping keys
//! only and never carry data values.

use crate::chart::config::{
    ChartSourceConfig, MappingConfig, MappingMetric, OmittedPolicy, ResolvedRequest, SourceFormat,
};
use crate::chart::dataset::{
    Cell, ChartDataset, DatasetParts, MAX_CELLS, MAX_INTERVALS, MAX_METRICS, MAX_SERIES,
    MetricDescriptor, Quality, Series, UnknownContribution, parse_native_snapshot,
};
use crate::chart::decimal::ChartDecimal;
use crate::chart::error::{ChartError, ChartErrorKind};
use crate::chart::time::{ChartZone, Interval, Timestamp, parse_instant};
use serde_json::Value;
use std::collections::BTreeMap;

fn mapping_error(key: &str, problem: &str) -> ChartError {
    ChartError::new(ChartErrorKind::InvalidMapping, format!("{key}: {problem}"))
}

fn dataset_error(key: &str, problem: &str) -> ChartError {
    ChartError::new(ChartErrorKind::InvalidDataset, format!("{key}: {problem}"))
}

fn target<'a>(root: &'a Value, pointer: &str, key: &str) -> Result<&'a Value, ChartError> {
    root.pointer(pointer)
        .ok_or_else(|| mapping_error(key, "pointer finds no value"))
}

fn array_at<'a>(root: &'a Value, pointer: &str, key: &str) -> Result<&'a Vec<Value>, ChartError> {
    target(root, pointer, key)?
        .as_array()
        .ok_or_else(|| mapping_error(key, "value is not an array"))
}

fn instant_value(value: &Value, key: &str) -> Result<Timestamp, ChartError> {
    let text = value
        .as_str()
        .ok_or_else(|| mapping_error(key, "value is not a string"))?;
    parse_instant(text).map_err(|_| dataset_error(key, "not a valid RFC 3339 instant"))
}

fn instant_at(root: &Value, pointer: &str, key: &str) -> Result<Timestamp, ChartError> {
    instant_value(target(root, pointer, key)?, key)
}

/// `Ok(None)` when the pointer's target is an explicit `null`.
fn nullable_instant_at(
    root: &Value,
    pointer: &str,
    key: &str,
) -> Result<Option<Timestamp>, ChartError> {
    match target(root, pointer, key)? {
        Value::Null => Ok(None),
        value => instant_value(value, key).map(Some),
    }
}

fn revision_at(root: &Value, pointer: &str) -> Result<Option<String>, ChartError> {
    match target(root, pointer, "mapping.revision")? {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text.clone())),
        Value::Number(number) if number.is_i64() || number.is_u64() => Ok(Some(number.to_string())),
        _ => Err(mapping_error(
            "mapping.revision",
            "value is not a string or integer",
        )),
    }
}

/// One value column of one group, before cells are built.
enum Column<'a> {
    /// The pointer found nothing.
    Absent,
    /// An explicit `null` column.
    Null,
    Values(&'a Vec<Value>),
}

fn column<'a>(group: &'a Value, pointer: &str, key: &str) -> Result<Column<'a>, ChartError> {
    match group.pointer(pointer) {
        None => Ok(Column::Absent),
        Some(Value::Null) => Ok(Column::Null),
        Some(Value::Array(items)) => Ok(Column::Values(items)),
        Some(_) => Err(mapping_error(key, "value is not an array")),
    }
}

/// An omitted leaf may be read as the declared zero only when its immediate
/// parent really exists as a container: an object (any token, the empty key
/// included) or an array (a canonical numeric index only). A missing, `null`
/// or scalar parent, or a non-index token against an array, is a wrong shape,
/// never an omitted optional leaf. A pointer whose parent is the group itself
/// is unchanged.
fn require_container_parent(group: &Value, pointer: &str, key: &str) -> Result<(), ChartError> {
    // A `/` inside a token is always written `~1`, so the last literal `/`
    // separates the leaf token from its parent pointer.
    let Some((parent_pointer, leaf)) = pointer.rsplit_once('/') else {
        return Ok(());
    };
    if parent_pointer.is_empty() {
        return Ok(());
    }
    match group.pointer(parent_pointer) {
        Some(Value::Object(_)) => Ok(()),
        Some(Value::Array(_)) => {
            let canonical = leaf == "0"
                || (leaf.starts_with(|c: char| c.is_ascii_digit() && c != '0')
                    && leaf.chars().all(|c| c.is_ascii_digit()));
            if canonical && leaf.parse::<usize>().is_ok() {
                Ok(())
            } else {
                Err(mapping_error(
                    key,
                    "pointer names a non-index token inside an array",
                ))
            }
        }
        _ => Err(mapping_error(
            key,
            "pointer's parent is missing or not an object or array",
        )),
    }
}

fn decimal_cell(value: &Value, key: &str) -> Result<Option<ChartDecimal>, ChartError> {
    let text = match value {
        Value::Null => return Ok(None),
        Value::String(text) => text.clone(),
        Value::Number(number) if number.is_i64() || number.is_u64() => number.to_string(),
        _ => {
            return Err(dataset_error(
                key,
                "values must be decimal strings or integers",
            ));
        }
    };
    ChartDecimal::parse(&text)
        .map(Some)
        .map_err(|_| dataset_error(key, "value is not a valid bounded decimal"))
}

/// What the counter says about each cell.
enum Counter {
    /// No counter configured: present values are complete.
    NotConfigured,
    /// Per cell: `None` is unknown, `Some(n)` is the count of incomplete responses.
    Known(Vec<Option<u64>>),
}

fn counter_cells(
    group: &Value,
    metric: &MappingMetric,
    buckets: usize,
) -> Result<Counter, ChartError> {
    let Some(pointer) = &metric.quality_counter else {
        return Ok(Counter::NotConfigured);
    };
    let key = format!("mapping.metrics.{}.quality_counter", metric.id);
    match column(group, pointer, &key)? {
        Column::Absent => Ok(Counter::Known(match metric.omitted {
            OmittedPolicy::Zero => {
                require_container_parent(group, pointer, &key)?;
                vec![Some(0); buckets]
            }
            OmittedPolicy::Missing => vec![None; buckets],
        })),
        Column::Null => Ok(Counter::Known(vec![None; buckets])),
        Column::Values(items) => {
            if items.len() != buckets {
                return Err(dataset_error(
                    &key,
                    "column length differs from the buckets",
                ));
            }
            items
                .iter()
                .map(|item| match item {
                    Value::Null => Ok(None),
                    Value::Number(number) => number.as_u64().map(Some).ok_or_else(|| {
                        dataset_error(&key, "counters must be nonnegative integers")
                    }),
                    _ => Err(dataset_error(&key, "counters must be nonnegative integers")),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Counter::Known)
        }
    }
}

fn metric_cells(
    group: &Value,
    metric: &MappingMetric,
    buckets: usize,
) -> Result<Vec<Cell>, ChartError> {
    let key = format!("mapping.metrics.{}.values", metric.id);
    let values: Vec<Option<ChartDecimal>> = match column(group, &metric.values, &key)? {
        Column::Absent => match metric.omitted {
            OmittedPolicy::Zero => {
                require_container_parent(group, &metric.values, &key)?;
                vec![Some(ChartDecimal::zero()); buckets]
            }
            OmittedPolicy::Missing => vec![None; buckets],
        },
        Column::Null => vec![None; buckets],
        Column::Values(items) => {
            if items.len() != buckets {
                return Err(dataset_error(
                    &key,
                    "column length differs from the buckets",
                ));
            }
            items
                .iter()
                .map(|item| decimal_cell(item, &key))
                .collect::<Result<_, _>>()?
        }
    };
    let counter = counter_cells(group, metric, buckets)?;
    Ok(values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let quality = match (&value, &counter) {
                (None, _) => Quality::Missing,
                (Some(_), Counter::NotConfigured) => Quality::Complete,
                (Some(_), Counter::Known(counts)) => match counts[index] {
                    None => Quality::Missing,
                    Some(0) => Quality::Complete,
                    Some(_) => Quality::Partial,
                },
            };
            let value = if quality == Quality::Missing {
                None
            } else {
                value
            };
            Cell { value, quality }
        })
        .collect())
}

/// Pure: raw JSON + mapping + request becomes a dataset or a structured
/// refusal (`InvalidMapping` for pointers and shapes, `InvalidDataset` for
/// values).
pub fn map_columns(
    raw: &Value,
    mapping: &MappingConfig,
    request: &ResolvedRequest,
) -> Result<ChartDataset, ChartError> {
    let bucket_items = array_at(raw, &mapping.buckets, "mapping.buckets")?;
    let group_items = array_at(raw, &mapping.groups, "mapping.groups")?;
    if bucket_items.len() > MAX_INTERVALS
        || group_items.len() > MAX_SERIES
        || mapping.metrics.len() > MAX_METRICS
        || group_items
            .len()
            .saturating_mul(mapping.metrics.len())
            .saturating_mul(bucket_items.len())
            > MAX_CELLS
    {
        return Err(ChartError::new(
            ChartErrorKind::InvalidDataset,
            "answer exceeds the interval, series, metric or value limits",
        ));
    }
    let timezone = ChartZone::parse(&request.timezone)
        .map_err(|_| mapping_error("timezone", "not a known IANA zone"))?;
    let intervals = bucket_items
        .iter()
        .map(|bucket| {
            Ok(Interval {
                start: instant_at(bucket, &mapping.interval_start, "mapping.interval_start")?,
                end: instant_at(bucket, &mapping.interval_end, "mapping.interval_end")?,
            })
        })
        .collect::<Result<Vec<_>, ChartError>>()?;
    let first_start = intervals.first().map_or(request.window.start, |i| i.start);
    let last_end = intervals.last().map_or(request.window.end, |i| i.end);
    let range = Interval {
        start: match &mapping.range_start {
            Some(pointer) => instant_at(raw, pointer, "mapping.range_start")?,
            None => first_start,
        },
        end: match &mapping.range_end {
            Some(pointer) => instant_at(raw, pointer, "mapping.range_end")?,
            None => last_end,
        },
    };
    let generated_at = match &mapping.generated_at {
        Some(pointer) => instant_at(raw, pointer, "mapping.generated_at")?,
        None => request.window.end,
    };
    let observed_through = match &mapping.observed_through {
        Some(pointer) => nullable_instant_at(raw, pointer, "mapping.observed_through")?,
        None => None,
    };
    let revision = match &mapping.revision {
        Some(pointer) => revision_at(raw, pointer)?,
        None => None,
    };
    let notices = mapping
        .notices
        .iter()
        .map(|pointer| {
            target(raw, pointer, "mapping.notices")?
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| mapping_error("mapping.notices", "value is not a string"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let metrics = mapping
        .metrics
        .iter()
        .map(|metric| MetricDescriptor {
            id: metric.id.clone(),
            label: metric.label.clone(),
            unit: metric.unit.clone(),
            quantity: metric.quantity,
            estimate: metric.estimate,
            unknown_contribution: metric
                .unknown_contribution
                .unwrap_or(UnknownContribution::Unknown),
            sampled: false,
        })
        .collect();
    let series = group_items
        .iter()
        .map(|group| {
            let mut dimensions = BTreeMap::new();
            let mut tuple = Vec::with_capacity(mapping.dimensions.len());
            for dimension in &mapping.dimensions {
                let value = group
                    .pointer(&dimension.pointer)
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        mapping_error("mapping.dimensions", "pointer finds no string value")
                    })?;
                tuple.push(value.to_string());
                let shown = if dimension.unknown_value.as_deref() == Some(value) {
                    ""
                } else {
                    value
                };
                dimensions.insert(dimension.name.clone(), shown.to_string());
            }
            let mut by_metric = BTreeMap::new();
            for metric in &mapping.metrics {
                by_metric.insert(
                    metric.id.clone(),
                    metric_cells(group, metric, intervals.len())?,
                );
            }
            Ok(Series {
                // A JSON array of the ordered values cannot collide the way a
                // delimiter-joined string can.
                id: serde_json::to_string(&tuple).unwrap_or_default(),
                dimensions,
                metrics: by_metric,
                breaks: BTreeMap::new(),
            })
        })
        .collect::<Result<Vec<_>, ChartError>>()?;
    ChartDataset::new(
        DatasetParts {
            generated_at,
            observed_through,
            revision,
            range,
            timezone,
            intervals,
            metrics,
            series,
            notices,
        },
        request,
    )
}

/// Bound by integration as the runtime's `Normalizer`: native output goes to
/// the native parser; column output is parsed as JSON (malformed is a
/// `ParseFailure`) and mapped.
pub fn normalize_output(
    source: &ChartSourceConfig,
    stdout: &str,
    request: &ResolvedRequest,
) -> Result<ChartDataset, ChartError> {
    match source.format {
        SourceFormat::NativeV1 => parse_native_snapshot(stdout, request),
        SourceFormat::ColumnsV1 => {
            let raw: Value = serde_json::from_str(stdout).map_err(|_| {
                ChartError::new(ChartErrorKind::ParseFailure, "output is not valid JSON")
            })?;
            let mapping = source
                .mapping
                .as_ref()
                .ok_or_else(|| mapping_error("mapping", "columns-v1 sources need a mapping"))?;
            map_columns(&raw, mapping, request)
        }
    }
}
