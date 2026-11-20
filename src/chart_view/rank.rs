//! Ranking of collapsed groups: stable ordering, an aggregated Other, and a
//! separate Unknown category that is never folded into Other.

use super::format::{display_labels, full_label};
use super::model::RowKind;
use super::palette::{Swatch, swatch_for};
use crate::chart::dataset::{Cell, Quality};
use crate::chart::decimal::ChartDecimal;
use crate::chart::reduce::{GroupedSeries, Reduction, last_known, sum_cells};
use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct Entry {
    pub key: Vec<(String, String)>,
    pub label: String,
    pub full_label: String,
    pub swatch: Swatch,
    pub kind: RowKind,
    pub value: Option<ChartDecimal>,
    pub quality: Quality,
    pub cells: Vec<Cell>,
}

pub struct Ranking {
    /// Top named entries, then Other when entries were cut, then Unknown.
    pub shown: Vec<Entry>,
    /// Every named entry in rank order, then Unknown; Other never repeats them.
    pub full: Vec<Entry>,
}

const OVERFLOW: &str = "Totals exceed the supported numeric range";

fn is_unknown(key: &[(String, String)]) -> bool {
    key.iter().any(|(_, value)| value.is_empty())
}

fn reduce(cells: &[Cell], reduction: Reduction) -> Result<(Option<ChartDecimal>, Quality), String> {
    match reduction {
        Reduction::Sum => {
            let sum = sum_cells(cells).map_err(|_| OVERFLOW.to_string())?;
            Ok((sum.known, sum.quality))
        }
        Reduction::Last => Ok(match last_known(cells) {
            Some(last) => (Some(last.value), last.quality),
            None => (None, Quality::Missing),
        }),
    }
}

fn order(a: &Entry, b: &Entry) -> Ordering {
    match (&a.value, &b.value) {
        (Some(x), Some(y)) => y.cmp(x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
    .then_with(|| a.key.cmp(&b.key))
}

/// `named_limit` of `None` keeps every named entry. Cutting named entries
/// creates an Other only for sums; gauges cannot be aggregated honestly.
pub fn rank(
    groups: Vec<GroupedSeries>,
    reduction: Reduction,
    named_limit: Option<usize>,
) -> Result<Ranking, String> {
    let keys: Vec<Vec<(String, String)>> = groups.iter().map(|group| group.key.clone()).collect();
    let labels = display_labels(&keys);
    let mut named = Vec::new();
    let mut unknown = Vec::new();
    for (group, label) in groups.into_iter().zip(labels) {
        let (value, quality) = reduce(&group.cells, reduction)?;
        let is_unknown = is_unknown(&group.key);
        let entry = Entry {
            full_label: full_label(&group.key),
            swatch: if is_unknown {
                Swatch::Unknown
            } else {
                swatch_for(&group.key)
            },
            kind: if is_unknown {
                RowKind::Unknown
            } else {
                RowKind::Named
            },
            label,
            key: group.key,
            value,
            quality,
            cells: group.cells,
        };
        if is_unknown {
            unknown.push(entry);
        } else {
            named.push(entry);
        }
    }
    named.sort_by(order);
    unknown.sort_by(order);
    let mut full = named.clone();
    full.extend(unknown.iter().cloned());

    let mut shown = named;
    let mut rest = Vec::new();
    if let Some(limit) = named_limit
        && shown.len() > limit
    {
        rest = shown.split_off(limit);
    }
    if !rest.is_empty() && reduction == Reduction::Sum {
        let pseudo: Vec<Cell> = rest
            .iter()
            .map(|entry| Cell {
                value: entry.value.clone(),
                quality: entry.quality,
            })
            .collect();
        let total = sum_cells(&pseudo).map_err(|_| OVERFLOW.to_string())?;
        let width = rest[0].cells.len();
        let mut cells = Vec::with_capacity(width);
        for index in 0..width {
            let sum = sum_cells(rest.iter().map(|entry| &entry.cells[index]))
                .map_err(|_| OVERFLOW.to_string())?;
            cells.push(Cell {
                value: sum.known,
                quality: sum.quality,
            });
        }
        shown.push(Entry {
            key: Vec::new(),
            label: "Other".to_string(),
            full_label: format!("Other ({} more)", rest.len()),
            swatch: Swatch::Other,
            kind: RowKind::Other,
            value: total.known,
            quality: total.quality,
            cells,
        });
    }
    shown.extend(unknown);
    Ok(Ranking { shown, full })
}
