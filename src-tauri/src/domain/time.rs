//! Time conversions. Canonical time is integer microseconds; these helpers
//! convert to and from the decimal-second strings FFmpeg uses without going
//! through floating point.

use serde::{Deserialize, Serialize};

pub const US_PER_SECOND: u64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32,
}

impl Rational {
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    pub fn is_positive(&self) -> bool {
        self.numerator > 0 && self.denominator > 0
    }

    /// Parses FFmpeg's `"30000/1001"` form. Returns `None` for `"0/0"` and
    /// other non-positive or malformed values.
    pub fn parse(text: &str) -> Option<Self> {
        let (n, d) = text.trim().split_once('/')?;
        let rational = Self::new(n.parse().ok()?, d.parse().ok()?);
        rational.is_positive().then_some(rational)
    }
}

/// Parses a non-negative decimal-seconds string such as `"12.500000"` into
/// microseconds, truncating digits beyond microsecond precision.
pub fn parse_seconds_to_us(text: &str) -> Option<u64> {
    let text = text.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let whole_us = if whole.is_empty() {
        0
    } else {
        whole.parse::<u64>().ok()?.checked_mul(US_PER_SECOND)?
    };
    let micro_digits: String = fraction.chars().chain("000000".chars()).take(6).collect();
    whole_us.checked_add(micro_digits.parse::<u64>().ok()?)
}

/// Formats microseconds as decimal seconds for FFmpeg arguments, e.g. `"1.500000"`.
pub fn format_us_as_seconds(us: u64) -> String {
    format!("{}.{:06}", us / US_PER_SECOND, us % US_PER_SECOND)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rational_frame_rates() {
        assert_eq!(
            Rational::parse("30000/1001"),
            Some(Rational::new(30000, 1001))
        );
        assert_eq!(Rational::parse("0/0"), None);
        assert_eq!(Rational::parse("30"), None);
        assert_eq!(Rational::parse("a/b"), None);
    }

    #[test]
    fn parses_seconds_without_float_error() {
        assert_eq!(parse_seconds_to_us("12.500000"), Some(12_500_000));
        assert_eq!(parse_seconds_to_us("0.1"), Some(100_000));
        assert_eq!(parse_seconds_to_us("3"), Some(3_000_000));
        assert_eq!(parse_seconds_to_us("1.23456789"), Some(1_234_567));
        assert_eq!(parse_seconds_to_us(".5"), Some(500_000));
    }

    #[test]
    fn rejects_invalid_seconds() {
        for bad in ["", ".", "-1", "N/A", "1e3", "1.2.3"] {
            assert_eq!(parse_seconds_to_us(bad), None, "{bad}");
        }
    }

    #[test]
    fn formats_seconds() {
        assert_eq!(format_us_as_seconds(1_500_000), "1.500000");
        assert_eq!(format_us_as_seconds(42), "0.000042");
    }
}
