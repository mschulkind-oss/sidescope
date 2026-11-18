//! Opt-in density and measured, bounded screen-fit planning. Never hides overflow.
use serde::Deserialize;

#[derive(Debug, Default, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Density {
    Comfortable,
    #[default]
    ScreenFit,
}
impl Density {
    pub fn toggle(self) -> Self {
        match self {
            Self::Comfortable => Self::ScreenFit,
            Self::ScreenFit => Self::Comfortable,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FitPlan {
    pub columns: Vec<usize>,
    pub height: i32,
    pub overflows: bool,
    pub core_columns: usize,
}

/// Heights come from GTK at the actual card width. CPU spans both columns;
/// the rest retain source order and stable shorter-column placement.
pub fn plan(
    heights: &[i32],
    cpu: Option<usize>,
    cpu_height: i32,
    width: i32,
    available: i32,
) -> FitPlan {
    let wide = width > 500;
    let mut totals = [0i32; 2];
    let mut columns = Vec::with_capacity(heights.len());
    for (index, &height) in heights.iter().enumerate() {
        if wide && Some(index) == cpu {
            columns.push(2);
            continue;
        }
        let column = usize::from(wide && totals[1] < totals[0]);
        if totals[column] > 0 {
            totals[column] += 8;
        }
        totals[column] += height.max(0);
        columns.push(column);
    }
    let mut height = *totals.iter().max().unwrap();
    if wide && cpu.is_some() {
        height += cpu_height.max(0) + if height > 0 { 8 } else { 0 };
    }
    FitPlan {
        columns,
        height,
        overflows: height > available.max(0),
        core_columns: if wide { 4 } else { 2 },
    }
}

/// Two measured candidates only; readable wider core traces win when they fit.
pub fn choose_core_plan(four: FitPlan, mut three: FitPlan) -> FitPlan {
    if three.overflows {
        four
    } else {
        three.core_columns = 3;
        three
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn density_is_closed_theme_independent_and_defaults_to_screen_fit() {
        let default = crate::parse_config("").unwrap();
        assert_eq!(default.density, Density::ScreenFit);
        for theme in crate::theme::THEMES {
            let config =
                crate::parse_config(&format!("theme = {:?}\ndensity = 'screen-fit'", theme.id))
                    .unwrap();
            assert_eq!(config.density, Density::ScreenFit);
            assert_eq!(config.density.toggle(), Density::Comfortable);
            assert_eq!(config.theme.as_deref(), Some(theme.id));
        }
        assert!(crate::parse_config("density = 'tiny'").is_err());
        assert!(crate::parse_config("density = 2").is_err());
        assert_eq!(
            crate::parse_config("density = 'comfortable'")
                .unwrap()
                .density,
            Density::Comfortable
        );
    }
    #[test]
    fn measured_packing_spans_cpu_without_losing_any_cards() {
        let p = plan(&[100, 200, 400, 100, 200], Some(2), 180, 616, 500);
        assert_eq!(p.columns, [0, 1, 2, 0, 1]);
        assert_eq!(p.height, 596);
        assert!(p.overflows);
        assert_eq!(p.core_columns, 4);
        assert!(!plan(&[100, 200, 400, 100, 200], Some(2), 180, 616, 596).overflows);
    }
    #[test]
    fn actual_height_selects_three_or_four_cells_without_unbounded_compression() {
        let four = || plan(&[100, 100], Some(0), 150, 612, 300);
        let three = |height| plan(&[100, 100], Some(0), height, 612, 300);
        assert_eq!(choose_core_plan(four(), three(192)).core_columns, 3);
        assert_eq!(choose_core_plan(four(), three(193)).core_columns, 4);
        let short = choose_core_plan(
            plan(&[100], Some(0), 150, 612, 100),
            plan(&[100], Some(0), 192, 612, 100),
        );
        assert!(short.overflows);
        assert_eq!(short.core_columns, 4);
    }
    #[test]
    fn narrow_high_core_and_short_windows_honestly_overflow() {
        let narrow = plan(&[100, 200, 900], Some(2), 300, 354, 720);
        assert_eq!(narrow.columns, [0, 0, 0]);
        assert_eq!(narrow.core_columns, 2);
        assert!(narrow.overflows);
        assert!(plan(&[100, 200, 900], Some(2), 1400, 616, 1200).overflows);
        assert_eq!(plan(&[], None, 0, 616, 0).height, 0);
    }
}
