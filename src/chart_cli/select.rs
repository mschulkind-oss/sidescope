//! Everything resolved before anything happens: the configuration file and
//! the source and card selectors. Failures here mean no process is spawned
//! and no file is written.

use super::args::Invocation;
use super::report::{ConfigBlock, ReportError};
use crate::chart::config::{ChartCardConfig, ChartSourceConfig};
use crate::{AppConfig, ConfigIssue, config_path, parse_config_detailed};
use std::path::PathBuf;

pub struct Loaded {
    pub block: ConfigBlock,
    pub config: AppConfig,
}

/// A TOML error is flattened and bounded; it has no key.
fn bounded(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 400 {
        let mut cut: String = flat.chars().take(399).collect();
        cut.push('…');
        cut
    } else {
        flat
    }
}

pub fn load_config(
    invocation: &Invocation,
) -> Result<Loaded, Box<(Option<ConfigBlock>, ReportError)>> {
    let (path, origin): (PathBuf, &'static str) = match &invocation.config {
        Some(path) => (PathBuf::from(path), "explicit"),
        None => match config_path() {
            Ok(path) => (path, "default"),
            Err(error) => {
                return Err(Box::new((
                    None,
                    ReportError::new("config-unreadable", bounded(&error)),
                )));
            }
        },
    };
    let block = |valid| ConfigBlock {
        path: path.display().to_string(),
        origin,
        valid,
    };
    let text = match std::fs::read(&path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                return Err(Box::new((
                    Some(block(false)),
                    ReportError::new("config-unreadable", "the configuration is not UTF-8"),
                )));
            }
        },
        // Only the default path may be absent: the live application treats
        // it as defaults, so the scope is then empty.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && origin == "default" => {
            return Ok(Loaded {
                block: block(true),
                config: AppConfig::default(),
            });
        }
        Err(error) => {
            return Err(Box::new((
                Some(block(false)),
                ReportError::new(
                    "config-unreadable",
                    format!("cannot read the configuration: {}", error.kind()),
                ),
            )));
        }
    };
    match parse_config_detailed(&text) {
        Ok(config) => Ok(Loaded {
            block: block(true),
            config,
        }),
        Err(ConfigIssue { key, message }) => {
            let mut error = ReportError::new("invalid-config", bounded(&message));
            error.key = key;
            Err(Box::new((Some(block(false)), error)))
        }
    }
}

pub struct Scope<'a> {
    pub sources: Vec<&'a ChartSourceConfig>,
    pub cards: Vec<&'a ChartCardConfig>,
}

fn problem(
    id: &'static str,
    text: String,
    source: Option<&str>,
    card: Option<&str>,
) -> ReportError {
    let mut error = ReportError::new(id, text);
    error.source = source.map(str::to_string);
    error.card = card.map(str::to_string);
    error
}

/// Applies the source and card selectors. `chosen` names the sources the verb
/// was given (`--source`, `--data` or a capture); `None` is every source.
pub fn select_scope<'a>(
    invocation: &Invocation,
    config: &'a AppConfig,
    chosen: Option<&[String]>,
) -> Result<Scope<'a>, ReportError> {
    let card_of = |id: &str| config.chart_cards.iter().find(|card| card.id == id);
    if let Some(ids) = chosen {
        for id in ids {
            if !config.chart_sources.iter().any(|source| source.id == *id) {
                return Err(problem(
                    "unknown-source",
                    format!("no chart source has the id {id}"),
                    Some(id),
                    None,
                ));
            }
        }
    }
    for id in &invocation.cards {
        if card_of(id).is_none() {
            return Err(problem(
                "unknown-card",
                format!("no chart card has the id {id}"),
                None,
                Some(id),
            ));
        }
    }
    let mut source_ids: Vec<&str> = match chosen {
        Some(ids) => ids.iter().map(String::as_str).collect(),
        None => config
            .chart_sources
            .iter()
            .map(|source| source.id.as_str())
            .collect(),
    };
    if chosen.is_none()
        && !invocation.cards.is_empty()
        && invocation.sources.is_empty()
        && !invocation.all_sources
    {
        // `check` narrows to the sources of the cards it was asked about.
        source_ids = invocation
            .cards
            .iter()
            .filter_map(|id| card_of(id).map(|card| card.source.as_str()))
            .collect();
    }
    for id in &invocation.cards {
        let card = card_of(id).expect("checked above");
        if !source_ids.contains(&card.source.as_str()) {
            return Err(problem(
                "conflicting-selection",
                format!(
                    "card {id} reads source {}, which this invocation does not select or give data for",
                    card.source
                ),
                Some(&card.source),
                Some(id),
            ));
        }
    }
    let sources: Vec<&ChartSourceConfig> = config
        .chart_sources
        .iter()
        .filter(|source| source_ids.contains(&source.id.as_str()))
        .collect();
    let cards: Vec<&ChartCardConfig> = config
        .chart_cards
        .iter()
        .filter(|card| {
            source_ids.contains(&card.source.as_str())
                && (invocation.cards.is_empty() || invocation.cards.contains(&card.id))
        })
        .collect();
    if let Some(details) = &invocation.details {
        if card_of(&details.card).is_none() {
            return Err(problem(
                "unknown-card",
                format!("--details names no chart card: {}", details.card),
                None,
                Some(&details.card),
            ));
        }
        if !cards.iter().any(|card| card.id == details.card) {
            return Err(problem(
                "conflicting-selection",
                format!(
                    "--details {} is outside this invocation's cards",
                    details.card
                ),
                None,
                Some(&details.card),
            ));
        }
    }
    if sources.is_empty() || cards.is_empty() {
        return Err(ReportError::new(
            "empty-scope",
            "no chart source or no chart card is selected; nothing to check",
        ));
    }
    Ok(Scope { sources, cards })
}
