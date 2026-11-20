//! Reductions over cells: sums that never hide missing or partial data,
//! last-known values, and series collapsing for `group_by`.

use super::dataset::{Cell, ChartDataset, Quality, Quantity};
use super::decimal::{ChartDecimal, DecimalError};
use super::error::{ChartError, ChartErrorKind};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reduction {
    Sum,
    Last,
}

impl Reduction {
    pub fn permitted(self, quantity: Quantity) -> bool {
        matches!(
            (self, quantity),
            (Self::Sum, Quantity::IntervalTotal) | (Self::Last, Quantity::Gauge)
        )
    }
}

/// Sum over cells. Missing if ANY cell is missing; Partial if none are missing
/// and any is partial. `known` is the exact sum of present values (`None` when
/// no cell has a value).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sum {
    pub known: Option<ChartDecimal>,
    pub quality: Quality,
    pub missing: usize,
    pub partial: usize,
    pub cells: usize,
}

pub fn sum_cells<'a>(cells: impl IntoIterator<Item = &'a Cell>) -> Result<Sum, DecimalError> {
    let mut sum = Sum {
        known: None,
        quality: Quality::Complete,
        missing: 0,
        partial: 0,
        cells: 0,
    };
    for cell in cells {
        sum.cells += 1;
        match cell.quality {
            Quality::Missing => sum.missing += 1,
            Quality::Partial => sum.partial += 1,
            Quality::Complete => {}
        }
        if let Some(value) = &cell.value {
            sum.known = Some(match &sum.known {
                Some(total) => total.checked_add(value)?,
                None => value.clone(),
            });
        }
    }
    sum.quality = if sum.missing > 0 {
        Quality::Missing
    } else if sum.partial > 0 {
        Quality::Partial
    } else {
        Quality::Complete
    };
    Ok(sum)
}

/// Last cell carrying a value; `is_latest` is false when any later cell is
/// missing ("last known").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastKnown {
    pub index: usize,
    pub value: ChartDecimal,
    pub quality: Quality,
    pub is_latest: bool,
}

pub fn last_known(cells: &[Cell]) -> Option<LastKnown> {
    let index = cells.iter().rposition(|cell| cell.value.is_some())?;
    let cell = &cells[index];
    Some(LastKnown {
        index,
        value: cell.value.clone()?,
        quality: cell.quality,
        is_latest: index + 1 == cells.len(),
    })
}

/// Cells of a merged series. A merged cell keeps the exact known subtotal of
/// its parts, so a `Missing` merged cell can still carry a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupedSeries {
    pub key: Vec<(String, String)>,
    pub cells: Vec<Cell>,
}

/// Retain only `group_by` dimensions; series sharing a retained tuple merge by
/// per-interval `sum_cells`. Empty `group_by` merges everything. An absent
/// dimension or metric is `CardMismatch`. Output is sorted by key.
pub fn collapse_series(
    dataset: &ChartDataset,
    metric: &str,
    group_by: &[String],
) -> Result<Vec<GroupedSeries>, ChartError> {
    if dataset.metric(metric).is_none() {
        return Err(ChartError::new(
            ChartErrorKind::CardMismatch,
            "the source does not provide the requested metric",
        ));
    }
    let mut groups: BTreeMap<Vec<(String, String)>, Vec<&[Cell]>> = BTreeMap::new();
    for series in dataset.series() {
        let mut key = Vec::with_capacity(group_by.len());
        for name in group_by {
            let value = series.dimensions.get(name).ok_or_else(|| {
                ChartError::new(
                    ChartErrorKind::CardMismatch,
                    "the source does not provide a requested dimension",
                )
            })?;
            key.push((name.clone(), value.clone()));
        }
        let cells = series.metrics.get(metric).ok_or_else(|| {
            ChartError::new(ChartErrorKind::CardMismatch, "series lacks the metric")
        })?;
        groups.entry(key).or_default().push(cells);
    }
    let width = dataset.intervals().len();
    groups
        .into_iter()
        .map(|(key, members)| {
            let cells = (0..width)
                .map(|index| {
                    let sum =
                        sum_cells(members.iter().map(|cells| &cells[index])).map_err(|_| {
                            ChartError::new(ChartErrorKind::InvalidDataset, "sum overflow")
                        })?;
                    Ok(Cell {
                        value: sum.known,
                        quality: sum.quality,
                    })
                })
                .collect::<Result<Vec<_>, ChartError>>()?;
            Ok(GroupedSeries { key, cells })
        })
        .collect()
}
