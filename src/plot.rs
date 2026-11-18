//! Shared plot geometry; drawing and tests must use the same coordinates.

/// 120 fixed two-second slots. Warm-up leaves unused space on the left.
pub fn history_x(index: usize, len: usize, width: f64) -> f64 {
    let slots = crate::dashboard::HISTORY_LIMIT;
    let len = len.min(slots);
    let slot = slots.saturating_sub(len) + index.min(len.saturating_sub(1));
    slot as f64 * width.max(0.0) / (slots - 1) as f64
}

/// A missing slot breaks the line; callers must not connect across None.
pub fn history_points(
    samples: &[Option<u64>],
    ceiling: u64,
    width: f64,
    height: f64,
) -> Vec<Option<(f64, f64)>> {
    let samples = &samples[samples
        .len()
        .saturating_sub(crate::dashboard::HISTORY_LIMIT)..];
    samples
        .iter()
        .enumerate()
        .map(|(i, value)| {
            value.map(|v| {
                (
                    history_x(i, samples.len(), width),
                    height.max(0.0) * (1.0 - v.min(ceiling) as f64 / ceiling.max(1) as f64),
                )
            })
        })
        .collect()
}

pub fn temperature_range(values: impl Iterator<Item = Option<i32>>) -> Option<(i32, i32)> {
    values.flatten().fold(None, |range, value| {
        Some(match range {
            None => (value, value),
            Some((lo, hi)) => (lo.min(value), hi.max(value)),
        })
    })
}

/// Auto range for a temperature channel, in millidegrees Celsius.
/// The floor/ceiling round outward to 5 °C steps with a minimum 10 °C span,
/// derived from the history passed in. Recomputing over an evicting window
/// can shrink the result when an early extreme leaves the window, so callers
/// needing session stability must hold the returned range and only expand
/// it (see the sampler's held range in sensors.rs). None when no slot has
/// a value.
pub fn temperature_autorange(values: impl Iterator<Item = Option<i64>>) -> Option<(i64, i64)> {
    const STEP: i64 = 5000;
    const MIN_SPAN: i64 = 10_000;
    let (min, max) = values.flatten().fold(None::<(i64, i64)>, |range, value| {
        Some(match range {
            None => (value, value),
            Some((lo, hi)) => (lo.min(value), hi.max(value)),
        })
    })?;
    let floor = min.div_euclid(STEP) * STEP;
    let mut ceiling = max.div_euclid(STEP) * STEP;
    if max.rem_euclid(STEP) != 0 {
        ceiling += STEP;
    }
    while ceiling - floor < MIN_SPAN {
        ceiling += STEP;
    }
    Some((floor, ceiling))
}

/// Signed ranged points for temperature plots: a missing slot breaks the
/// line, values clamp to the stable range, and a degenerate floor==ceiling
/// range draws a midline rather than dividing by zero.
pub fn ranged_history_points(
    samples: &[Option<i64>],
    floor: i64,
    ceiling: i64,
    width: f64,
    height: f64,
) -> Vec<Option<(f64, f64)>> {
    let samples = &samples[samples
        .len()
        .saturating_sub(crate::dashboard::HISTORY_LIMIT)..];
    samples
        .iter()
        .enumerate()
        .map(|(i, value)| {
            value.map(|v| {
                let y = if ceiling <= floor {
                    0.5
                } else {
                    1.0 - (v.clamp(floor, ceiling) - floor) as f64 / (ceiling - floor) as f64
                };
                (history_x(i, samples.len(), width), y * height.max(0.0))
            })
        })
        .collect()
}

/// Forecast slots retain their time position even when temperatures are null.
pub fn forecast_points(values: &[Option<i32>], width: f64, height: f64) -> Vec<Option<(f64, f64)>> {
    let values = &values[..values.len().min(12)];
    let Some((low, high)) = temperature_range(values.iter().copied()) else {
        return vec![None; values.len()];
    };
    values
        .iter()
        .enumerate()
        .map(|(i, value)| {
            value.map(|v| {
                let y = if low == high {
                    0.5
                } else {
                    1.0 - (f64::from(v) - f64::from(low)) / (f64::from(high) - f64::from(low))
                };
                (i as f64 * width.max(0.0) / 11.0, y * height.max(0.0))
            })
        })
        .collect()
}

pub fn daily_range(
    low: Option<i32>,
    high: Option<i32>,
    bounds: Option<(i32, i32)>,
    width: f64,
) -> Option<(f64, f64)> {
    let (lo, hi, (min, max)) = (low?, high?, bounds?);
    if lo > hi || min > max || lo < min || hi > max {
        return None;
    }
    if min == max {
        return Some((width.max(0.0) / 2.0, width.max(0.0) / 2.0));
    }
    let x = |n| f64::from(n) - f64::from(min);
    let scale = width.max(0.0) / (f64::from(max) - f64::from(min));
    Some((x(lo) * scale, x(hi) * scale))
}

