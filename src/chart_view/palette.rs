//! Stable swatches and theme-derived colors for chart drawing and CSS.

use crate::theme::{Color, Theme};

pub const CATEGORY_COUNT: usize = 8;

/// A color slot, never a color: chosen from the full dimension tuple so the
/// same group keeps its color across rankings, windows and datasets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Swatch {
    /// The single merged series of a card with no grouping.
    Accent,
    Categorical(u8),
    Other,
    Unknown,
}

/// FNV-1a over the tuple with separators that cannot occur in plain text.
pub fn swatch_for(key: &[(String, String)]) -> Swatch {
    if key.is_empty() {
        return Swatch::Accent;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (name, value) in key {
        for byte in name
            .bytes()
            .chain(std::iter::once(0x1f))
            .chain(value.bytes())
            .chain(std::iter::once(0x1e))
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Swatch::Categorical((hash % CATEGORY_COUNT as u64) as u8)
}

/// Okabe–Ito categorical colors, lightened for dark cards: separable under
/// common color-vision differences, and identical in every theme so a group
/// keeps its color when the theme changes text and surfaces.
const CATEGORY_RAMP: [Color; CATEGORY_COUNT] = [
    Color::rgb(0.34, 0.71, 0.91),
    Color::rgb(0.90, 0.62, 0.00),
    Color::rgb(0.00, 0.78, 0.55),
    Color::rgb(0.94, 0.89, 0.26),
    Color::rgb(0.86, 0.47, 0.70),
    Color::rgb(0.95, 0.40, 0.18),
    Color::rgb(0.55, 0.65, 0.95),
    Color::rgb(0.80, 0.80, 0.80),
];

#[derive(Debug, Clone, PartialEq)]
pub struct ChartPalette {
    pub text: Color,
    pub muted: Color,
    pub grid: Color,
    pub baseline: Color,
    pub background: Color,
    pub accent: Color,
    pub other: Color,
    pub unknown: Color,
    pub ramp: [Color; CATEGORY_COUNT],
    pub font: String,
}

impl ChartPalette {
    pub fn from_theme(theme: &Theme) -> ChartPalette {
        ChartPalette {
            text: theme.body,
            muted: theme.muted,
            grid: theme.plot.grid,
            baseline: theme.muted,
            background: theme.card,
            accent: theme.good,
            other: theme.neutral,
            unknown: theme.muted,
            ramp: CATEGORY_RAMP,
            font: theme.font.to_string(),
        }
    }

    pub fn color(&self, swatch: Swatch) -> Color {
        match swatch {
            Swatch::Accent => self.accent,
            Swatch::Categorical(index) => self.ramp[usize::from(index) % CATEGORY_COUNT],
            Swatch::Other => self.other,
            Swatch::Unknown => self.unknown,
        }
    }
}

/// Chart classes only, all prefixed `chart-`. Loaded in a second provider
/// after the theme stylesheet.
pub fn css(theme: &Theme) -> String {
    let c = |color: Color| color.css();
    format!(
        r#".chart-card .chart-title {{ color: {title}; font-weight: {weight}; }}
.chart-card .chart-headline-value {{ color: {title}; font-weight: 700; }}
.chart-card .chart-headline-label, .chart-card .chart-subtitle, .chart-card .chart-note, .chart-card .chart-legend {{ color: {muted}; }}
.chart-card .chart-readout {{ color: {body}; min-height: 1.2em; }}
.chart-card .chart-banner {{ background: {header}; color: {warning}; border-radius: 4px; padding: 2px 6px; }}
.chart-card .chart-message {{ color: {muted}; padding: 8px 0; }}
.chart-card .chart-plot:focus {{ outline: 2px solid {good}; outline-offset: 1px; }}
.chart-card .chart-table {{ font-family: monospace; background: {card}; color: {body}; }}
.chart-card .chart-info {{ color: {body}; }}
.chart-card .chart-action {{ background: {header}; color: {body}; min-height: 22px; padding: 1px 8px; }}
"#,
        title = c(theme.title),
        weight = theme.title_weight,
        muted = c(theme.muted),
        body = c(theme.body),
        header = c(theme.header),
        warning = c(theme.warning),
        good = c(theme.good),
        card = c(theme.card),
    )
}
