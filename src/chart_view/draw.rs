//! Display-free Cairo drawing and hit testing. Incomplete data carries
//! non-color cues (hatching, hollow markers, dashed outlines, footnote
//! asterisks) so nothing depends on color alone.

use super::model::{Axes, Bar, ChartCardModel, LineSeries, PlotModel, RankedRow, RowKind};
use super::palette::ChartPalette;
use crate::chart::dataset::Quality;
use crate::theme::Color;
use gtk::cairo::{Context, FontSlant, FontWeight};

const LEFT: f64 = 34.0;
const RIGHT: f64 = 6.0;
const TOP: f64 = 6.0;
const BOTTOM: f64 = 18.0;
const MAX_MARKED_POINTS: usize = 48;
const RANKED_ROW_LIMIT: f64 = 28.0;

fn paint(cr: &Context, color: Color, alpha: f64) {
    cr.set_source_rgba(color.0, color.1, color.2, color.3 * alpha);
}

fn text_width(cr: &Context, text: &str) -> f64 {
    cr.text_extents(text)
        .map_or(0.0, |extents| extents.x_advance())
}

fn ellipsize(cr: &Context, text: &str, max: f64) -> String {
    if text_width(cr, text) <= max {
        return text.to_string();
    }
    let mut kept: String = text.to_string();
    while !kept.is_empty() && text_width(cr, &format!("{kept}…")) > max {
        kept.pop();
    }
    format!("{kept}…")
}

fn show(cr: &Context, text: &str, x: f64, y: f64) {
    cr.move_to(x, y);
    let _ = cr.show_text(text);
}

/// Diagonal hatch over a rectangle in a contrasting color.
fn hatch(cr: &Context, x: f64, y: f64, w: f64, h: f64, color: Color) {
    let _ = cr.save();
    cr.rectangle(x, y, w, h);
    cr.clip();
    paint(cr, color, 0.9);
    cr.set_line_width(1.0);
    let mut offset = -h;
    while offset < w {
        cr.move_to(x + offset, y + h);
        cr.line_to(x + offset + h, y);
        offset += 4.0;
    }
    let _ = cr.stroke();
    let _ = cr.restore();
}

fn marked(quality: Quality) -> bool {
    quality != Quality::Complete
}