pub fn core_label_parts(label: &str) -> (&str, &str) {
    let label = label.strip_prefix("Core ").unwrap_or(label);
    label.split_once(" · ").unwrap_or((label, ""))
}

/// Decorations need room between samples. Compact CPU panels prioritize the raw
/// signal; only style changes, never the sampled values or their coordinates.
pub fn effective_style(
    mut style: crate::theme::PlotStyle,
    spacing: f64,
    compact: bool,
) -> crate::theme::PlotStyle {
    if compact || spacing < 6.0 {
        style.points = false;
        style.dash = &[];
        style.fill_alpha = 0.0;
        style.width = style.width.min(if compact { 1.1 } else { 1.5 });
        style.grid_rows = if compact { 2 } else { style.grid_rows.min(3) };
        style.grid_width = style.grid_width.min(if compact { 0.5 } else { 0.7 });
        style.grid.3 = style.grid.3.min(if compact { 0.12 } else { 0.16 });
    }
    style
}

pub fn core_cell(index: usize) -> (i32, i32) {
    ((index % 2) as i32, (index / 2) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::HISTORY_LIMIT;

    #[test]
    fn retro_full_history_cannot_overlay_markers_dashes_or_fill_on_signal() {
        let retro = crate::theme::THEMES
            .iter()
            .find(|t| t.id == "retro")
            .unwrap();
        assert!(retro.plot.points && !retro.plot.dash.is_empty());
        let dense = effective_style(retro.plot, 80.0 / 119.0, true);
        assert!(
            !dense.points,
            "120 markers over an 80px core trace cause blobs"
        );
        assert!(dense.dash.is_empty());
        assert_eq!(dense.fill_alpha, 0.0);
    }

    #[test]
    fn dense_cpu_styles_are_continuous_thin_unfilled_and_quiet() {
        for theme in crate::theme::THEMES {
            for width in [125.0, 158.0, 280.0] {
                let style = effective_style(theme.plot, width / 119.0, true);
                assert!(style.width <= 1.1, "{} dense width", theme.id);
                assert!(style.dash.is_empty(), "{} dense dash", theme.id);
                assert!(!style.points, "{} dense markers", theme.id);
                assert_eq!(style.fill_alpha, 0.0);
                assert_eq!(style.grid_rows, 2);
                assert!(style.grid.3 <= 0.12 && style.grid_width <= 0.5);
            }
            assert_eq!(effective_style(theme.plot, 12.0, false), theme.plot);
            let dense = effective_style(theme.plot, 1.0, false);
            assert!(!dense.points && dense.dash.is_empty() && dense.fill_alpha == 0.0);
        }
        assert_eq!(
            effective_style(crate::theme::ORIGINAL.plot, 2.0, false),
            crate::theme::ORIGINAL.plot
        );
    }

    #[test]
    fn core_headers_separate_ids_from_right_aligned_current_values() {
        assert_eq!(core_label_parts("Core 0 · 1%"), ("0", "1%"));
        assert_eq!(core_label_parts("Core 11 · 100%"), ("11", "100%"));
        assert_eq!(
            core_label_parts("Core 255 · warming up"),
            ("255", "warming up")
        );
        use crate::theme::CpuLayout;
        assert_eq!(CpuLayout::Compact.core_height(), 24);
        assert_eq!(CpuLayout::Compact.row_max_height(), 32);
        assert_eq!(CpuLayout::Compact.row_spacing(), 2);
        assert_eq!(CpuLayout::Roomy.core_height(), 28);
        assert_eq!(CpuLayout::Roomy.row_max_height(), 48);
        assert_eq!(CpuLayout::Roomy.row_spacing(), 6);
    }

    #[test]
    fn missing_slots_percent_bounds_and_forecast_nulls() {
        assert!(history_points(&[], 100, 238.0, 30.0).is_empty());
        assert_eq!(
            history_points(&[Some(50)], 100, 238.0, 30.0),
            vec![Some((238.0, 15.0))]
        );
        let oversized: Vec<_> = (0..121).map(Some).collect();
        let bounded = history_points(&oversized, 120, 238.0, 120.0);
        assert_eq!(bounded.len(), 120);
        assert_eq!(bounded[119], Some((238.0, 0.0)));
        let points = history_points(&[Some(0), None, Some(100), Some(200)], 100, 238.0, 30.0);
        assert_eq!(
            points,
            vec![
                Some((232.0, 30.0)),
                None,
                Some((236.0, 0.0)),
                Some((238.0, 0.0))
            ]
        );
        assert_eq!(forecast_points(&[None; 12], 110.0, 40.0), vec![None; 12]);
        let mut temps = vec![Some(-100); 12];
        temps[1] = None;
        temps[11] = Some(100);
        let points = forecast_points(&temps, 110.0, 40.0);
        assert_eq!(points[0], Some((0.0, 40.0)));
        assert_eq!(points[1], None);
        assert_eq!(points[11], Some((110.0, 0.0)));
        assert_eq!(
            forecast_points(&[Some(20); 12], 110.0, 40.0)[11],
            Some((110.0, 20.0))
        );
        assert_eq!(
            daily_range(Some(10), Some(20), Some((0, 40)), 80.0),
            Some((20.0, 40.0))
        );
        assert_eq!(daily_range(None, Some(20), Some((0, 40)), 80.0), None);
        assert_eq!(daily_range(Some(30), Some(20), Some((0, 40)), 80.0), None);
        assert_eq!(
            daily_range(Some(10), Some(10), Some((10, 10)), 80.0),
            Some((40.0, 40.0))
        );
    }

    #[test]
    fn ranged_temperature_points_break_gaps_clamp_and_guard_degenerate_range() {
        let points = ranged_history_points(
            &[Some(-5000), Some(0), None, Some(5000)],
            -5000,
            5000,
            238.0,
            30.0,
        );
        assert_eq!(
            points,
            vec![
                Some((232.0, 30.0)),
                Some((234.0, 15.0)),
                None,
                Some((238.0, 0.0))
            ]
        );
        let clamped =
            ranged_history_points(&[Some(-99_999), Some(99_999)], -5000, 5000, 238.0, 30.0);
        assert_eq!(clamped, vec![Some((236.0, 30.0)), Some((238.0, 0.0))]);
        assert_eq!(
            ranged_history_points(&[Some(42_000)], 42_000, 42_000, 238.0, 30.0),
            vec![Some((238.0, 15.0))]
        );
        assert!(ranged_history_points(&[], 0, 10_000, 238.0, 30.0).is_empty());
        let oversized: Vec<_> = (0..130).map(|i| Some(i as i64 * 100)).collect();
        assert_eq!(
            ranged_history_points(&oversized, 0, 20_000, 238.0, 30.0).len(),
            120
        );
    }

    #[test]
    fn temperature_autorange_rounds_outward_enforces_span_and_holds_in_range() {
        assert_eq!(
            temperature_autorange(Vec::<Option<i64>>::new().into_iter()),
            None
        );
        assert_eq!(temperature_autorange([None, None].into_iter()), None);
        assert_eq!(
            temperature_autorange([Some(54_200)].into_iter()),
            Some((50_000, 60_000))
        );
        assert_eq!(
            temperature_autorange([Some(45_000), Some(50_000)].into_iter()),
            Some((45_000, 55_000))
        );
        assert_eq!(
            temperature_autorange([Some(-2500)].into_iter()),
            Some((-5000, 5000))
        );
        assert_eq!(
            temperature_autorange([Some(35_100), Some(77_900)].into_iter()),
            Some((35_000, 80_000))
        );
        let history = vec![Some(54_200), None, Some(55_100)];
        let range = temperature_autorange(history.iter().copied()).unwrap();
        for extra in [50_000, 54_200, 59_999, 60_000] {
            let mut extended = history.clone();
            extended.push(Some(extra));
            assert_eq!(
                temperature_autorange(extended.iter().copied()),
                Some(range),
                "in-range sample {extra} moved the range"
            );
        }
        let mut outside = history.clone();
        outside.push(Some(60_001));
        assert_ne!(temperature_autorange(outside.iter().copied()), Some(range));
    }

    #[test]
    fn core_grid_is_dense_and_preserves_order_at_high_counts() {
        assert_eq!(core_cell(0), (0, 0));
        assert_eq!(core_cell(1), (1, 0));
        assert_eq!(core_cell(2), (0, 1));
        assert_eq!(core_cell(255), (1, 127));
    }

    #[test]
    fn history_warmup_uses_fixed_slots_with_newest_at_right() {
        let width = 238.0;
        assert_eq!(history_x(0, 1, width), width);
        assert_eq!(history_x(0, 2, width), width - 2.0);
        assert_eq!(history_x(1, 2, width), width);
        assert_eq!(history_x(0, 60, width), 120.0);
        assert_eq!(history_x(59, 60, width), width);
        assert_eq!(history_x(0, HISTORY_LIMIT, width), 0.0);
        assert_eq!(history_x(119, HISTORY_LIMIT, width), width);
    }

    #[test]
    fn history_growth_and_rolling_move_old_samples_one_slot_not_stretch() {
        for len in 2..=120 {
            let spacing = history_x(1, len, 238.0) - history_x(0, len, 238.0);
            assert_eq!(spacing, 2.0);
            if len < 120 {
                assert_eq!(history_x(0, len + 1, 238.0), history_x(0, len, 238.0) - 2.0);
            }
        }
        assert_eq!(history_x(0, 120, 238.0), history_x(1, 120, 238.0) - 2.0);
    }
}
