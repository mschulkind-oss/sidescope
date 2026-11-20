//! Display attachments for ranked rows: labels and swatches. Ordering,
//! folding into Other and the separate Unknown category are decided by
//! `chart::facts`; nothing here ranks.

use super::format::{display_labels, full_label};
use super::model::RowKind;
use super::palette::{Swatch, swatch_for};
use crate::chart::dataset::{Cell, Quality};
use crate::chart::decimal::ChartDecimal;
use crate::chart::facts::{CardFacts, RowRole};

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
    /// Interval indices a sampled line does not connect across.
    pub breaks: Vec<usize>,
    /// For last-observation reductions: index of the interval observed and
    /// whether it is the newest interval of the answer.
    pub observed: Option<(usize, bool)>,
}

/// The plotted rows of the facts with their labels and swatches. Short labels
/// depend on every collapsed group, so they come from the whole key set.
pub fn shown_entries(facts: &CardFacts) -> Vec<Entry> {
    let labels = display_labels(&facts.group_keys);
    facts
        .rows
        .iter()
        .map(|row| {
            let observed = row
                .observed
                .map(|observed| (observed.index, observed.is_latest));
            match &row.group {
                Some(key) => {
                    let unknown = row.role == RowRole::Unknown;
                    let label = facts
                        .group_keys
                        .iter()
                        .position(|candidate| candidate == key)
                        .map_or_else(|| full_label(key), |index| labels[index].clone());
                    Entry {
                        full_label: full_label(key),
                        swatch: if unknown {
                            Swatch::Unknown
                        } else {
                            swatch_for(key)
                        },
                        kind: if unknown {
                            RowKind::Unknown
                        } else {
                            RowKind::Named
                        },
                        label,
                        key: key.clone(),
                        value: row.value.clone(),
                        quality: row.quality,
                        cells: row.cells.clone(),
                        breaks: row.breaks.clone(),
                        observed,
                    }
                }
                None => Entry {
                    key: Vec::new(),
                    label: "Other".to_string(),
                    full_label: format!("Other ({} more)", row.members),
                    swatch: Swatch::Other,
                    kind: RowKind::Other,
                    value: row.value.clone(),
                    quality: row.quality,
                    cells: row.cells.clone(),
                    breaks: Vec::new(),
                    observed,
                },
            }
        })
        .collect()
}
