//! Exact decimal values: parsed from a strict plain-notation grammar, summed
//! without rounding, and converted to floating point only for drawing.

use bigdecimal::{BigDecimal, RoundingMode, ToPrimitive, Zero};
use std::str::FromStr;

pub const MAX_INTEGER_DIGITS: usize = 24;
pub const MAX_FRACTION_DIGITS: usize = 18;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChartDecimal(BigDecimal);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalError {
    Syntax,
    TooManyIntegerDigits,
    TooManyFractionDigits,
    Overflow,
}

impl ChartDecimal {
    /// Grammar: `-?(0|[1-9][0-9]*)(\.[0-9]+)?`.
    pub fn parse(text: &str) -> Result<Self, DecimalError> {
        let unsigned = text.strip_prefix('-').unwrap_or(text);
        let (integer, fraction) = match unsigned.split_once('.') {
            Some((integer, fraction)) => (integer, Some(fraction)),
            None => (unsigned, None),
        };
        let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if !digits(integer) || (integer.len() > 1 && integer.starts_with('0')) {
            return Err(DecimalError::Syntax);
        }
        if let Some(fraction) = fraction
            && !digits(fraction)
        {
            return Err(DecimalError::Syntax);
        }
        if integer.len() > MAX_INTEGER_DIGITS {
            return Err(DecimalError::TooManyIntegerDigits);
        }
        if fraction.is_some_and(|fraction| fraction.len() > MAX_FRACTION_DIGITS) {
            return Err(DecimalError::TooManyFractionDigits);
        }
        BigDecimal::from_str(text)
            .map(Self)
            .map_err(|_| DecimalError::Syntax)
    }

    pub fn zero() -> Self {
        Self(BigDecimal::zero())
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub fn is_negative(&self) -> bool {
        self.0 < BigDecimal::zero()
    }

    pub fn checked_add(&self, other: &Self) -> Result<Self, DecimalError> {
        let sum = Self(&self.0 + &other.0);
        let plain = sum.to_plain_string();
        let integer_digits = plain
            .trim_start_matches('-')
            .split('.')
            .next()
            .map_or(0, str::len);
        if integer_digits > MAX_INTEGER_DIGITS {
            return Err(DecimalError::Overflow);
        }
        Ok(sum)
    }

    /// For drawing only; never feed the result back into sums.
    pub fn to_f64(&self) -> f64 {
        self.0.to_f64().unwrap_or(f64::NAN)
    }

    /// Exact, normalized, no exponent; zero never carries a sign.
    pub fn to_plain_string(&self) -> String {
        if self.0.is_zero() {
            return "0".to_string();
        }
        self.0.normalized().to_plain_string()
    }

    /// Half away from zero; for labels only, never for accumulation.
    pub fn format_fixed(&self, places: u32) -> String {
        let rounded = self
            .0
            .with_scale_round(i64::from(places), RoundingMode::HalfUp);
        if rounded.is_zero() && places > 0 {
            return format!("0.{}", "0".repeat(places as usize));
        }
        rounded.to_plain_string()
    }
}
