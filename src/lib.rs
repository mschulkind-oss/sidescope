pub mod chart;
pub mod chart_cli;
pub mod chart_mapping;
pub mod chart_runtime;
pub mod chart_view;
pub mod cli;
pub mod dashboard;
pub mod density;
pub mod plot;
pub mod process;
pub mod sensors;
pub mod theme;
pub mod weather;

use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// A local report from `ai-usagebar usage --json` (schema version 1).
#[derive(Debug, Deserialize)]
pub struct UsageReport {
    pub schema_version: u8,
    pub entries: Vec<UsageEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageEntry {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub plan: Option<String>,
    pub status: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub fetched_at: Option<String>,
    #[serde(default)]
    pub metrics: Vec<UsageMetric>,
    #[serde(default)]
    pub sections: Vec<UsageSection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageMetric {
    pub label: String,
    pub percent: u16,
    pub value: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub reset_at: Option<String>,
    #[serde(default)]
    pub window_secs: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UsageSection {
    Text {
        label: String,
        value: String,
    },
    Metric(UsageMetric),
    Block {
        #[serde(default)]
        label: String,
        #[serde(default)]
        body: SectionBody,
    },
    #[serde(other)]
    Other,
}

/// An OpenRouter-style prose block body. The report carries it as one string
/// or a list of lines; anything else (a number, an object, a missing field)
/// is kept rather than failing the whole report, and yields no row.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SectionBody {
    Text(String),
    Lines(Vec<String>),
    Other(serde_json::Value),
}

impl Default for SectionBody {
    fn default() -> Self {
        SectionBody::Other(serde_json::Value::Null)
    }
}

impl SectionBody {
    fn text(&self) -> String {
        match self {
            SectionBody::Text(line) => line.clone(),
            SectionBody::Lines(lines) => lines.join(" · "),
            SectionBody::Other(value) => match value {
                serde_json::Value::String(line) => line.clone(),
                serde_json::Value::Array(items) => items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" · "),
                _ => String::new(),
            },
        }
    }
}

/// Display labels for the agents this build names by hand. Membership comes from
/// the report, not from this list: an agent ai-usagebar reports that is missing
/// here still renders, labelled from the report itself.
const PROVIDER_LABELS: &[(&str, &str)] = &[
    ("openrouter", "OPENROUTER"),
    ("deepseek", "DEEPSEEK"),
    ("anthropic", "CLAUDE"),
    ("openai", "CODEX"),
    ("zai", "Z.AI"),
    ("antigravity", "AGY"),
    ("muse", "MUSE"),
];

/// The agent an entry id belongs to: `anthropic` for `anthropic`, `anthropic` for
/// `anthropic@work`.
fn base_target(entry_id: &str) -> &str {
    entry_id.split('@').next().unwrap_or(entry_id)
}

fn provider_label(id: &str, display_name: &str) -> String {
    PROVIDER_LABELS
        .iter()
        .find(|&&(target, _)| target == id)
        .map(|&(_, label)| label.to_owned())
        .unwrap_or_else(|| display_name.to_ascii_uppercase())
}

/// Non-secret local preferences. No file means automatic provider visibility.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub enabled: Option<Vec<String>>,
    pub theme: Option<String>,
    #[serde(default)]
    pub density: density::Density,
    /// Show Antigravity's independent Claude & GPT OSS pool in both windows.
    #[serde(default)]
    pub show_agy_gpt_oss: bool,
    pub weather: Option<weather::WeatherConfig>,
    #[serde(default)]
    pub sensors: sensors::SensorConfig,
    #[serde(default)]
    pub chart_sources: Vec<chart::config::ChartSourceConfig>,
    #[serde(default)]
    pub chart_cards: Vec<chart::config::ChartCardConfig>,
}

/// A configuration failure; `key` is set only for a chart table or key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    pub key: Option<String>,
    pub message: String,
}

pub fn parse_config(source: &str) -> Result<AppConfig, String> {
    parse_config_detailed(source).map_err(|issue| issue.message)
}

/// `parse_config` with the failing chart key when there is one. The message is
/// exactly the text `parse_config` returns.
pub fn parse_config_detailed(source: &str) -> Result<AppConfig, ConfigIssue> {
    let plain = |message: String| ConfigIssue { key: None, message };
    let config: AppConfig = toml::from_str(source).map_err(|error| plain(error.to_string()))?;
    if let Some(id) = &config.theme
        && theme::index(id).is_none()
    {
        return Err(plain(format!("unknown theme id: {id}")));
    }
    if let Some(ids) = &config.enabled {
        let mut seen = HashSet::new();
        for id in ids {
            // Any id is accepted. The installed ai-usagebar decides which agents
            // exist, so an id it does not report shows the unavailable state
            // rather than failing startup.
            if id.is_empty() {
                return Err(plain("provider id must not be empty".to_string()));
            }
            if !seen.insert(id) {
                return Err(plain(format!("duplicate provider id: {id}")));
            }
        }
    }
    if let Some(location) = &config.weather {
        location.validate().map_err(plain)?;
    }
    config.sensors.validate().map_err(plain)?;
    chart::config::validate_chart_config_detailed(&config.chart_sources, &config.chart_cards)
        .map_err(|issue| ConfigIssue {
            message: issue.rendered().to_string(),
            key: Some(issue.key),
        })?;
    Ok(config)
}

