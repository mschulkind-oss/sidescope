//! Plain-text formatting of instants, intervals, amounts and axis ticks.

use crate::chart::decimal::ChartDecimal;
use crate::chart::time::{ChartZone, Interval, Timestamp};

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

pub fn weekday(year: i16, month: i8, day: i8) -> &'static str {
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = i32::from(year);
    if month < 3 {
        y -= 1;
    }
    let index =
        (y + y / 4 - y / 100 + y / 400 + OFFSETS[(month as usize - 1) % 12] + i32::from(day))
            .rem_euclid(7);
    WEEKDAYS[index as usize]
}

pub fn offset_text(offset_seconds: i32) -> String {
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let total = offset_seconds.unsigned_abs() / 60;
    format!("{sign}{:02}:{:02}", total / 60, total % 60)
}

/// `2026-11-01 01:30:00 -04:00`: always seconds and numeric offset, which
/// also tells apart the repeated hour of a DST change.
pub fn local_text(instant: Timestamp, zone: &ChartZone) -> String {
    let local = zone.local(instant);
    let second = (instant.as_second() + i64::from(local.offset_seconds)).rem_euclid(60);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {}",
        local.year,
        local.month,
        local.day,
        local.hour,
        local.minute,
        second,
        offset_text(local.offset_seconds)
    )
}

pub fn interval_text(interval: &Interval, zone: &ChartZone) -> String {
    format!(
        "{} \u{2013} {}",
        exact_local_text(interval.start, zone),
        exact_local_text(interval.end, zone)
    )
}

/// One instant in the zone, with any fractional second: where a sampled
/// reading was taken.
pub fn instant_text(instant: Timestamp, zone: &ChartZone) -> String {
    exact_local_text(instant, zone)
}

/// `local_text` plus any nonzero fractional second, so sub-second interval
/// bounds stay distinguishable in Details.
fn exact_local_text(instant: Timestamp, zone: &ChartZone) -> String {
    let text = local_text(instant, zone);
    let nanos = instant.subsec_nanosecond();
    if nanos == 0 {
        return text;
    }
    let fraction = format!("{:09}", nanos.rem_euclid(1_000_000_000));
    let fraction = fraction.trim_end_matches('0');
    match text.rsplit_once(' ') {
        Some((clock, offset)) => format!("{clock}.{fraction} {offset}"),
        None => text,
    }
}

/// Short axis label: clock time for spans up to two days, otherwise the date.
pub fn axis_time(instant: Timestamp, zone: &ChartZone, span_seconds: i64) -> String {
    let local = zone.local(instant);
    if span_seconds <= 2 * 86_400 {
        format!("{:02}:{:02}", local.hour, local.minute)
    } else {
        format!("{:02}-{:02}", local.month, local.day)
    }
}

pub fn day_label(instant: Timestamp, zone: &ChartZone) -> String {
    let local = zone.local(instant);
    weekday(local.year, local.month, local.day).to_string()
}

/// Compact range for subtitles, in the dataset's zone.
pub fn range_text(range: &Interval, zone: &ChartZone) -> String {
    let (start, end) = (zone.local(range.start), zone.local(range.end));
    let same_day = (start.year, start.month, start.day) == (end.year, end.month, end.day);
    if same_day {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}–{:02}:{:02} {}",
            start.year,
            start.month,
            start.day,
            start.hour,
            start.minute,
            end.hour,
            end.minute,
            zone.name()
        )
    } else {
        format!(
            "{:04}-{:02}-{:02}–{:04}-{:02}-{:02} {}",
            start.year,
            start.month,
            start.day,
            end.year,
            end.month,
            end.day,
            zone.name()
        )
    }
}

pub fn unit_text(unit: &str) -> &str {
    if unit == "percent" { "%" } else { unit }
}

/// Value with its unit for headlines: rounded for reading, never summed.
pub fn amount_text(value: &ChartDecimal, unit: &str) -> String {
    let magnitude = value.to_f64().abs();
    let places = if magnitude >= 100.0 {
        0
    } else if magnitude >= 1.0 {
        2
    } else if magnitude >= 0.01 {
        3
    } else {
        4
    };
    let mut text = value.format_fixed(places);
    if !value.is_zero() && text.chars().all(|c| matches!(c, '0' | '.' | '-')) {
        text = if value.is_negative() {
            ">-0.0001".to_string()
        } else {
            "<0.0001".to_string()
        };
    } else if value.is_zero() {
        text = "0".to_string();
    }
    join_unit(&text, unit)
}

/// Exact, unrounded value for details and readouts.
pub fn exact_text(value: &ChartDecimal) -> String {
    value.to_plain_string()
}

pub fn join_unit(number: &str, unit: &str) -> String {
    match unit_text(unit) {
        "%" => format!("{number}%"),
        unit => format!("{number} {unit}"),
    }
}

/// Short tick label such as `0`, `0.5`, `12`, `1.2k`.
pub fn tick_text(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs();
    let (scaled, suffix) = if magnitude >= 1e9 {
        (value / 1e9, "G")
    } else if magnitude >= 1e6 {
        (value / 1e6, "M")
    } else if magnitude >= 1e3 {
        (value / 1e3, "k")
    } else {
        (value, "")
    };
    let places = if scaled.abs() >= 10.0 {
        0
    } else if scaled.abs() >= 1.0 {
        1
    } else if scaled.abs() >= 0.1 {
        2
    } else {
        3
    };
    let mut text = format!("{scaled:.places$}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    format!("{text}{suffix}")
}

pub fn share_text(share: f64) -> String {
    if share > 0.0 && share < 0.005 {
        "<1%".to_string()
    } else {
        format!("{:.0}%", share * 100.0)
    }
}

/// Last path component, so long workspace paths shorten to their name.
fn tail(value: &str, components: usize) -> String {
    let parts: Vec<&str> = value.trim_end_matches('/').split('/').collect();
    let take = components.min(parts.len()).max(1);
    parts[parts.len() - take..].join("/")
}

/// Display labels for dimension tuples. Values shorten to their last path
/// component; tuples whose shortened labels collide gain parent-path context,
/// then fall back to the full value.
pub fn display_labels(keys: &[Vec<(String, String)>]) -> Vec<String> {
    let render = |key: &Vec<(String, String)>, components: usize| -> String {
        if key.is_empty() {
            return "All".to_string();
        }
        key.iter()
            .map(|(_, value)| {
                if value.is_empty() {
                    "Unknown".to_string()
                } else if components == usize::MAX {
                    value.clone()
                } else {
                    tail(value, components)
                }
            })
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let mut labels: Vec<String> = keys.iter().map(|key| render(key, 1)).collect();
    for components in [2, usize::MAX] {
        let colliding: Vec<bool> = labels
            .iter()
            .map(|label| labels.iter().filter(|other| *other == label).count() > 1)
            .collect();
        for (index, key) in keys.iter().enumerate() {
            if colliding[index] {
                labels[index] = render(key, components);
            }
        }
    }
    labels
}

pub fn full_label(key: &[(String, String)]) -> String {
    if key.is_empty() {
        return "All".to_string();
    }
    key.iter()
        .map(|(_, value)| {
            if value.is_empty() {
                "Unknown"
            } else {
                value.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}