struct Frame {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Frame {
    fn of(width: f64, height: f64) -> Frame {
        Frame {
            x: LEFT,
            y: TOP,
            w: (width - LEFT - RIGHT).max(1.0),
            h: (height - TOP - BOTTOM).max(1.0),
        }
    }
    fn px(&self, fraction: f64) -> f64 {
        self.x + fraction * self.w
    }
    fn py(&self, axes_scale: &crate::chart::geometry::ValueScale, value: f64) -> f64 {
        self.y + self.h * (1.0 - axes_scale.fraction(value))
    }
}

fn draw_axes(
    cr: &Context,
    axes: &Axes,
    palette: &ChartPalette,
    frame: &Frame,
    width: f64,
    height: f64,
) {
    cr.set_line_width(0.8);
    for (tick, label) in axes.scale.ticks().iter().zip(&axes.y_labels) {
        let y = frame.py(&axes.scale, *tick);
        paint(cr, palette.grid, 1.0);
        cr.move_to(frame.x, y);
        cr.line_to(frame.x + frame.w, y);
        let _ = cr.stroke();
        paint(cr, palette.muted, 1.0);
        let label_width = text_width(cr, label);
        show(cr, label, frame.x - 4.0 - label_width, y + 3.5);
    }
    if axes.scale.min <= 0.0 && axes.scale.max >= 0.0 {
        let y = frame.py(&axes.scale, 0.0);
        paint(cr, palette.baseline, 1.0);
        cr.set_line_width(1.2);
        cr.move_to(frame.x, y);
        cr.line_to(frame.x + frame.w, y);
        let _ = cr.stroke();
    }
    paint(cr, palette.muted, 1.0);
    for (fraction, label) in &axes.x_labels {
        let label_width = text_width(cr, label);
        let x =
            (frame.px(*fraction) - label_width / 2.0).clamp(0.0, (width - label_width).max(0.0));
        show(cr, label, x, height - 4.0);
    }
}

fn draw_lines(
    cr: &Context,
    series: &[LineSeries],
    axes: &Axes,
    palette: &ChartPalette,
    frame: &Frame,
) {
    for line in series {
        let color = palette.color(line.swatch);
        let points: usize = line.segments.iter().map(Vec::len).sum();
        paint(cr, color, 1.0);
        cr.set_line_width(1.5);
        cr.set_line_join(gtk::cairo::LineJoin::Round);
        for segment in &line.segments {
            if segment.len() < 2 {
                continue;
            }
            for (index, point) in segment.iter().enumerate() {
                let (x, y) = (frame.px(point.x), frame.py(&axes.scale, point.value));
                if index == 0 {
                    cr.move_to(x, y);
                } else {
                    cr.line_to(x, y);
                }
            }
            let _ = cr.stroke();
        }
        for segment in &line.segments {
            for point in segment {
                let (x, y) = (frame.px(point.x), frame.py(&axes.scale, point.value));
                if marked(point.quality) {
                    cr.arc(x, y, 3.2, 0.0, std::f64::consts::TAU);
                    paint(cr, palette.background, 1.0);
                    let _ = cr.fill_preserve();
                    paint(cr, color, 1.0);
                    cr.set_line_width(1.5);
                    let _ = cr.stroke();
                } else if segment.len() == 1 {
                    cr.arc(x, y, 3.0, 0.0, std::f64::consts::TAU);
                    paint(cr, color, 1.0);
                    let _ = cr.fill();
                } else if points <= MAX_MARKED_POINTS {
                    cr.arc(x, y, 1.8, 0.0, std::f64::consts::TAU);
                    paint(cr, color, 1.0);
                    let _ = cr.fill();
                }
            }
        }
    }
}

fn draw_bars(
    cr: &Context,
    bars: &[Bar],
    axes: &Axes,
    palette: &ChartPalette,
    frame: &Frame,
    selected: Option<usize>,
) {
    for bar in bars {
        let x0 = frame.px(bar.x0) + 1.0;
        let x1 = (frame.px(bar.x1) - 1.0).max(x0 + 1.5);
        let mut top = f64::MAX;
        let mut bottom = f64::MIN;
        for segment in &bar.segments {
            let (a, b) = (
                frame.py(&axes.scale, segment.low),
                frame.py(&axes.scale, segment.high),
            );
            let (y, h) = (a.min(b), (a - b).abs().max(1.0));
            paint(cr, palette.color(segment.swatch), 1.0);
            cr.rectangle(x0, y, x1 - x0, h);
            let _ = cr.fill();
            top = top.min(y);
            bottom = bottom.max(y + h);
        }
        if bar.segments.is_empty() {
            continue;
        }
        if marked(bar.quality) {
            hatch(cr, x0, top, x1 - x0, bottom - top, palette.background);
            paint(cr, palette.text, 0.9);
            cr.set_line_width(1.0);
            cr.set_dash(&[3.0, 2.0], 0.0);
            cr.rectangle(
                x0 + 0.5,
                top + 0.5,
                (x1 - x0 - 1.0).max(0.5),
                (bottom - top - 1.0).max(0.5),
            );
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);
        }
    }
    if let Some(bar) = selected.and_then(|index| bars.get(index)) {
        paint(cr, palette.text, 0.9);
        cr.set_line_width(1.5);
        cr.rectangle(
            frame.px(bar.x0) + 0.5,
            frame.y,
            (frame.px(bar.x1) - frame.px(bar.x0) - 1.0).max(1.0),
            frame.h,
        );
        let _ = cr.stroke();
    }
}

fn ranked_row_height(rows: usize, height: f64) -> f64 {
    (height / rows.max(1) as f64).min(RANKED_ROW_LIMIT)
}

fn row_value_text(row: &RankedRow) -> String {
    match &row.share {
        Some(share) => format!("{}  {share}", row.value_text),
        None => row.value_text.clone(),
    }
}

