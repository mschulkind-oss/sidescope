//! Curated, closed theme registry. Configuration selects an id, never CSS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color(pub f64, pub f64, pub f64, pub f64);
impl Color {
    pub const fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self(r, g, b, 1.0)
    }
    pub fn css(self) -> String {
        format!(
            "rgba({},{},{},{})",
            (self.0 * 255.0).round() as u8,
            (self.1 * 255.0).round() as u8,
            (self.2 * 255.0).round() as u8,
            self.3
        )
    }
}
const fn hex(n: u32) -> Color {
    Color::rgb(
        ((n >> 16) & 255) as f64 / 255.0,
        ((n >> 8) & 255) as f64 / 255.0,
        (n & 255) as f64 / 255.0,
    )
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Series {
    Cpu,
    Ram,
    Swap,
    Rx,
    Tx,
    Weather,
}
impl Series {
    pub fn from_label(label: &str) -> Self {
        if label.starts_with("RAM") {
            Self::Ram
        } else if label.starts_with("Swap") {
            Self::Swap
        } else if label.starts_with("Receive") {
            Self::Rx
        } else if label.starts_with("Transmit") {
            Self::Tx
        } else {
            Self::Cpu
        }
    }
    fn index(self) -> usize {
        self as usize
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlotStyle {
    pub series: [Color; 6],
    pub grid: Color,
    pub background: Color,
    pub width: f64,
    pub grid_width: f64,
    pub grid_rows: u8,
    pub dash: &'static [f64],
    pub points: bool,
    pub fill_alpha: f64,
    pub glyph: Color,
    pub glyph_width: f64,
    pub range_track: Color,
    pub range_low: Color,
    pub range_high: Color,
    pub range_width: f64,
    pub marker_outline: Color,
    pub marker: Color,
}
impl PlotStyle {
    pub fn color(self, series: Series) -> Color {
        self.series[series.index()]
    }
}
/// Curated component geometry, independent of graph ink and decoration tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuLayout {
    Compact,
    Roomy,
}
impl CpuLayout {
    pub const fn core_height(self) -> i32 {
        match self {
            Self::Compact => 24,
            Self::Roomy => 28,
        }
    }
    pub const fn row_max_height(self) -> i32 {
        match self {
            Self::Compact => 32,
            Self::Roomy => 48,
        }
    }
    pub const fn row_spacing(self) -> u32 {
        match self {
            Self::Compact => 2,
            Self::Roomy => 6,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherLayout {
    CurveRows,
    ConditionStrip,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryLayout {
    Stacked,
    Inline,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutStyle {
    pub cpu: CpuLayout,
    pub weather: WeatherLayout,
    pub telemetry: TelemetryLayout,
    pub card_spacing: i32,
}
const ORIGINAL_LAYOUT: LayoutStyle = LayoutStyle {
    cpu: CpuLayout::Compact,
    weather: WeatherLayout::CurveRows,
    telemetry: TelemetryLayout::Stacked,
    card_spacing: 0,
};
const INSTRUMENT_LAYOUT: LayoutStyle = LayoutStyle {
    cpu: CpuLayout::Compact,
    weather: WeatherLayout::ConditionStrip,
    telemetry: TelemetryLayout::Inline,
    card_spacing: 2,
};
const DISPLAY_LAYOUT: LayoutStyle = LayoutStyle {
    cpu: CpuLayout::Roomy,
    weather: WeatherLayout::CurveRows,
    telemetry: TelemetryLayout::Stacked,
    card_spacing: 4,
};
const QUIET_LAYOUT: LayoutStyle = LayoutStyle {
    cpu: CpuLayout::Roomy,
    weather: WeatherLayout::ConditionStrip,
    telemetry: TelemetryLayout::Inline,
    card_spacing: 2,
};

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub direction: &'static str,
    pub background: Color,
    pub header: Color,
    pub footer: Color,
    pub focus: Color,
    pub card: Color,
    pub featured: Color,
    pub title: Color,
    pub body: Color,
    pub muted: Color,
    pub border: Color,
    pub provider: Color,
    pub local: Color,
    pub remote: Color,
    pub good: Color,
    pub warning: Color,
    pub neutral: Color,
    pub temperature: Color,
    pub font: &'static str,
    pub title_font: &'static str,
    pub radius: u8,
    pub border_width: u8,
    pub padding: u8,
    pub title_size: u8,
    pub title_weight: u16,
    pub shadow: bool,
    pub plot: PlotStyle,
    pub layout: LayoutStyle,
}
const DEFAULT_LINE: Color = Color::rgb(0.53, 0.82, 0.66);
const DEFAULT_GRID: Color = Color(0.65, 0.8, 0.76, 0.16);
pub const ORIGINAL: Theme = Theme {
    id: "original",
    name: "Original",
    direction: "Existing green night dashboard, unchanged",
    background: hex(0x101720),
    header: hex(0x202b32),
    footer: hex(0x202b32),
    focus: hex(0x194439),
    card: hex(0x1b2931),
    featured: hex(0x21382f),
    title: hex(0xf0f3ed),
    body: hex(0xeaf0ec),
    muted: hex(0xaebfbd),
    border: hex(0x344b50),
    provider: hex(0xa4d5b7),
    local: hex(0xa4d5b7),
    remote: hex(0xa4d5b7),
    good: hex(0x86d1a3),
    warning: hex(0xe6a06c),
    neutral: hex(0x9ac9cf),
    temperature: hex(0xeaf0ec),
    font: "Sans",
    title_font: "Sans",
    radius: 11,
    border_width: 1,
    padding: 10,
    title_size: 20,
    title_weight: 700,
    shadow: false,
    layout: ORIGINAL_LAYOUT,
    plot: PlotStyle {
        series: [DEFAULT_LINE; 6],
        grid: DEFAULT_GRID,
        background: Color(0.0, 0.0, 0.0, 0.0),
        width: 1.5,
        grid_width: 0.7,
        grid_rows: 3,
        dash: &[],
        points: false,
        fill_alpha: 0.0,
        glyph: Color::rgb(0.69, 0.87, 0.75),
        glyph_width: 1.7,
        range_track: DEFAULT_GRID,
        range_low: DEFAULT_LINE,
        range_high: DEFAULT_LINE,
        range_width: 4.0,
        marker_outline: Color::rgb(0.08, 0.13, 0.16),
        marker: Color::rgb(0.98, 0.98, 0.94),
    },
};
// Each art direction chooses components and six graph inks independently.
#[allow(clippy::too_many_arguments)]
const fn crafted(
    id: &'static str,
    name: &'static str,
    direction: &'static str,
    layout: LayoutStyle,
    surfaces: [u32; 6],
    inks: [u32; 7],
    series: [u32; 6],
    fonts: [&'static str; 2],
    shape: [u8; 6],
    plot: [f64; 5],
    dash: &'static [f64],
    points: bool,
    shadow: bool,
) -> Theme {
    Theme {
        id,
        name,
        direction,
        layout,
        background: hex(surfaces[0]),
        header: hex(surfaces[1]),
        footer: hex(surfaces[2]),
        focus: hex(surfaces[3]),
        card: hex(surfaces[4]),
        featured: hex(surfaces[5]),
        title: hex(inks[0]),
        body: hex(inks[1]),
        muted: hex(inks[2]),
        border: hex(inks[3]),
        provider: hex(inks[4]),
        local: hex(inks[5]),
        remote: hex(inks[6]),
        good: hex(series[0]),
        warning: hex(series[4]),
        neutral: hex(series[3]),
        temperature: hex(series[5]),
        font: fonts[0],
        title_font: fonts[1],
        radius: shape[0],
        border_width: shape[1],
        padding: shape[2],
        title_size: shape[3],
        title_weight: if shape[4] == 0 { 500 } else { 700 },
        shadow,
        plot: PlotStyle {
            series: [
                hex(series[0]),
                hex(series[1]),
                hex(series[2]),
                hex(series[3]),
                hex(series[4]),
                hex(series[5]),
            ],
            grid: Color(hex(inks[3]).0, hex(inks[3]).1, hex(inks[3]).2, plot[4]),
            background: hex(surfaces[4]),
            width: plot[0],
            grid_width: plot[1],
            grid_rows: shape[5],
            dash,
            points,
            fill_alpha: plot[2],
            glyph: hex(inks[6]),
            glyph_width: plot[3],
            range_track: Color(hex(inks[3]).0, hex(inks[3]).1, hex(inks[3]).2, 0.3),
            range_low: hex(series[3]),
            range_high: hex(series[5]),
            range_width: plot[0] + 2.0,
            marker_outline: hex(surfaces[4]),
            marker: hex(inks[0]),
        },
    }
}
pub static THEMES: &[Theme] = &[
    ORIGINAL,
    crafted(
        "paper",
        "Paper Ledger",
        "Warm paper, serif headings, ruled fine ink",
        LayoutStyle {
            card_spacing: 0,
            ..DISPLAY_LAYOUT
        },
        [0xebe6db, 0xe1d8c6, 0xe6ddcf, 0xd5c5a5, 0xfbf8f0, 0xf4ecd9],
        [
            0x292d32, 0x343a40, 0x63645e, 0xb5aa97, 0x805234, 0x3c6470, 0x875437,
        ],
        [0x397154, 0x755889, 0x97652f, 0x246d82, 0xa54736, 0x875437],
        ["Sans", "Serif"],
        [2, 1, 10, 20, 1, 3],
        [1.2, 0.6, 0.0, 1.4, 0.4],
        &[],
        false,
        false,
    ),
    crafted(
        "terminal",
        "Phosphor Terminal",
        "Monospace squared panels, luminous dotted traces",
        INSTRUMENT_LAYOUT,
        [0x080f0b, 0x14251b, 0x0c1d12, 0x213d29, 0x0e1812, 0x12291b],
        [
            0xd4ffdc, 0xb6edc1, 0x83b88d, 0x42794b, 0xa4efa6, 0x7edba7, 0xd6dc8c,
        ],
        [0x88e6a3, 0xc2e884, 0xe5cf85, 0x7fd2d5, 0xeab588, 0xdce995],
        ["Monospace", "Monospace"],
        [0, 1, 9, 19, 1, 3],
        [1.4, 0.7, 0.0, 1.4, 0.3],
        &[2.0, 2.0],
        true,
        false,
    ),
    crafted(
        "blueprint",
        "Blueprint",
        "Technical navy, squared double rules and dashed grids",
        INSTRUMENT_LAYOUT,
        [0x10263f, 0x123553, 0x102d49, 0x194667, 0x16314d, 0x1a3956],
        [
            0xe4f5ff, 0xc7e6f5, 0x96bcd2, 0x5187aa, 0x91d7fb, 0x92e4d8, 0xefce87,
        ],
        [0x80ded0, 0x92bcfa, 0xccb3f4, 0x83d4fa, 0xf4b57f, 0xefce87],
        ["Sans", "Monospace"],
        [0, 2, 9, 19, 1, 5],
        [1.3, 0.8, 0.0, 1.6, 0.4],
        &[5.0, 3.0],
        false,
        false,
    ),
    crafted(
        "editorial",
        "Editorial",
        "Ivory newsprint, black serif titles and vermilion rules",
        LayoutStyle {
            cpu: CpuLayout::Compact,
            card_spacing: 0,
            ..DISPLAY_LAYOUT
        },
        [0xeeeae4, 0xe2ddd6, 0xe6e0d7, 0xd6c8bb, 0xfffff9, 0xf8f0e8],
        [
            0x252323, 0x383332, 0x68615c, 0xb1a59b, 0xa24432, 0x315e63, 0x845126,
        ],
        [0x366758, 0x5f5784, 0x8b633a, 0x286d7b, 0xb34730, 0x8e502b],
        ["Sans", "Serif"],
        [0, 1, 10, 21, 0, 0],
        [2.0, 0.5, 0.0, 1.7, 0.25],
        &[],
        false,
        false,
    ),
    crafted(
        "contrast",
        "Signal Contrast",
        "Black white panels, bold rails, solid high contrast plots",
        DISPLAY_LAYOUT,
        [0x000000, 0x171717, 0x171717, 0x303030, 0x080808, 0x161616],
        [
            0xffffff, 0xf5f5f5, 0xcccccc, 0xaaaaaa, 0xffda70, 0x73edcf, 0xffaacb,
        ],
        [0x80f5b7, 0xb2c8ff, 0xe5b7ff, 0x76e9ff, 0xffc083, 0xffadc8],
        ["Sans", "Sans"],
        [0, 2, 10, 20, 1, 3],
        [2.4, 1.0, 0.0, 2.2, 0.5],
        &[],
        true,
        false,
    ),
    crafted(
        "retro",
        "Retro Console",
        "Cream monitor, chunky brown rules, stepped-feel dots",
        INSTRUMENT_LAYOUT,
        [0xe5d5b3, 0xcbb98e, 0xd3c49e, 0xc3a776, 0xf6eccf, 0xefe1b8],
        [
            0x443524, 0x4e402c, 0x756043, 0x9f8255, 0x8d4936, 0x526549, 0x76572c,
        ],
        [0x526b40, 0x6c6686, 0x8a6b36, 0x3b7080, 0xa84c36, 0x8b552d],
        ["Monospace", "Sans"],
        [5, 2, 9, 19, 1, 3],
        [2.0, 0.8, 0.08, 2.0, 0.35],
        &[4.0, 2.0],
        true,
        false,
    ),
    crafted(
        "cloud",
        "Cloud Soft",
        "Cool porcelain, pill cards, quiet fills and lavender glyphs",
        QUIET_LAYOUT,
        [0xe6edf3, 0xdce5ef, 0xe0e7f0, 0xc9d6e8, 0xf7faff, 0xf0f4ff],
        [
            0x28384e, 0x34465a, 0x607188, 0xafbfd0, 0x72588b, 0x3b6f79, 0x775a89,
        ],
        [0x36735f, 0x6064a0, 0x916794, 0x26708e, 0xa95646, 0x775a89],
        ["Sans", "Sans"],
        [18, 1, 10, 20, 0, 3],
        [1.6, 0.5, 0.1, 1.6, 0.28],
        &[],
        false,
        true,
    ),
    crafted(
        "minimal",
        "Quiet Minimal",
        "White canvas, low-radius hairlines, no grid or fill",
        QUIET_LAYOUT,
        [0xf4f4f2, 0xeaece9, 0xeef0ec, 0xdfe5e0, 0xffffff, 0xfafcf9],
        [
            0x25352f, 0x35443e, 0x66756d, 0xc1cbc5, 0x61704b, 0x35665f, 0x765d4c,
        ],
        [0x366c59, 0x656391, 0x7c7151, 0x356b83, 0x995e43, 0x765d4c],
        ["Sans", "Sans"],
        [2, 1, 10, 20, 0, 0],
        [1.2, 0.5, 0.0, 1.3, 0.25],
        &[],
        false,
        false,
    ),
    crafted(
        "nord",
        "Nord Observatory",
        "Frosted slate, rounded instruments, sparse arctic traces",
        QUIET_LAYOUT,
        [0x242d3b, 0x303e50, 0x29394a, 0x3c5266, 0x2e3b4d, 0x344557],
        [
            0xedf5fb, 0xdce7f0, 0xaabed0, 0x536b83, 0xb1c5ea, 0x91d1c3, 0xe9ca9b,
        ],
        [0x95d5b2, 0xb3c6f4, 0xcab4e6, 0x88d4e4, 0xeeb493, 0xe9ca9b],
        ["Sans", "Sans"],
        [9, 1, 10, 20, 1, 3],
        [1.8, 0.6, 0.08, 1.7, 0.32],
        &[],
        false,
        false,
    ),
    crafted(
        "solar",
        "Solar Study",
        "Ochre laboratory, monospaced metrics, fine dashed plots",
        INSTRUMENT_LAYOUT,
        [0xf2e8cf, 0xe5d6ae, 0xeadfbe, 0xd7c28e, 0xfbf3de, 0xf5eac6],
        [
            0x444630, 0x505039, 0x757258, 0xb7a773, 0x956a2c, 0x4c746d, 0x935a32,
        ],
        [0x547b55, 0x766a9c, 0x947232, 0x307985, 0xad533b, 0x965e32],
        ["Monospace", "Serif"],
        [4, 1, 9, 20, 1, 3],
        [1.5, 0.6, 0.0, 1.5, 0.35],
        &[6.0, 2.0],
        false,
        false,
    ),
    crafted(
        "rose",
        "Rose Atelier",
        "Dusty blush cards, serif headings, rose curves and soft shadows",
        DISPLAY_LAYOUT,
        [0xeee3e5, 0xe2d0d6, 0xe6d6dc, 0xd7bfc9, 0xfbf3f5, 0xf5e7ec],
        [
            0x4a303d, 0x543e48, 0x7b6470, 0xbba0ac, 0x935368, 0x557471, 0x965d73,
        ],
        [0x507866, 0x765c91, 0x966540, 0x3c7388, 0xa65342, 0x99516f],
        ["Sans", "Serif"],
        [14, 1, 10, 20, 1, 3],
        [1.8, 0.5, 0.12, 1.8, 0.28],
        &[],
        false,
        true,
    ),
    crafted(
        "ocean",
        "Ocean Depths",
        "Teal glass instruments with broad cyan lines and fine grids",
        DISPLAY_LAYOUT,
        [0x09262f, 0x103d47, 0x0c343f, 0x175462, 0x12353f, 0x17434a],
        [
            0xe4f8f5, 0xc7e6e5, 0x96bbbd, 0x3f737d, 0xeac693, 0x83d9cb, 0xb1d5f2,
        ],
        [0x80d9c3, 0xa5c2f5, 0xd5b7e5, 0x72d6ee, 0xf4bb91, 0xb2d7f5],
        ["Sans", "Sans"],
        [12, 1, 10, 20, 1, 5],
        [2.0, 0.5, 0.12, 1.8, 0.25],
        &[],
        false,
        true,
    ),
    crafted(
        "ember",
        "Ember Workshop",
        "Charcoal and copper, square frames, dashed violet memory",
        INSTRUMENT_LAYOUT,
        [0x211d1b, 0x37302a, 0x302820, 0x49372a, 0x2e2723, 0x3c3028],
        [
            0xf7e8d7, 0xe9d6c4, 0xbfa996, 0x745b46, 0xf0b37f, 0xb9ce9a, 0xf1cb8e,
        ],
        [0xb5d59a, 0xc7b4ed, 0xe3c68e, 0x8fcbd5, 0xf0a781, 0xf1cb8e],
        ["Sans", "Monospace"],
        [3, 1, 10, 19, 1, 3],
        [1.8, 0.7, 0.06, 1.9, 0.35],
        &[4.0, 2.0],
        false,
        false,
    ),
    crafted(
        "orchid",
        "Orchid Studio",
        "Plum panels with editorial serif titles and fine dotted points",
        DISPLAY_LAYOUT,
        [0x241e33, 0x39304d, 0x302941, 0x4c3a65, 0x31283f, 0x3e304e],
        [
            0xf5ebfa, 0xe7daef, 0xbba8c9, 0x6d5688, 0xe9b6d1, 0xa5d9c7, 0xe5c99b,
        ],
        [0xa1d7b7, 0xbcb7f3, 0xe5acd1, 0x8fcedf, 0xf0b390, 0xe5c99b],
        ["Sans", "Serif"],
        [13, 1, 10, 20, 1, 3],
        [1.6, 0.6, 0.08, 1.6, 0.3],
        &[2.0, 3.0],
        true,
        false,
    ),
    crafted(
        "forest",
        "Forest Fieldnotes",
        "Moss paper, organic rounded cards, ink-green filled readings",
        DISPLAY_LAYOUT,
        [0xe1e8d8, 0xcdd8bf, 0xd3dec6, 0xbacba7, 0xf2f6e8, 0xe9f0dc],
        [
            0x2e402b, 0x3f5039, 0x68755e, 0xa0b091, 0x77533b, 0x47683e, 0x746134,
        ],
        [0x4b7441, 0x72648e, 0x8b6b3a, 0x31717a, 0xa5573c, 0x7d6131],
        ["Sans", "Serif"],
        [10, 1, 10, 20, 1, 3],
        [1.7, 0.5, 0.12, 1.7, 0.35],
        &[],
        false,
        false,
    ),
    crafted(
        "cobalt",
        "Cobalt Instruments",
        "Sharp electric blue frame, compact sans and bright point traces",
        INSTRUMENT_LAYOUT,
        [0x121c35, 0x1c3059, 0x17284b, 0x294579, 0x1b2945, 0x24365a],
        [
            0xeaf0ff, 0xd7e2f7, 0xa7badb, 0x5274b4, 0xaac7ff, 0x83d7c5, 0xf1ce8c,
        ],
        [0x8bd9b3, 0xb4c0ff, 0xdeb2ed, 0x88d5fa, 0xf6b292, 0xf1ce8c],
        ["Sans", "Sans"],
        [5, 2, 9, 20, 1, 5],
        [2.0, 0.7, 0.0, 1.8, 0.35],
        &[],
        true,
        false,
    ),
    crafted(
        "sand",
        "Sandstone",
        "Desert linen with thin rust outlines, airy serif display",
        DISPLAY_LAYOUT,
        [0xece4d7, 0xded1bb, 0xe4d8c4, 0xcfbb99, 0xfaf3e7, 0xf3e7d4],
        [
            0x493b30, 0x55483c, 0x796b5c, 0xb8a48c, 0x966443, 0x597167, 0x8e663c,
        ],
        [0x557461, 0x7b648e, 0x926e37, 0x3a7186, 0xa8563c, 0x956539],
        ["Sans", "Serif"],
        [7, 1, 10, 21, 0, 0],
        [1.4, 0.5, 0.0, 1.5, 0.28],
        &[],
        false,
        false,
    ),
    crafted(
        "mint",
        "Mint Modern",
        "Fresh flat aqua, smooth pill corners and modest fills",
        QUIET_LAYOUT,
        [0xdeeeea, 0xc9e1db, 0xd3e7e0, 0xb8d5cd, 0xf1faf6, 0xe8f5ee],
        [
            0x29433d, 0x36534b, 0x617d73, 0x9fbdb1, 0x78658f, 0x347362, 0x886343,
        ],
        [0x387d60, 0x6c67a0, 0x97673d, 0x277b91, 0xac5547, 0x89613c],
        ["Sans", "Sans"],
        [16, 1, 10, 20, 1, 3],
        [1.9, 0.5, 0.1, 1.7, 0.3],
        &[],
        false,
        false,
    ),
    crafted(
        "noir",
        "Noir Cinema",
        "Graphite monochrome surfaces, serif display, restrained gold ink",
        DISPLAY_LAYOUT,
        [0x17191c, 0x2b2d30, 0x232529, 0x393b40, 0x242629, 0x303236],
        [
            0xf2eee5, 0xdedbd3, 0xb6b3ab, 0x62625e, 0xd7bd88, 0xb9c6b4, 0xcbbbe1,
        ],
        [0xb4c5aa, 0xbcbfe1, 0xd8bd96, 0xa7c8d5, 0xdfb492, 0xcbbbe1],
        ["Sans", "Serif"],
        [1, 1, 10, 21, 0, 0],
        [1.3, 0.5, 0.0, 1.5, 0.25],
        &[],
        false,
        false,
    ),
    crafted(
        "arcade",
        "Arcade Midnight",
        "Indigo arcade panels, monospaced headers, neon dashed traces",
        INSTRUMENT_LAYOUT,
        [0x17152d, 0x2b2550, 0x221f41, 0x40336b, 0x25203f, 0x30274f],
        [
            0xf4ecff, 0xded3f6, 0xb5a5d5, 0x7355a0, 0xf3b3e3, 0x92ddc9, 0xf1d18f,
        ],
        [0x90e0b7, 0xb4b7ff, 0xe6abe5, 0x82d7f1, 0xf7b595, 0xf1d18f],
        ["Sans", "Monospace"],
        [6, 2, 9, 19, 1, 3],
        [2.0, 0.8, 0.06, 2.0, 0.3],
        &[5.0, 2.0],
        true,
        true,
    ),
];
pub fn index(id: &str) -> Option<usize> {
    THEMES.iter().position(|theme| theme.id == id)
}
pub fn cycle(current: usize, delta: i32) -> usize {
    (current as i32 + delta).rem_euclid(THEMES.len() as i32) as usize
}

/// The original stylesheet is returned byte-for-byte; alternative rules only use registry tokens.
pub fn css(theme: &Theme) -> String {
    let base = include_str!("style.css");
    if theme.id == "original" {
        return base.into();
    }
    let c = |v: Color| v.css();
    format!(
        r#"{base}
window {{ background: {bg}; color: {body}; font-family: "{font}"; }}
.stripe {{ background: {bg}; }}
.stripe-header {{ background: {header}; border-bottom-color: {border}; }}
.footer {{ background: {footer}; border-top-color: {border}; }}
.stripe.window-focused .stripe-header, .stripe.window-focused .footer {{ background: {focus}; border-color: {good}; }}
.kicker, .stripe.window-focused .kicker, .section-title, .section-count, .footer-label, .footer-right {{ color: {muted}; }}
.headline, .card-title {{ color: {title}; font-family: "{title_font}"; font-weight: {weight}; }}
.card-title {{ font-size: {title_size}px; }}
.usage-card {{ background: {card}; border: {border_width}px solid {border}; border-radius: {radius}px; padding: {padding}px; box-shadow: {shadow}; }}
.usage-card.featured {{ background: {featured}; border-color: {provider}; }}
.provider-card .card-kicker {{ color: {provider}; }}
.local-card .card-kicker {{ color: {local}; }}
.remote-card .card-kicker {{ color: {remote}; }}
.card-subtitle, .metric-detail, .source-note, .axis-label, .footnote, .empty-state {{ color: {muted}; }}
.metric-label, .metric-value, .core-label, .day-label, .day-temperature, .hourly-details {{ color: {body}; }}
.state, .pace-label.pace-under {{ color: {good}; }}
.state.muted, .pace-label.pace-over {{ color: {warning}; }}
.pace-label.pace-even, .pace-time {{ color: {neutral}; }}
.weather-temperature {{ color: {temperature}; }}
progressbar.meter trough {{ background: {border}; }}
progressbar.meter progress, progressbar.meter.pace-under progress {{ background: {good}; }}
progressbar.meter.pace-over progress {{ background: {warning}; }}
progressbar.meter.pace-even progress {{ background: {neutral}; }}
.refresh-button, .forecast-detail button, .theme-selector button, .shortcut-close {{ background: {header}; color: {body}; }}
.refresh-button:hover, .theme-selector button:hover, .shortcut-close:hover {{ background: {focus}; }}
.unavailable, .error-banner {{ background: {header}; color: {warning}; }}
.shortcuts-panel, popover contents {{ background: {card}; color: {body}; border-color: {border}; }}
.shortcut-key {{ color: {good}; }}
.shortcut-description {{ color: {body}; }}
"#,
        bg = c(theme.background),
        body = c(theme.body),
        font = theme.font,
        header = c(theme.header),
        footer = c(theme.footer),
        border = c(theme.border),
        focus = c(theme.focus),
        good = c(theme.good),
        muted = c(theme.muted),
        title = c(theme.title),
        title_font = theme.title_font,
        weight = theme.title_weight,
        title_size = theme.title_size,
        card = c(theme.card),
        border_width = theme.border_width,
        radius = theme.radius,
        padding = theme.padding,
        shadow = if theme.shadow {
            "0 2px 3px alpha(black, 0.15)"
        } else {
            "none"
        },
        featured = c(theme.featured),
        provider = c(theme.provider),
        local = c(theme.local),
        remote = c(theme.remote),
        warning = c(theme.warning),
        neutral = c(theme.neutral),
        temperature = c(theme.temperature)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curated_layouts_keep_original_compact_and_vary_multiple_components() {
        assert_eq!(THEMES.len(), 21);
        assert_eq!(ORIGINAL.layout.cpu, CpuLayout::Compact);
        assert_eq!(ORIGINAL.layout.weather, WeatherLayout::CurveRows);
        assert_eq!(ORIGINAL.layout.telemetry, TelemetryLayout::Stacked);
        assert_eq!(ORIGINAL.layout.card_spacing, 0);
        for cpu in [CpuLayout::Compact, CpuLayout::Roomy] {
            assert!(THEMES.iter().any(|t| t.layout.cpu == cpu));
        }
        for weather in [WeatherLayout::CurveRows, WeatherLayout::ConditionStrip] {
            assert!(THEMES.iter().any(|t| t.layout.weather == weather));
        }
        for telemetry in [TelemetryLayout::Inline, TelemetryLayout::Stacked] {
            assert!(THEMES.iter().any(|t| t.layout.telemetry == telemetry));
        }
        assert_eq!(
            THEMES[index("retro").unwrap()].layout.cpu,
            CpuLayout::Compact
        );
        assert_eq!(THEMES[index("cloud").unwrap()].layout.cpu, CpuLayout::Roomy);
        for t in THEMES {
            assert!(t.layout.card_spacing <= 4);
        }
    }

    #[test]
    fn registry_is_complete_safe_and_stylistically_distinct() {
        assert_eq!(THEMES.len(), 21);
        let mut ids = std::collections::HashSet::new();
        let mut styles = std::collections::HashSet::new();
        for t in THEMES {
            assert!(ids.insert(t.id));
            assert!(t.id.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'));
            assert!(!t.name.is_empty() && !t.direction.is_empty());
            assert!(styles.insert(format!(
                "{:?}",
                (
                    t.font,
                    t.title_font,
                    t.radius,
                    t.border_width,
                    t.title_size,
                    t.title_weight,
                    t.plot.width,
                    t.plot.dash,
                    t.plot.points,
                    t.plot.fill_alpha,
                    t.plot.grid_rows,
                    t.shadow
                )
            )));
            for color in t.plot.series.into_iter().chain([
                t.background,
                t.header,
                t.footer,
                t.card,
                t.title,
                t.body,
                t.muted,
                t.border,
                t.provider,
                t.local,
                t.remote,
                t.good,
                t.warning,
                t.neutral,
                t.temperature,
                t.plot.grid,
                t.plot.background,
                t.plot.glyph,
                t.plot.range_track,
                t.plot.range_low,
                t.plot.range_high,
                t.plot.marker_outline,
                t.plot.marker,
            ]) {
                for n in [color.0, color.1, color.2, color.3] {
                    assert!(n.is_finite() && (0.0..=1.0).contains(&n));
                }
            }
            assert!(t.plot.width >= 1.0 && t.plot.width <= 3.0 && t.plot.fill_alpha <= 0.2);
            if t.id != "original" {
                assert_ne!(t.plot.series[0], t.plot.series[1]);
                assert_ne!(t.plot.series[3], t.plot.series[4]);
            }
        }
    }
    #[test]
    fn original_css_and_graph_tokens_are_unchanged() {
        assert_eq!(css(&ORIGINAL), include_str!("style.css"));
        assert_eq!(ORIGINAL.plot.series, [Color::rgb(0.53, 0.82, 0.66); 6]);
        assert_eq!(ORIGINAL.plot.grid, Color(0.65, 0.8, 0.76, 0.16));
        assert_eq!((ORIGINAL.plot.width, ORIGINAL.plot.grid_width), (1.5, 0.7));
        assert_eq!(
            (ORIGINAL.plot.glyph, ORIGINAL.plot.glyph_width),
            (Color::rgb(0.69, 0.87, 0.75), 1.7)
        );
        assert_eq!(ORIGINAL.plot.range_track, ORIGINAL.plot.grid);
        assert_eq!(ORIGINAL.plot.range_low, ORIGINAL.plot.series[0]);
        assert_eq!(ORIGINAL.plot.range_high, ORIGINAL.plot.series[0]);
        assert_eq!(ORIGINAL.plot.range_width, 4.0);
        assert!(
            !ORIGINAL.plot.points
                && ORIGINAL.plot.dash.is_empty()
                && ORIGINAL.plot.fill_alpha == 0.0
        );
    }
    #[test]
    fn alternative_buttons_include_the_close_guide() {
        for theme in THEMES.iter().filter(|theme| theme.id != "original") {
            let css = css(theme);
            assert!(css.contains(".shortcut-close { background:"));
            assert!(css.contains(".shortcut-close:hover { background:"));
        }
        assert_eq!(css(&ORIGINAL), include_str!("style.css"));
    }
    #[test]
    fn browsing_wraps_and_series_stay_independent() {
        assert_eq!(cycle(0, -1), 20);
        assert_eq!(cycle(20, 1), 0);
        assert_eq!(cycle(3, 21), 3);
        assert_eq!(index("original"), Some(0));
        assert_eq!(index("unknown"), None);
        for (label, series) in [
            ("Total · 5%", Series::Cpu),
            ("Core 1", Series::Cpu),
            ("RAM · 40%", Series::Ram),
            ("Swap · 0%", Series::Swap),
            ("Receive · auto Y", Series::Rx),
            ("Transmit · auto Y", Series::Tx),
        ] {
            assert_eq!(Series::from_label(label), series);
        }
    }
}