pub fn load_config_at(path: &Path) -> Result<AppConfig, String> {
    match std::fs::read_to_string(path) {
        Ok(source) => parse_config(&source).map_err(|error| format!("{}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

pub fn config_path() -> Result<std::path::PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .ok_or("Neither XDG_CONFIG_HOME nor HOME is set")?;
    Ok(base.join("sidescope").join("config.toml"))
}

fn matches_target(entry_id: &str, target: &str) -> bool {
    entry_id == target
        || entry_id
            .strip_prefix(target)
            .is_some_and(|suffix| suffix.starts_with('@'))
}

/// The reported agents, or exactly the ones `enabled` names. A selected agent
/// the report omits is honored, so it shows its unavailable state.
pub fn visible_cards(json: &str, config: &AppConfig) -> Result<Vec<Card>, String> {
    visible_cards_at(json, config, current_unix_time())
}

/// As `visible_cards`, with a stable check time for tests and snapshot rendering.
pub fn visible_cards_at(
    json: &str,
    config: &AppConfig,
    now_unix: i64,
) -> Result<Vec<Card>, String> {
    let cards = cards_from_json_at(json, config, now_unix)?;
    let Some(ids) = &config.enabled else {
        return Ok(cards);
    };
    // An explicit selection is honored even when the report has no entry for it,
    // so a chosen agent shows its unavailable state instead of vanishing.
    Ok(ids
        .iter()
        .map(|id| {
            cards
                .iter()
                .find(|card| card.id == *id)
                .cloned()
                .unwrap_or_else(|| missing_card(id))
        })
        .collect())
}

fn missing_card(id: &str) -> Card {
    let eyebrow = provider_label(id, id);
    Card {
        id: id.into(),
        eyebrow: eyebrow.clone(),
        title: id.into(),
        subtitle: "Not configured".into(),
        state: CardState::Unavailable("No report for this provider".into()),
        metrics: vec![],
        graphs: vec![],
        weather: None,
        note: String::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub id: String,
    pub eyebrow: String,
    pub title: String,
    pub subtitle: String,
    pub state: CardState,
    pub metrics: Vec<CardMetric>,
    pub graphs: Vec<Graph>,
    pub weather: Option<weather::WeatherDisplay>,
    pub note: String,
}

/// How a quota's usage compares with the elapsed fraction of its reset window.
/// A verdict needs a current reading; the elapsed fraction it is measured
/// against does not, and lives on [`CardMetric::elapsed_pct`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    /// Percentage points: quota used minus time elapsed.
    pub delta: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardMetric {
    pub label: String,
    pub value: String,
    pub detail: String,
    pub fraction: Option<u16>,
    /// How far along the reset window is, as a percentage. Time passes whether
    /// or not the reading is current, so a window the report states exactly
    /// keeps this even for a stale observation; only the source's own prose
    /// goes out of date, and only a current reading may use it. Absent when
    /// neither a window nor current prose supplies a trustworthy fraction.
    pub elapsed_pct: Option<u8>,
    /// Signed point difference between usage and elapsed time, for a current
    /// reading only. A stale or failed source must not acquire a verdict.
    pub pace: Option<Pace>,
}

/// Integer samples keep graph presentation deterministic and bounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    pub label: String,
    /// None marks a missing sampling slot, never a zero or interpolated value.
    pub samples: Vec<Option<u64>>,
    pub ceiling: u64,
    /// Signed series (e.g. temperature in millidegrees Celsius). None keeps
    /// the unsigned domain 0..ceiling behavior above.
    pub signed_samples: Option<Vec<Option<i64>>>,
    /// Explicit Y range in the signed series' native units. Set whenever
    /// `signed_samples` is set; ignored otherwise.
    pub range: Option<(i64, i64)>,
    /// Y unit for Info/tooltip wording, e.g. "°C". None keeps the legacy
    /// percent/bytes/s wording.
    pub unit: Option<String>,
}

impl Graph {
    /// The legacy unsigned graph: domain 0..ceiling, no unit wording change.
    pub fn unsigned(label: String, samples: Vec<Option<u64>>, ceiling: u64) -> Self {
        Graph {
            label,
            samples,
            ceiling,
            signed_samples: None,
            range: None,
            unit: None,
        }
    }
}

fn is_balance_label(label: &str) -> bool {
    matches!(
        label.to_ascii_lowercase().as_str(),
        "balance" | "credit balance" | "available balance" | "remaining" | "credits"
    )
}

fn money_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.contains('%') || value.contains(" of ") {
        return None;
    }
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let number = unsigned
        .trim_start_matches(['$', '¥', '€', '£'])
        .trim_end_matches([' ']);
    let number = number
        .strip_suffix(" USD")
        .or_else(|| number.strip_suffix(" CNY"))
        .unwrap_or(number);
    (number != unsigned && number.parse::<f64>().is_ok_and(f64::is_finite))
        .then(|| value.to_owned())
}

/// Fallback for a Muse Code report that carries no metrics: ai-usagebar's
/// documented custom `muse` recipe tallies the CLI's local token log and reports
/// spend and call counts as text rows, which have no reset window behind them.
/// Read them as readings, the way a balance is read, and never draw them as a
/// quota bar. A report from the Muse vendor carries real 5-hour and weekly
/// windows and takes the normal quota path instead.
fn spend_metrics(entry: &UsageEntry) -> Vec<CardMetric> {
    entry
        .sections
        .iter()
        .filter_map(|section| match section {
            UsageSection::Text { label, value } => Some(CardMetric {
                label: label.clone(),
                value: value.clone(),
                detail: String::new(),
                fraction: None,
                elapsed_pct: None,
                pace: None,
            }),
            _ => None,
        })
        .take(3)
        .collect()
}

fn balance_metrics(entry: &UsageEntry) -> Vec<CardMetric> {
    let sections = entry.sections.iter().filter_map(|s| match s {
        UsageSection::Text { label, value } if is_balance_label(label) => money_value(value),
        UsageSection::Metric(m) if is_balance_label(&m.label) => balance_amount(m),
        _ => None,
    });
    sections
        .chain(
            entry
                .metrics
                .iter()
                .filter(|m| is_balance_label(&m.label))
                .filter_map(balance_amount),
        )
        .take(1)
        .map(|value| CardMetric {
            label: "Current balance".into(),
            value,
            detail: String::new(),
            fraction: None,
            elapsed_pct: None,
            pace: None,
        })
        .collect()
}

fn report_note(entry: &UsageEntry, balance_only: bool, now_unix: i64) -> String {
    let mut note = if balance_only {
        "Daily spend history unavailable: no dated provider history in this report.".to_owned()
    } else {
        String::new()
    };
    if let Some(time) = &entry.fetched_at {
        if !note.is_empty() {
            note.push('\n');
        }
        note.push_str(&format!(
            "Source fetched {}",
            freshness_text(now_unix, time, 0)
        ));
    }
    note
}

fn balance_amount(m: &UsageMetric) -> Option<String> {
    money_value(&m.value).or_else(|| {
        // DeepSeek percentage headlines put the explicit remaining amount first in detail.
        // Never use this fallback for OpenRouter's consumed-credit comparison.
        if m.label != "Balance" {
            return None;
        }
        m.detail.split(" of ").next().and_then(money_value)
    })
}

/// The prose body of the first block whose label matches, or `None` when the
/// report carries no such block. Matching ignores case and hyphens, so
/// `Per-key limit` and `per key limit` are the same block.
fn openrouter_block(entry: &UsageEntry, name: &str) -> Option<String> {
    entry.sections.iter().find_map(|section| match section {
        UsageSection::Block { label, body }
            if label.trim().to_ascii_lowercase().replace('-', " ") == name =>
        {
            Some(body.text())
        }
        _ => None,
    })
}

/// This key's spend from a `Usage by period` block like
/// `today $1.00 · week $5.00 · month $25.00`. One compact row carries today's
/// spend with the week/month amounts as detail; a body without a readable
/// today amount yields no row rather than an invented one.
fn openrouter_key_spend(entry: &UsageEntry) -> Option<CardMetric> {
    let body = openrouter_block(entry, "usage by period")?;
    let mut today = None;
    let mut week = None;
    let mut month = None;
    for part in body.split('·') {
        let part = part.trim();
        let part = part
            .strip_prefix("this ")
            .or_else(|| part.strip_prefix("This "))
            .unwrap_or(part);
        let Some((word, amount)) = part.split_once(char::is_whitespace) else {
            continue;
        };
        // One unreadable segment stays unread; it must not sink the rest.
        let Some(amount) = money_value(amount.trim()) else {
            continue;
        };
        match word.to_ascii_lowercase().as_str() {
            "today" if today.is_none() => today = Some(amount),
            "week" | "weekly" if week.is_none() => week = Some(amount),
            "month" | "monthly" if month.is_none() => month = Some(amount),
            _ => {}
        }
    }
    Some(CardMetric {
        label: "Key spend today".into(),
        value: today?,
        detail: week
            .map(|value| format!("week {value}"))
            .into_iter()
            .chain(month.map(|value| format!("month {value}")))
            .collect::<Vec<_>>()
            .join(" · "),
        fraction: None,
        elapsed_pct: None,
        pace: None,
    })
}

/// This key's limit from a `Per-key limit` block like
/// `$20.00 of $50.00 remaining`. Only a body naming both the remaining and
/// the total amount yields a row; anything else stays absent.
fn openrouter_key_limit(entry: &UsageEntry) -> Option<CardMetric> {
    let body = openrouter_block(entry, "per key limit")?;
    let lowered = body.to_ascii_lowercase();
    let at = lowered.find(" of ")?;
    // ASCII-only needle, so the byte index splits the original safely.
    let (before, after) = body.split_at(at);
    let remaining = money_value(before.trim())?;
    let after = after[4..].trim();
    let lowered = after.to_ascii_lowercase();
    let total = ["remaining", "left"]
        .iter()
        .find_map(|word| lowered.strip_suffix(word).map(|head| &after[..head.len()]))
        .unwrap_or(after);
    let total = money_value(total.trim())?;
    Some(CardMetric {
        label: "This key limit remaining".into(),
        value: remaining.clone(),
        detail: format!("{remaining} of {total} remaining"),
        fraction: None,
        elapsed_pct: None,
        pace: None,
    })
}

/// This key's spend and limit rows, after the account balance. Never quota
/// bars or percentages: money rows keep `fraction` unset.
fn openrouter_metrics(entry: &UsageEntry) -> Vec<CardMetric> {
    let mut metrics = balance_metrics(entry);
    metrics.extend(openrouter_key_limit(entry));
    metrics.extend(openrouter_key_spend(entry));
    metrics
}

/// Key-scoped context for Info only: the tier and recent-model prose never
/// reaches the card face (both note summaries filter these lines out).
fn openrouter_note_lines(entry: &UsageEntry) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(tier) = openrouter_block(entry, "tier") {
        let tier = tier.trim();
        if !tier.is_empty() {
            lines.push(format!("Key tier: {tier}"));
        }
    }
    for name in ["recent models", "recent model"] {
        if let Some(models) = openrouter_block(entry, name) {
            let models = models.trim();
            if !models.is_empty() {
                lines.push(format!("Recent models: {models}"));
            }
            break;
        }
    }
    lines
}

/// Choose the next of two independent vertical card columns by measured height.
/// Equal heights go left, keeping placement deterministic across redraws.
pub fn pack_columns(heights: &[i32]) -> Vec<usize> {
    let mut totals = [0i32; 2];
    heights
        .iter()
        .map(|height| {
            let column = usize::from(totals[0] > totals[1]);
            totals[column] = totals[column]
                .saturating_add((*height).max(0))
                .saturating_add(12);
            column
        })
        .collect()
}

/// Keep human reset context when structured pace labels replace the source's prose.
pub fn pace_context(detail: &str) -> String {
    detail
        .split('·')
        .map(str::trim)
        .filter(|part| {
            !part.ends_with("% elapsed")
                && !part.ends_with("pts ahead")
                && !part.ends_with("pts under")
                && *part != "on track"
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The elapsed fraction and point difference that a current reading's own
/// `N% elapsed` prose states, with the claimed direction checked against
/// `used − elapsed`. `None` when the prose is missing, out of range or
/// internally inconsistent.
fn pace_from_detail(used: u16, detail: &str) -> Option<(u8, i16)> {
    if used > 100 {
        return None;
    }
    let parts = detail.split('·').map(str::trim);
    let elapsed_pct = parts.clone().find_map(|part| {
        part.strip_suffix("% elapsed")?
            .parse::<u8>()
            .ok()
            .filter(|value| *value <= 100)
    })?;
    let delta = used as i16 - i16::from(elapsed_pct);
    let claimed = parts
        .filter_map(|part| {
            if part == "on track" {
                return Some(0);
            }
            if let Some(value) = part.strip_suffix("pts ahead") {
                return value.parse::<i16>().ok();
            }
            if let Some(value) = part.strip_suffix("pts under") {
                return value.parse::<i16>().ok().map(|value| -value);
            }
            None
        })
        .next();
    if claimed.is_some_and(|value| value != delta) {
        return None;
    }
    Some((elapsed_pct, delta))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardState {
    Ready,
    Stale,
    Unavailable(String),
}

pub fn cards_from_json(json: &str) -> Result<Vec<Card>, String> {
    cards_from_json_at(json, &AppConfig::default(), current_unix_time())
}

pub fn current_unix_time() -> i64 {
    gtk::glib::DateTime::now_utc()
        .map(|now| now.to_unix())
        .unwrap_or(0)
}

/// Coarse human freshness for a past instant, e.g. "24 min ago".
///
/// A future instant reads "just now" rather than inventing negative time; a
/// skewed source clock should not turn into "-3 min ago".
pub fn freshness(now_unix: i64, then_unix: i64) -> String {
    let seconds = now_unix.saturating_sub(then_unix);
    if seconds < 60 {
        "just now".into()
    } else if seconds < 3600 {
        format!("{} min ago", seconds / 60)
    } else if seconds < 86_400 {
        let hours = seconds / 3600;
        if hours == 1 {
            "1 h ago".into()
        } else {
            format!("{hours} h ago")
        }
    } else {
        let days = seconds / 86_400;
        if days == 1 {
            "1 d ago".into()
        } else {
            format!("{days} d ago")
        }
    }
}

/// "24 min ago" for a source timestamp, falling back to the raw text so an
/// unrecognized format stays visible instead of disappearing.
pub fn freshness_text(now_unix: i64, text: &str, utc_offset_secs: i64) -> String {
    match instant_unix(text, utc_offset_secs) {
        Some(then) => freshness(now_unix, then),
        None => text.trim().to_owned(),
    }
}

/// Unix seconds for a source timestamp, or `None` when it is not a recognizable
/// instant. `utc_offset_secs` resolves a zone-less local time (Open-Meteo emits
/// the location's local clock and its offset separately).
pub fn instant_unix(text: &str, utc_offset_secs: i64) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // Open-Meteo omits seconds; glib needs a complete instant to parse.
    let padded = if text.len() == 16 {
        format!("{text}:00Z")
    } else {
        text.to_owned()
    };
    let parsed = gtk::glib::DateTime::from_iso8601(&padded, None)
        .ok()?
        .to_unix();
    Some(parsed.saturating_sub(utc_offset_secs))
}

/// How far along a reset window is, from its reset instant and exact length.
/// The clock decides this, so it needs no reading of the quota itself and
/// survives a stale observation. `None` when either field is missing, invalid,
/// implicit in a zero-length window, or outside the window.
fn elapsed_pct_from_window(reset_at: &str, window_secs: u64, now_unix: i64) -> Option<u8> {
    if window_secs == 0 {
        return None;
    }
    let window = i64::try_from(window_secs).ok()?;
    let reset = gtk::glib::DateTime::from_iso8601(reset_at, None)
        .ok()?
        .to_unix();
    let remaining = reset.checked_sub(now_unix)?;
    if !(0..=window).contains(&remaining) {
        return None;
    }
    Some((u128::from(window_secs - remaining as u64) * 100 / u128::from(window_secs)) as u8)
}

fn metric_label(id: &str, metric: &UsageMetric, agy_group_ordinal: Option<usize>) -> String {
    let lower = metric.label.to_ascii_lowercase();
    let window = match metric.window_secs {
        Some(18_000) => Some("5-hour"),
        Some(604_800) => Some("Weekly"),
        _ if lower.contains("5h") || lower.contains("5-hour") => Some("5-hour"),
        _ if lower.contains("weekly") => Some("Weekly"),
        _ if id == "antigravity" && metric.window_secs.is_none() => {
            agy_group_ordinal.map(|ordinal| if ordinal == 0 { "5-hour" } else { "Weekly" })
        }
        _ => None,
    };
    match window {
        Some(window) if id == "antigravity" => format!("{} · {window}", metric.label),
        Some(window) => window.into(),
        None => metric.label.clone(),
    }
}

fn cards_from_json_at(json: &str, config: &AppConfig, now_unix: i64) -> Result<Vec<Card>, String> {
    let report: UsageReport = serde_json::from_str(json).map_err(|error| error.to_string())?;
    if report.schema_version != 1 {
        return Err(format!(
            "unsupported usage schema {}",
            report.schema_version
        ));
    }
    // The report is the inventory: one tile per reported agent, in report order.
    // A named account belongs to its base agent, and the unnamed entry wins when
    // both are present.
    let mut targets: Vec<&str> = Vec::new();
    for entry in &report.entries {
        let target = base_target(&entry.id);
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    Ok(targets
        .into_iter()
        .map(|id| {
            let entry = report
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .or_else(|| {
                    report
                        .entries
                        .iter()
                        .find(|entry| matches_target(&entry.id, id))
                })
                .expect("a target comes from an entry");
            let label = provider_label(id, &entry.display_name);
            let state = if entry.status == "error" {
                CardState::Unavailable(
                    entry
                        .error
                        .clone()
                        .unwrap_or_else(|| "Source unavailable".into()),
                )
            } else if entry.stale {
                CardState::Stale
            } else {
                CardState::Ready
            };
            let balance_only = matches!(id, "openrouter" | "deepseek");
            // ai-usagebar reports Muse Code's own reset windows when its
            // Muse vendor is available. Only a report that carries no
            // metrics at all falls back to the spend and call rows a
            // custom-provider recipe supplies.
            let spend_only = id == "muse" && entry.metrics.is_empty();
            Card {
                id: id.into(),
                eyebrow: label,
                title: entry.display_name.clone(),
                subtitle: entry
                    .plan
                    .clone()
                    .unwrap_or_else(|| "Usage overview".into()),
                state,
                weather: None,
                graphs: vec![],
                note: {
                    let mut note = report_note(entry, balance_only || spend_only, now_unix);
                    // Tier and recent-model prose belongs to source Info, and
                    // both face-note summaries filter these lines back out.
                    if id == "openrouter" {
                        for line in openrouter_note_lines(entry) {
                            if !note.is_empty() {
                                note.push('\n');
                            }
                            note.push_str(&line);
                        }
                    }
                    note
                },
                metrics: if id == "openrouter" {
                    openrouter_metrics(entry)
                } else if balance_only {
                    balance_metrics(entry)
                } else if spend_only {
                    spend_metrics(entry)
                } else {
                    entry
                        .metrics
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| {
                            id != "antigravity"
                                || config.show_agy_gpt_oss
                                || m.label != "Claude & GPT OSS"
                        })
                        .take(if id == "antigravity" { 4 } else { 3 })
                        .map(|(index, m)| {
                            let group_count = entry
                                .metrics
                                .iter()
                                .filter(|candidate| candidate.label == m.label)
                                .count();
                            let group_ordinal =
                                (id == "antigravity" && group_count == 2).then(|| {
                                    entry.metrics[..index]
                                        .iter()
                                        .filter(|candidate| candidate.label == m.label)
                                        .count()
                                });
                            let fraction = (m.headline == "percent"
                                || (m.headline.is_empty() && m.value.trim().ends_with('%')))
                            .then_some(m.percent.min(100));
                            let window_elapsed = fraction.and_then(|_| {
                                elapsed_pct_from_window(
                                    m.reset_at.as_deref()?,
                                    m.window_secs?,
                                    now_unix,
                                )
                            });
                            // A reading can be stale; the clock cannot. A window
                            // the report states exactly is recomputed here, so
                            // its time mark outlives the reading it measures. The
                            // verdict needs a current reading, and the source's
                            // own `N% elapsed` prose goes out of date with it.
                            let (elapsed_pct, pace) = if entry.stale || entry.status == "error" {
                                (window_elapsed, None)
                            } else if m.detail.contains("% elapsed") {
                                match pace_from_detail(m.percent, &m.detail) {
                                    Some((elapsed, delta)) => (Some(elapsed), Some(Pace { delta })),
                                    None => (window_elapsed, None),
                                }
                            } else {
                                match window_elapsed {
                                    Some(elapsed) => (
                                        Some(elapsed),
                                        (m.percent <= 100).then(|| Pace {
                                            delta: m.percent as i16 - i16::from(elapsed),
                                        }),
                                    ),
                                    None => (None, None),
                                }
                            };
                            CardMetric {
                                label: if fraction.is_some() {
                                    metric_label(id, m, group_ordinal)
                                } else {
                                    m.label.clone()
                                },
                                value: m.value.clone(),
                                detail: m.detail.clone(),
                                fraction,
                                elapsed_pct,
                                pace,
                            }
                        })
                        .collect()
                },
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tile for one agent, so a test names the agent rather than a slot index.
    fn card<'a>(cards: &'a [Card], id: &str) -> &'a Card {
        cards
            .iter()
            .find(|card| card.id == id)
            .unwrap_or_else(|| panic!("no tile for {id}"))
    }

    #[test]
    fn negative_balance_and_deepseek_percentage_headline_keep_only_money() {
        assert_eq!(money_value("-$5.71"), Some("-$5.71".into()));
        let json = r#"{"schema_version":1,"entries":[{"id":"deepseek","display_name":"DeepSeek","status":"ready","sections":[{"type":"metric","label":"Balance","percent":90,"value":"90%","headline":"percent","detail":"¥5.00 of ¥50.00 left (90% used)"}]}]}"#;
        let cards = cards_from_json(json).unwrap();
        assert_eq!(card(&cards, "deepseek").metrics[0].value, "¥5.00");
        assert_eq!(card(&cards, "deepseek").metrics[0].fraction, None);
        assert_eq!(money_value("$NaN"), None);
        assert_eq!(money_value("$3 of $6"), None);
    }

    #[test]
    fn deepseek_text_sections_supply_current_balance() {
        let json = r#"{"schema_version":1,"entries":[{"id":"deepseek","display_name":"DeepSeek","status":"ready","metrics":[],"sections":[{"type":"text","label":"Balance","value":"$9.84"},{"type":"block","label":"Breakdown","body":["granted $1 · topped-up $8.84"]}]}]}"#;
        let cards = cards_from_json(json).unwrap();
        let deepseek = card(&cards, "deepseek");
        assert_eq!(deepseek.metrics.len(), 1);
        assert_eq!(deepseek.metrics[0].value, "$9.84");
        assert_eq!(deepseek.metrics[0].fraction, None);
    }

    #[test]
    fn openrouter_balance_discards_comparison_and_percentage() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openrouter","display_name":"OpenRouter","status":"ready","metrics":[{"label":"Credit balance","percent":50,"value":"$30.00","headline":"percent","detail":"$30.00 of $60.00 used (50%)"},{"label":"This month","percent":0,"value":"$6.72"}]}]}"#;
        let card = &cards_from_json(json).unwrap()[0];
        assert_eq!(card.metrics.len(), 1);
        assert_eq!(card.metrics[0].value, "$30.00");
        assert_eq!(card.metrics[0].fraction, None);
        assert!(!card.metrics[0].detail.contains("of"));
        assert_eq!(card.metrics[0].pace, None);
    }

    #[test]
    fn openrouter_key_blocks_add_key_scoped_rows_without_bars() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openrouter","display_name":"OpenRouter","plan":"API credits","status":"ready","metrics":[{"label":"Available balance","percent":0,"value":"$18.40","headline":"value","detail":"Account credit"}],"sections":[{"type":"block","label":"Usage by period","body":"today $1.00 · week $5.00 · month $25.00"},{"type":"block","label":"Per-key limit","body":"$20.00 of $50.00 remaining"},{"type":"block","label":"Tier","body":"paid tier"},{"type":"block","label":"Recent models","body":["anthropic/claude","openai/gpt-5"]}]}]}"#;
        let cards = cards_from_json(json).unwrap();
        let openrouter = card(&cards, "openrouter");
        assert_eq!(
            openrouter
                .metrics
                .iter()
                .map(|metric| (metric.label.as_str(), metric.value.as_str()))
                .collect::<Vec<_>>(),
            [
                ("Current balance", "$18.40"),
                ("This key limit remaining", "$20.00"),
                ("Key spend today", "$1.00"),
            ]
        );
        // The limit detail keeps the `X of Y remaining` context; the spend
        // detail keeps the wider periods. Neither is a quota bar or a pace.
        assert_eq!(openrouter.metrics[1].detail, "$20.00 of $50.00 remaining");
        assert_eq!(openrouter.metrics[2].detail, "week $5.00 · month $25.00");
        assert!(
            openrouter
                .metrics
                .iter()
                .all(|metric| metric.fraction.is_none()
                    && metric.elapsed_pct.is_none()
                    && metric.pace.is_none())
        );
        // Tier and recent models belong to source Info, not the card face.
        assert!(openrouter.note.contains("Key tier: paid tier"));
        assert!(
            openrouter
                .note
                .contains("Recent models: anthropic/claude · openai/gpt-5")
        );
        assert!(openrouter.note.contains("no dated provider history"));
    }

    #[test]
    fn openrouter_without_blocks_keeps_balance_only() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openrouter","display_name":"OpenRouter","status":"ready","metrics":[{"label":"Available balance","percent":0,"value":"$18.40","headline":"value","detail":"Account credit"}]}]}"#;
        let openrouter = &cards_from_json(json).unwrap()[0];
        assert_eq!(openrouter.metrics.len(), 1);
        assert_eq!(openrouter.metrics[0].label, "Current balance");
        assert!(!openrouter.note.contains("Key tier:"));
    }

    #[test]
    fn openrouter_malformed_blocks_yield_no_row_not_an_error() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openrouter","display_name":"OpenRouter","status":"ready","metrics":[{"label":"Available balance","percent":0,"value":"$18.40","headline":"value"}],"sections":[{"type":"block","label":"Usage by period","body":"spent some money lately"},{"type":"block","label":"Per-key limit","body":"unlimited"},{"type":"block","label":"Tier","body":42},{"type":"block","label":"Usage by period"}]}]}"#;
        let cards = cards_from_json(json).unwrap();
        let openrouter = card(&cards, "openrouter");
        assert_eq!(openrouter.metrics.len(), 1);
        assert_eq!(openrouter.metrics[0].value, "$18.40");
        assert!(!openrouter.note.contains("Key tier:"));
        // A week-only body has no today amount, so no spend row is invented.
        let json = r#"{"schema_version":1,"entries":[{"id":"openrouter","display_name":"OpenRouter","status":"ready","metrics":[],"sections":[{"type":"block","label":"Usage by period","body":"week $5.00 · month $25.00"}]}]}"#;
        assert!(cards_from_json(json).unwrap()[0].metrics.is_empty());
    }

    #[test]
    fn tiles_follow_the_report_as_the_inventory() {
        // The report decides which agents exist and in what order; an agent it
        // never mentions has no tile, and an error entry keeps its own state.
        let json = r#"{"schema_version":1,"entries":[{"id":"openai","display_name":"Codex","status":"ready","metrics":[]},{"id":"openrouter","display_name":"OpenRouter","status":"ready","metrics":[]},{"id":"deepseek","display_name":"DeepSeek","status":"error","error":"Authentication required","metrics":[]}]}"#;
        let cards = cards_from_json(json).unwrap();
        assert_eq!(
            cards
                .iter()
                .map(|card| card.id.as_str())
                .collect::<Vec<_>>(),
            ["openai", "openrouter", "deepseek"]
        );
        assert_eq!(cards[0].title, "Codex");
        assert!(matches!(cards[2].state, CardState::Unavailable(_)));
        assert!(cards.iter().all(|card| card.id != "muse"));
    }

    #[test]
    fn muse_reads_its_quota_windows_and_falls_back_to_reported_rows() {
        // ai-usagebar's Muse vendor reports the same two reset windows the other
        // coding plans report, so the card draws them as quotas.
        let json = r#"{"schema_version":1,"entries":[{"id":"muse","display_name":"Muse Code","plan":"Muse Code Power Usage","status":"ready","metrics":[{"label":"Session","percent":2,"value":"2%","headline":"percent","window_secs":18000,"detail":"Resets in 0h 06m · 4% elapsed · 2pts under"},{"label":"Weekly","percent":4,"value":"4%","headline":"percent","window_secs":604800,"detail":"Resets in 5d 3h · 26% elapsed · 22pts under"}]}]}"#;
        let card = cards_from_json(json)
            .unwrap()
            .into_iter()
            .find(|card| card.id == "muse")
            .unwrap();
        assert_eq!(card.eyebrow, "MUSE");
        assert_eq!(card.title, "Muse Code");
        assert_eq!(card.subtitle, "Muse Code Power Usage");
        assert_eq!(
            card.metrics
                .iter()
                .map(|metric| (
                    metric.label.as_str(),
                    metric.fraction,
                    metric.value.as_str()
                ))
                .collect::<Vec<_>>(),
            [("5-hour", Some(2), "2%"), ("Weekly", Some(4), "4%")]
        );
        assert_eq!(card.metrics[0].elapsed_pct, Some(4));
        assert_eq!(card.metrics[0].pace, Some(Pace { delta: -2 }));
        assert!(!card.note.contains("no dated provider history"));

        // A report carrying only a custom provider's text rows still reads,
        // without inventing a quota window.
        let json = r#"{"schema_version":1,"entries":[{"id":"muse","display_name":"Muse","plan":"Muse Spark (local)","status":"ready","metrics":[],"sections":[{"type":"text","label":"Today","value":"$4.20"},{"type":"text","label":"30 days","value":"$61.80"},{"type":"text","label":"Calls today","value":"37"}]}]}"#;
        let card = cards_from_json(json)
            .unwrap()
            .into_iter()
            .find(|card| card.id == "muse")
            .unwrap();
        assert_eq!(
            card.metrics
                .iter()
                .map(|metric| (metric.label.as_str(), metric.value.as_str()))
                .collect::<Vec<_>>(),
            [
                ("Today", "$4.20"),
                ("30 days", "$61.80"),
                ("Calls today", "37")
            ]
        );
        assert!(card.metrics.iter().all(|metric| metric.fraction.is_none()));
        assert!(card.note.contains("no dated provider history"));
    }

    #[test]
    fn named_codex_account_is_visible_without_matching_openrouter() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openai@personal","display_name":"Codex personal","status":"ready","metrics":[]}]}"#;
        let cards = visible_cards(json, &AppConfig::default()).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].id, "openai");
        assert_eq!(cards[0].title, "Codex personal");
    }

    #[test]
    fn named_claude_account_is_not_silently_reported_missing() {
        let cards = cards_from_json(r#"{"schema_version":1,"entries":[{"id":"anthropic@personal","display_name":"Claude · personal","status":"ready","metrics":[]}]}"#).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].id, "anthropic");
        assert_eq!(cards[0].title, "Claude · personal");
        assert_eq!(cards[0].state, CardState::Ready);
    }

    #[test]
    fn stale_and_error_are_not_presented_as_current() {
        let cards = cards_from_json(r#"{"schema_version":1,"entries":[{"id":"openrouter","display_name":"OpenRouter","status":"ready","stale":true,"metrics":[{"label":"Credits","percent":72,"value":"$14.00","headline":"value"}]},{"id":"deepseek","display_name":"DeepSeek","status":"error","error":"Authentication required"}]}"#).unwrap();
        assert_eq!(cards[0].state, CardState::Stale);
        assert_eq!(cards[0].metrics[0].fraction, None); // A monetary balance is not a percentage.
        assert_eq!(
            cards[1].state,
            CardState::Unavailable("Authentication required".into())
        );
    }

    #[test]
    fn synthetic_report_exercises_every_target_and_failure_state() {
        let cards = cards_from_json(include_str!("../tests/fixtures/usage.json")).unwrap();
        assert_eq!(cards.len(), 6);
        assert_eq!(cards[4].state, CardState::Stale);
        assert!(matches!(cards[5].state, CardState::Unavailable(_)));
        assert_eq!(cards[2].metrics.len(), 2);
        assert_eq!(cards[2].metrics[0].elapsed_pct, Some(25));
        assert_eq!(cards[2].metrics[0].pace, Some(Pace { delta: 13 }));
        assert_eq!(cards[2].metrics[1].elapsed_pct, Some(75));
        assert_eq!(cards[2].metrics[1].pace, Some(Pace { delta: -8 }));
    }

    #[test]
    fn stale_quota_keeps_the_window_time_mark_without_a_verdict() {
        // Meta omits Muse Code's subscription usage while its five-hour window is
        // idle, so ai-usagebar replays the last observation as stale. The reset
        // window and the clock are untouched, so the time mark outlives the
        // reading; the point difference it would be compared against does not.
        let json = r#"{"schema_version":1,"entries":[{"id":"muse","display_name":"Muse Code","status":"ready","stale":true,"metrics":[{"label":"Session","percent":0,"value":"0%","headline":"percent","reset_at":"2026-09-26T15:00:00Z","window_secs":18000,"detail":"Resets in 3h · 4% elapsed · 4pts under"}]}]}"#;
        let cards = visible_cards_at(json, &AppConfig::default(), 1_790_424_000).unwrap();
        assert_eq!(cards[0].state, CardState::Stale);
        // 40% of the window has elapsed at check time, not the stale prose's 4%.
        assert_eq!(cards[0].metrics[0].elapsed_pct, Some(40));
        assert_eq!(cards[0].metrics[0].pace, None);
    }

    #[test]
    fn a_verdict_never_outlives_the_time_mark_it_is_measured_against() {
        for fixture in [
            include_str!("../tests/fixtures/usage.json"),
            include_str!("../tests/fixtures/screen-fit-usage.json"),
        ] {
            for card in cards_from_json(fixture).unwrap() {
                for metric in &card.metrics {
                    assert!(
                        metric.pace.is_none() || metric.elapsed_pct.is_some(),
                        "{} reported a verdict without a time position",
                        card.id
                    );
                }
            }
        }
    }

    #[test]
    fn agy_shows_two_gemini_windows_with_derived_pace_and_opt_in_expands_both_pools() {
        let json = r#"{"schema_version":1,"entries":[{"id":"antigravity","display_name":"Antigravity","status":"ready","metrics":[{"label":"Gemini","percent":22,"value":"22%","headline":"percent","detail":"Resets in 2h 30m","reset_at":"2026-09-26T14:30:00Z","window_secs":18000},{"label":"Claude & GPT OSS","percent":0,"value":"0%","headline":"percent","detail":"Resets in 2h 30m","reset_at":"2026-09-26T14:30:00Z","window_secs":18000},{"label":"Gemini","percent":29,"value":"29%","headline":"percent","detail":"Resets in 3d 12h","reset_at":"2026-09-30T00:00:00Z","window_secs":604800},{"label":"Claude & GPT OSS","percent":5,"value":"5%","headline":"percent","detail":"Resets in 3d 12h","reset_at":"2026-09-30T00:00:00Z","window_secs":604800}]}]}"#;
        let now = 1_790_424_000; // 2026-09-26 12:00 UTC
        let default = visible_cards_at(json, &AppConfig::default(), now).unwrap();
        assert_eq!(default.len(), 1);
        let agy = &default[0];
        assert_eq!(
            agy.metrics
                .iter()
                .map(|m| m.label.as_str())
                .collect::<Vec<_>>(),
            ["Gemini · 5-hour", "Gemini · Weekly"]
        );
        assert_eq!(agy.metrics[0].elapsed_pct, Some(50));
        assert_eq!(agy.metrics[0].pace, Some(Pace { delta: -28 }));
        assert_eq!(agy.metrics[1].elapsed_pct, Some(50));
        assert_eq!(agy.metrics[1].pace, Some(Pace { delta: -21 }));

        let config = parse_config("show_agy_gpt_oss = true").unwrap();
        let agy = &visible_cards_at(json, &config, now).unwrap()[0];
        assert_eq!(
            agy.metrics
                .iter()
                .map(|m| m.label.as_str())
                .collect::<Vec<_>>(),
            [
                "Gemini · 5-hour",
                "Claude & GPT OSS · 5-hour",
                "Gemini · Weekly",
                "Claude & GPT OSS · Weekly"
            ]
        );
        assert_eq!(agy.metrics[1].elapsed_pct, Some(50));
        assert_eq!(agy.metrics[1].pace, Some(Pace { delta: -50 }));
    }

    #[test]
    fn quota_window_labels_are_consistent_across_providers() {
        let demo = cards_from_json(include_str!("../tests/fixtures/usage.json")).unwrap();
        assert_eq!(
            demo[2]
                .metrics
                .iter()
                .map(|m| m.label.as_str())
                .collect::<Vec<_>>(),
            ["5-hour", "Weekly"]
        );
        let json = r#"{"schema_version":1,"entries":[{"id":"openai","display_name":"Codex","status":"ready","metrics":[{"label":"Codex weekly","percent":5,"value":"5%","window_secs":604800}]},{"id":"zai","display_name":"Z.AI","status":"ready","metrics":[{"label":"Session (5h)","percent":10,"value":"10%","window_secs":18000},{"label":"Weekly","percent":12,"value":"12%","window_secs":604800}]}]}"#;
        let cards = cards_from_json(json).unwrap();
        assert_eq!(card(&cards, "openai").metrics[0].label, "Weekly");
        assert_eq!(
            card(&cards, "zai")
                .metrics
                .iter()
                .map(|m| m.label.as_str())
                .collect::<Vec<_>>(),
            ["5-hour", "Weekly"]
        );
    }

    #[test]
    fn missing_or_invalid_window_metadata_never_invents_derived_pace() {
        assert_eq!(
            elapsed_pct_from_window("2026-09-26T11:00:00Z", 18_000, 1_790_424_000),
            None
        );
        assert_eq!(
            elapsed_pct_from_window("2026-09-26T18:00:00Z", 18_000, 1_790_424_000),
            None
        );
        assert_eq!(
            elapsed_pct_from_window("2026-09-26T14:30:00Z", 0, 1_790_424_000),
            None
        );
        assert_eq!(
            elapsed_pct_from_window("2026-09-26T15:00:00Z", 18_000, 1_790_424_000),
            Some(40)
        );
        let json = r#"{"schema_version":1,"entries":[{"id":"antigravity","display_name":"Agy","status":"ready","metrics":[{"label":"Gemini","percent":22,"value":"22%","reset_at":"2026-09-26T14:30:00Z"},{"label":"Gemini","percent":29,"value":"29%","reset_at":"not-a-date","window_secs":604800}]}]}"#;
        let agy = &visible_cards_at(json, &AppConfig::default(), 1_790_424_000).unwrap()[0];
        assert_eq!(agy.metrics[0].pace, None);
        assert_eq!(agy.metrics[0].elapsed_pct, None);
        assert_eq!(agy.metrics[1].pace, None);
        assert_eq!(agy.metrics[1].elapsed_pct, None);
        assert_eq!(
            agy.metrics
                .iter()
                .map(|m| m.label.as_str())
                .collect::<Vec<_>>(),
            ["Gemini · 5-hour", "Gemini · Weekly"]
        );
    }

    #[test]
    fn pace_markers_reflect_used_minus_elapsed_percentage_points() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openai","display_name":"Codex","status":"ready","metrics":[{"label":"Ahead","percent":58,"value":"58%","headline":"percent","detail":"Resets in 2d 22h · 57% elapsed · 1pts ahead"},{"label":"Under","percent":30,"value":"70% left","headline":"percent","detail":"Resets in 2h · 50% elapsed · 20pts under"},{"label":"Even","percent":40,"value":"60% left","headline":"percent","detail":"Resets tomorrow · 40% elapsed · on track"}]}]}"#;
        let cards = cards_from_json(json).unwrap();
        let codex = card(&cards, "openai");
        assert_eq!(codex.metrics[0].elapsed_pct, Some(57));
        assert_eq!(codex.metrics[0].pace, Some(Pace { delta: 1 }));
        assert_eq!(codex.metrics[1].elapsed_pct, Some(50));
        assert_eq!(codex.metrics[1].pace, Some(Pace { delta: -20 }));
        assert_eq!(codex.metrics[2].elapsed_pct, Some(40));
        assert_eq!(codex.metrics[2].pace, Some(Pace { delta: 0 }));
    }

    #[test]
    fn pace_is_absent_for_money_stale_and_unrecognized_or_inconsistent_detail() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openrouter","display_name":"OpenRouter","status":"ready","metrics":[{"label":"Balance","percent":58,"value":"$9.00","headline":"value","detail":"57% elapsed · 1pts ahead"}]},{"id":"openai","display_name":"Codex","status":"ready","metrics":[{"label":"Unknown","percent":58,"value":"58%","headline":"percent","detail":"Resets soon"},{"label":"Wrong sign","percent":58,"value":"58%","headline":"percent","detail":"57% elapsed · 1pts under"},{"label":"Bad time","percent":58,"value":"58%","headline":"percent","detail":"105% elapsed · 47pts under"}]},{"id":"zai","display_name":"z.ai","status":"ready","stale":true,"metrics":[{"label":"Stale","percent":58,"value":"58%","headline":"percent","detail":"57% elapsed · 1pts ahead"}]}]}"#;
        let cards = cards_from_json(json).unwrap();
        assert_eq!(card(&cards, "openrouter").metrics[0].pace, None);
        assert_eq!(card(&cards, "openrouter").metrics[0].elapsed_pct, None);
        assert!(
            card(&cards, "openai")
                .metrics
                .iter()
                .all(|metric| metric.pace.is_none() && metric.elapsed_pct.is_none())
        );
        // Stale prose states where the window stood when its observation was
        // fetched, so it cannot place a time mark at check time; only a window
        // the report states exactly can do that.
        assert_eq!(card(&cards, "zai").metrics[0].elapsed_pct, None);
        assert_eq!(card(&cards, "zai").metrics[0].pace, None);
    }

    #[test]
    fn freshness_is_coarse_honest_and_falls_back_to_raw_text() {
        assert_eq!(freshness(1_000_000, 1_000_000), "just now");
        assert_eq!(freshness(1_000_059, 1_000_000), "just now");
        assert_eq!(freshness(1_000_060, 1_000_000), "1 min ago");
        assert_eq!(freshness(1_001_440, 1_000_000), "24 min ago");
        assert_eq!(freshness(1_000_000 + 3600, 1_000_000), "1 h ago");
        assert_eq!(freshness(1_000_000 + 7200, 1_000_000), "2 h ago");
        assert_eq!(freshness(1_000_000 + 86_400, 1_000_000), "1 d ago");
        assert_eq!(freshness(1_000_000 + 3 * 86_400, 1_000_000), "3 d ago");
        // A source clock ahead of ours is not negative time.
        assert_eq!(freshness(1_000_000, 1_000_600), "just now");
        assert_eq!(
            freshness_text(1_790_425_440, "2026-09-26T12:00:00Z", 0),
            "24 min ago"
        );
        // Zone-less Open-Meteo text plus its declared offset resolves to UTC.
        assert_eq!(
            freshness_text(1_790_425_440, "2026-09-26T14:00", 7200),
            "24 min ago"
        );
        // Unrecognized text stays visible rather than turning into a fake time.
        assert_eq!(freshness_text(1_000_000, "Someday soon", 0), "Someday soon");
        assert_eq!(instant_unix("", 0), None);
    }

    #[test]
    fn paced_detail_keeps_reset_context_without_repeating_points() {
        assert_eq!(
            pace_context("Resets in 2d 22h · 57% elapsed · 1pts ahead"),
            "Resets in 2d 22h"
        );
        assert_eq!(pace_context("57% elapsed · on track"), "");
    }

    #[test]
    fn shortest_column_packing_has_stable_ties_and_no_shared_rows() {
        assert_eq!(pack_columns(&[220, 130, 170, 240]), [0, 1, 1, 0]);
        assert_eq!(pack_columns(&[100, 100, 100, 100]), [0, 1, 0, 1]);
    }

    #[test]
    fn legacy_percentage_metric_still_draws_a_bar_without_guessing_on_money() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openai","display_name":"Codex","status":"ready","metrics":[{"label":"Weekly","percent":58,"value":"58%"},{"label":"Weekly spending","percent":0,"value":"$9.00"}]}]}"#;
        let cards = cards_from_json(json).unwrap();
        let codex = card(&cards, "openai");
        assert_eq!(codex.metrics[0].fraction, Some(58));
        assert_eq!(codex.metrics[1].fraction, None);
        assert_eq!(codex.metrics[1].label, "Weekly spending");
    }

    #[test]
    fn tiles_show_only_the_reported_agents() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openai","display_name":"Codex","status":"ready","metrics":[]}]}"#;
        let cards = visible_cards(json, &AppConfig::default()).unwrap();
        assert_eq!(
            cards.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["openai"]
        );
    }

    #[test]
    fn explicit_config_controls_order_and_can_show_unavailable_or_nothing() {
        let json = r#"{"schema_version":1,"entries":[{"id":"openai","display_name":"Codex","status":"ready","metrics":[]}]}"#;
        let config = parse_config("enabled = ['zai', 'openai']").unwrap();
        let cards = visible_cards(json, &config).unwrap();
        assert_eq!(
            cards.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["zai", "openai"]
        );
        assert!(matches!(cards[0].state, CardState::Unavailable(_)));
        assert!(
            visible_cards(json, &parse_config("enabled = []").unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn missing_config_uses_auto_mode_but_invalid_config_fails() {
        let path =
            std::env::temp_dir().join(format!("stripe-config-test-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(load_config_at(&path).unwrap().enabled.is_none());
        std::fs::write(&path, "enabled = ['']").unwrap();
        assert!(
            load_config_at(&path)
                .unwrap_err()
                .contains("must not be empty")
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn config_rejects_duplicate_and_empty_provider_ids() {
        // Any id the installed ai-usagebar reports is valid, so only a malformed
        // selection is refused.
        assert!(parse_config("enabled = ['openai', 'openai']").is_err());
        assert!(parse_config("enabled = ['']").is_err());
        assert!(parse_config("enabled = ['not-a-provider']").is_ok());
        assert!(parse_config("unknown = 1").is_err());
        assert!(parse_config("show_agy_gpt_oss = 'yes'").is_err());
    }

    #[test]
    fn rejects_unknown_schema_and_invalid_json() {
        assert!(
            cards_from_json(r#"{"schema_version":2,"entries":[]}"#)
                .unwrap_err()
                .contains("unsupported")
        );
        assert!(cards_from_json("not json").is_err());
    }
}

#[cfg(test)]
mod theme_config_tests {
    use super::*;
    #[test]
    fn theme_config_defaults_validates_closed_ids_and_never_accepts_css() {
        assert!(parse_config("").unwrap().theme.is_none());
        assert!(AppConfig::default().theme.is_none());
        for t in theme::THEMES {
            assert_eq!(
                parse_config(&format!("theme = {:?}", t.id))
                    .unwrap()
                    .theme
                    .as_deref(),
                Some(t.id)
            );
        }
        for id in ["unknown", "Original", "paper; window { color: red; }", ""] {
            assert!(
                parse_config(&format!("theme = {id:?}"))
                    .unwrap_err()
                    .contains("unknown theme id")
            );
        }
        assert!(parse_config("theme = 12").is_err());
        assert!(parse_config("layout = \"compact\"").is_err());
    }
}
