//! Instants, half-open intervals, IANA zones and query windows.

pub use jiff::Timestamp;
use jiff::civil::Time;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Span};

pub const MAX_FIXED_WINDOW_MINUTES: u32 = 30 * 24 * 60;
pub const MAX_CALENDAR_DAYS: u16 = 400;

/// RFC 3339 with a numeric offset or `Z`; fractional seconds are allowed.
pub fn parse_instant(text: &str) -> Result<Timestamp, String> {
    text.parse::<Timestamp>()
        .map_err(|_| "expected an RFC 3339 instant with an offset".to_string())
}

/// `2026-11-20T12:00:00Z`, whole seconds.
pub fn format_utc(instant: Timestamp) -> String {
    Timestamp::from_second(instant.as_second())
        .unwrap_or(instant)
        .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: Timestamp,
    pub end: Timestamp,
}

impl Interval {
    pub fn new(start: Timestamp, end: Timestamp) -> Result<Self, String> {
        if end > start {
            Ok(Self { start, end })
        } else {
            Err("interval end must be after its start".to_string())
        }
    }

    pub fn seconds(&self) -> i64 {
        self.end.as_second() - self.start.as_second()
    }
}

#[derive(Debug, Clone)]
pub struct ChartZone {
    name: String,
    zone: TimeZone,
}

impl PartialEq for ChartZone {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ChartZone {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i16,
    pub month: i8,
    pub day: i8,
    pub hour: i8,
    pub minute: i8,
    pub offset_seconds: i32,
}

impl ChartZone {
    pub fn parse(name: &str) -> Result<Self, String> {
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err("timezone must be an IANA zone name".to_string());
        }
        let zone = TimeZone::get(name).map_err(|_| "unknown IANA timezone".to_string())?;
        Ok(Self {
            name: name.to_string(),
            zone,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn local(&self, instant: Timestamp) -> LocalTime {
        let local = self.zone.to_datetime(instant);
        LocalTime {
            year: local.year(),
            month: local.month(),
            day: local.day(),
            hour: local.hour(),
            minute: local.minute(),
            offset_seconds: self.zone.to_offset(instant).seconds(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowSpec {
    Fixed { minutes: u32 },
    CalendarDays(u16),
}

impl WindowSpec {
    /// `<n>m`, `<n>h` or `<n>d`: positive whole minutes, at most 30 days.
    pub fn parse_fixed(text: &str) -> Result<Self, String> {
        let invalid = || "window must look like 90m, 3h or 2d (whole units, at most 30 days)";
        let unit = text.chars().last().ok_or_else(invalid)?;
        let digits = &text[..text.len() - unit.len_utf8()];
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid().to_string());
        }
        let count: u64 = digits.parse().map_err(|_| invalid().to_string())?;
        let per_unit: u64 = match unit {
            'm' => 1,
            'h' => 60,
            'd' => 24 * 60,
            _ => return Err(invalid().to_string()),
        };
        let minutes = count.checked_mul(per_unit).ok_or_else(invalid)?;
        if minutes == 0 || minutes > u64::from(MAX_FIXED_WINDOW_MINUTES) {
            return Err(invalid().to_string());
        }
        Ok(Self::Fixed {
            minutes: minutes as u32,
        })
    }
}

/// Fixed: `[now-minutes, now)`. Calendar: `[local midnight (days-1) dates
/// before today, now)`.
pub fn resolve_window(
    spec: &WindowSpec,
    zone: &ChartZone,
    now: Timestamp,
) -> Result<Interval, String> {
    let start = match spec {
        WindowSpec::Fixed { minutes } => now
            .checked_sub(SignedDuration::from_mins(i64::from(*minutes)))
            .map_err(|_| "window start is out of range".to_string())?,
        WindowSpec::CalendarDays(days) => {
            let today = zone.zone.to_datetime(now).date();
            let first = today
                .checked_sub(Span::new().days(i64::from(*days) - 1))
                .map_err(|_| "window start is out of range".to_string())?;
            zone.zone
                .to_timestamp(first.to_datetime(Time::midnight()))
                .map_err(|_| "window start is out of range".to_string())?
        }
    };
    Interval::new(start, now)
}
