//! Display-independent plot geometry: a real-duration time axis and
//! deterministic outward-rounded value scales.

use super::time::{Interval, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeAxis {
    start: i128,
    end: i128,
}

impl TimeAxis {
    pub fn new(range: Interval) -> Self {
        Self {
            start: range.start.as_nanosecond(),
            end: range.end.as_nanosecond(),
        }
    }

    /// Position in `0..=1` by exact duration; instants outside clamp.
    pub fn fraction(&self, instant: Timestamp) -> f64 {
        self.fraction_of(instant.as_nanosecond())
    }

    fn fraction_of(&self, nanoseconds: i128) -> f64 {
        let width = self.end - self.start;
        if width <= 0 {
            return 0.0;
        }
        let offset = (nanoseconds - self.start).clamp(0, width);
        offset as f64 / width as f64
    }

    pub fn center(&self, interval: &Interval) -> f64 {
        let middle = (interval.start.as_nanosecond() + interval.end.as_nanosecond()) / 2;
        self.fraction_of(middle)
    }

    /// Bar left and right edges; width follows real duration.
    pub fn span(&self, interval: &Interval) -> (f64, f64) {
        (self.fraction(interval.start), self.fraction(interval.end))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueScale {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleKind {
    Nonnegative,
    Signed,
    Percent0To100,
}

/// `None` when there are no finite values. Nonnegative and signed scales
/// include zero; signed also includes both extrema; ends round outward to
/// 1-2-5 steps; an all-zero input still gets a nonzero span.
pub fn value_scale(values: &[f64], kind: ScaleKind) -> Option<ValueScale> {
    let mut finite = values.iter().copied().filter(|value| value.is_finite());
    let first = finite.next()?;
    if kind == ScaleKind::Percent0To100 {
        return Some(ValueScale {
            min: 0.0,
            max: 100.0,
        });
    }
    let (low, high) = finite.fold((first, first), |(low, high), v| (low.min(v), high.max(v)));
    let low = if kind == ScaleKind::Nonnegative {
        0.0
    } else {
        low.min(0.0)
    };
    let high = high.max(0.0);
    if low == 0.0 && high == 0.0 {
        return Some(ValueScale { min: 0.0, max: 1.0 });
    }
    Some(ValueScale {
        min: if low < 0.0 { -nice_ceiling(-low) } else { 0.0 },
        max: if high > 0.0 { nice_ceiling(high) } else { 0.0 },
    })
}

/// Smallest 1, 2, 5 or 10 times a power of ten that is not below `value`.
fn nice_ceiling(value: f64) -> f64 {
    let exponent = value.log10().floor() as i32;
    let unit = 10f64.powi(exponent);
    let mantissa = value / unit;
    let tolerance = 1.0 + 1e-9;
    let step = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .find(|candidate| mantissa <= candidate * tolerance)
        .unwrap_or(10.0);
    step * unit
}

impl ValueScale {
    pub fn fraction(&self, value: f64) -> f64 {
        let span = self.max - self.min;
        if span <= 0.0 || !value.is_finite() {
            return 0.0;
        }
        ((value - self.min) / span).clamp(0.0, 1.0)
    }

    pub fn ticks(&self) -> [f64; 3] {
        [self.min, (self.min + self.max) / 2.0, self.max]
    }
}
