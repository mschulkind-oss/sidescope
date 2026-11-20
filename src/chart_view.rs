//! Native chart cards: a pure, display-free model built from a source
//! snapshot, Cairo drawing that renders into any surface, and thin GTK widgets
//! over both. Nothing here starts work, runs commands or knows what a source
//! measures; every label comes from the dataset and the card configuration.

mod a11y;
mod draw;
mod format;
mod model;
mod palette;
mod rank;
mod widget;

pub use draw::{draw_plot, hit_test, plot_frame};
pub use model::{
    Bar, BarSegment, ChartCardModel, Headline, LegendEntry, LineSeries, PlotModel, PlotPoint,
    RankedRow, RowKind, build_model, present_card,
};
pub use palette::{CATEGORY_COUNT, ChartPalette, Swatch, css, swatch_for};
pub use widget::{CardLayout, card_widget, update_card_widget};