fn draw_ranked(
    cr: &Context,
    rows: &[RankedRow],
    scale: &crate::chart::geometry::ValueScale,
    palette: &ChartPalette,
    width: f64,
    height: f64,
    selected: Option<usize>,
) {
    let row_h = ranked_row_height(rows.len(), height);
    let value_w = rows
        .iter()
        .map(|row| text_width(cr, &row_value_text(row)))
        .fold(0.0, f64::max)
        .min(width * 0.42);
    let label_w = (width * 0.3).clamp(48.0, 140.0);
    let bar_x = label_w + 8.0;
    let bar_w = (width - bar_x - value_w - 10.0).max(12.0);
    let zero_x = bar_x + scale.fraction(0.0) * bar_w;
    for (index, row) in rows.iter().enumerate() {
        let top = index as f64 * row_h;
        let middle = top + row_h / 2.0;
        if selected == Some(index) {
            paint(cr, palette.text, 0.14);
            cr.rectangle(0.0, top, width, row_h);
            let _ = cr.fill();
        }
        paint(cr, palette.text, 1.0);
        let label = ellipsize(cr, &row.label, label_w);
        show(cr, &label, 0.0, middle + 3.5);
        if let Some(value) = row.value {
            let end = bar_x + scale.fraction(value) * bar_w;
            let (x, w) = (zero_x.min(end), (end - zero_x).abs().max(1.5));
            let (y, h) = (top + row_h * 0.2, row_h * 0.6);
            paint(cr, palette.color(row.swatch), 1.0);
            cr.rectangle(x, y, w, h);
            let _ = cr.fill();
            if marked(row.quality) {
                hatch(cr, x, y, w, h, palette.background);
            }
            if marked(row.quality) || row.kind == RowKind::Unknown {
                paint(cr, palette.text, 0.9);
                cr.set_line_width(1.0);
                cr.set_dash(&[3.0, 2.0], 0.0);
                cr.rectangle(x + 0.5, y + 0.5, (w - 1.0).max(0.5), (h - 1.0).max(0.5));
                let _ = cr.stroke();
                cr.set_dash(&[], 0.0);
            }
        }
        paint(cr, palette.text, 1.0);
        let text = row_value_text(row);
        let w = text_width(cr, &text);
        show(cr, &text, width - w, middle + 3.5);
    }
    if scale.min < 0.0 {
        paint(cr, palette.baseline, 1.0);
        cr.set_line_width(1.2);
        cr.move_to(zero_x, 0.0);
        cr.line_to(zero_x, rows.len() as f64 * row_h);
        let _ = cr.stroke();
    }
}

fn setup_text(cr: &Context, palette: &ChartPalette) {
    cr.select_font_face(&palette.font, FontSlant::Normal, FontWeight::Normal);
    cr.set_font_size(10.0);
}

/// Cairo drawing is display-free so tests render to an ImageSurface.
pub fn draw_plot(
    cr: &Context,
    model: &ChartCardModel,
    palette: &ChartPalette,
    width: f64,
    height: f64,
    selected: Option<usize>,
) {
    let _ = cr.save();
    setup_text(cr, palette);
    match model.plot() {
        PlotModel::Empty => {}
        PlotModel::Lines { series, axes } => {
            let frame = Frame::of(width, height);
            draw_axes(cr, axes, palette, &frame, width, height);
            draw_lines(cr, series, axes, palette, &frame);
            if let Some(center) = selected.and_then(|index| axes.centers.get(index)) {
                paint(cr, palette.text, 0.6);
                cr.set_line_width(1.0);
                cr.move_to(frame.px(*center), frame.y);
                cr.line_to(frame.px(*center), frame.y + frame.h);
                let _ = cr.stroke();
            }
        }
        PlotModel::Bars { bars, axes } => {
            let frame = Frame::of(width, height);
            draw_axes(cr, axes, palette, &frame, width, height);
            draw_bars(cr, bars, axes, palette, &frame, selected);
        }
        PlotModel::Ranked { rows, scale } => {
            draw_ranked(cr, rows, scale, palette, width, height, selected);
        }
    }
    let _ = cr.restore();
}

/// The selectable position under a pointer, if any.
pub fn hit_test(model: &ChartCardModel, width: f64, height: f64, x: f64, y: f64) -> Option<usize> {
    match model.plot() {
        PlotModel::Empty => None,
        PlotModel::Lines { axes, .. } | PlotModel::Bars { axes, .. } => {
            let frame = Frame::of(width, height);
            let fraction = (x - frame.x) / frame.w;
            axes.centers
                .iter()
                .enumerate()
                .min_by(|a, b| (a.1 - fraction).abs().total_cmp(&(b.1 - fraction).abs()))
                .map(|(index, _)| index)
        }
        PlotModel::Ranked { rows, .. } => {
            let row_h = ranked_row_height(rows.len(), height);
            let index = (y / row_h).floor();
            (index >= 0.0 && (index as usize) < rows.len()).then_some(index as usize)
        }
    }
}

/// Left, top, width and height of the axis area of time charts, so callers
/// can relate model fractions to pixels.
pub fn plot_frame(width: f64, height: f64) -> (f64, f64, f64, f64) {
    let frame = Frame::of(width, height);
    (frame.x, frame.y, frame.w, frame.h)
}
