//! Thin GTK widgets over the tested model. They never start work: a card is
//! built from a model, updated in place from a newer model, and keeps its open
//! Details popover and keyboard focus across updates.

use super::draw::{draw_plot, hit_test};
use super::model::{ChartCardModel, PlotModel};
use super::palette::ChartPalette;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardLayout {
    Wide,
    Narrow,
}

const STATE_KEY: &str = "sidescope-chart-card-state";

struct Parts {
    title: gtk::Label,
    headline_label: gtk::Label,
    headline_value: gtk::Label,
    headline_note: gtk::Label,
    subtitle: gtk::Label,
    badges: gtk::Label,
    banner: gtk::Label,
    message: gtk::Label,
    plot: gtk::DrawingArea,
    readout: gtk::Label,
    legend: gtk::Label,
    footnote: gtk::Label,
    table: gtk::TextBuffer,
    info: gtk::Label,
    details: gtk::MenuButton,
}

struct State {
    model: ChartCardModel,
    palette: ChartPalette,
    selected: Option<usize>,
    parts: Parts,
}

type Shared = Rc<RefCell<State>>;

fn label(class: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.add_css_class(class);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label
}

fn widget_name(card_id: &str) -> String {
    format!("chart-card-{card_id}")
}

fn set_text(label: &gtk::Label, text: Option<&str>) {
    label.set_label(text.unwrap_or(""));
    label.set_visible(text.is_some_and(|text| !text.is_empty()));
}

fn legend_markup(model: &ChartCardModel, palette: &ChartPalette) -> String {
    let mut entries: Vec<String> = model
        .legend()
        .iter()
        .map(|entry| {
            let color = palette.color(entry.swatch);
            format!(
                "<span foreground=\"#{:02x}{:02x}{:02x}\">■</span> {}",
                (color.0 * 255.0).round() as u8,
                (color.1 * 255.0).round() as u8,
                (color.2 * 255.0).round() as u8,
                glib::markup_escape_text(&entry.label)
            )
        })
        .collect();
    if model.legend_overflow() > 0 {
        entries.push(format!("+{} more in Details", model.legend_overflow()));
    }
    entries.join("   ")
}

fn table_text(model: &ChartCardModel) -> String {
    let mut lines = vec![model.table_header().join("  |  ")];
    lines.extend(model.table_rows().iter().map(|row| row.join("  |  ")));
    if model.table_rows().is_empty() {
        lines.push("No rows".to_string());
    }
    lines.join("\n")
}

fn refresh(state: &State) {
    let (model, parts) = (&state.model, &state.parts);
    parts.title.set_label(model.title());
    match model.headline() {
        Some(headline) => {
            set_text(&parts.headline_label, Some(&headline.label));
            set_text(&parts.headline_value, Some(&headline.value));
            set_text(&parts.headline_note, headline.note.as_deref());
        }
        None => {
            set_text(&parts.headline_label, None);
            set_text(&parts.headline_value, None);
            set_text(&parts.headline_note, None);
        }
    }
    set_text(&parts.subtitle, Some(model.subtitle()));
    set_text(&parts.badges, Some(&model.badges().join(" · ")));
    set_text(&parts.banner, model.banner());
    set_text(&parts.message, model.message());
    let has_plot = !matches!(model.plot(), PlotModel::Empty);
    parts.plot.set_visible(has_plot);
    parts
        .plot
        .set_content_height(model.preferred_plot_height().ceil() as i32);
    parts.plot.queue_draw();
    parts
        .legend
        .set_markup(&legend_markup(model, &state.palette));
    parts.legend.set_visible(!model.legend().is_empty());
    set_text(&parts.footnote, model.footnote());
    let readout = state
        .selected
        .and_then(|index| model.selection_readout(index))
        .unwrap_or("");
    set_text(&parts.readout, Some(readout));
    parts.table.set_text(&table_text(model));
    parts.info.set_label(&model.info_lines().join("\n"));
    // The crate's GTK feature level has no accessible-property API; GTK
    // exposes the tooltip as the accessible name/description of a widget.
    parts.plot.set_tooltip_text(Some(model.accessible_name()));
}

fn select(shared: &Shared, index: Option<usize>) {
    let mut state = shared.borrow_mut();
    if state.selected == index {
        return;
    }
    state.selected = index;
    let readout = index
        .and_then(|index| state.model.selection_readout(index))
        .unwrap_or("")
        .to_string();
    set_text(&state.parts.readout, Some(&readout));
    state.parts.plot.queue_draw();
}

fn step(shared: &Shared, key: gtk::gdk::Key) -> bool {
    let count = shared.borrow().model.selection_count();
    if count == 0 {
        return false;
    }
    let current = shared.borrow().selected;
    let target = match key {
        gtk::gdk::Key::Right | gtk::gdk::Key::Down => {
            Some(current.map_or(0, |i| (i + 1).min(count - 1)))
        }
        gtk::gdk::Key::Left | gtk::gdk::Key::Up => {
            Some(current.map_or(count - 1, |i| i.saturating_sub(1)))
        }
        gtk::gdk::Key::Home => Some(0),
        gtk::gdk::Key::End => Some(count - 1),
        gtk::gdk::Key::Escape => None,
        _ => return false,
    };
    select(shared, target);
    true
}

fn popover_button(name: &str, label: &str, content: &impl IsA<gtk::Widget>) -> gtk::MenuButton {
    let button = gtk::MenuButton::new();
    button.set_label(label);
    button.add_css_class("chart-action");
    button.set_widget_name(name);
    let popover = gtk::Popover::new();
    popover.set_child(Some(content));
    button.set_popover(Some(&popover));
    button
}

/// Widget name `chart-card-{id}`; keyboard-focusable plot, Details/Info controls, accessible name set.
pub fn card_widget(
    model: &ChartCardModel,
    palette: &ChartPalette,
    layout: CardLayout,
) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.set_widget_name(&widget_name(model.card_id()));
    card.add_css_class("usage-card");
    card.add_css_class("provider-card");
    card.add_css_class("chart-card");

    let (title, headline_label, headline_value, headline_note) = (
        label("chart-title"),
        label("chart-headline-label"),
        label("chart-headline-value"),
        label("chart-headline-note"),
    );
    headline_note.add_css_class("chart-note");
    title.set_hexpand(true);
    let headline = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for part in [&headline_label, &headline_value, &headline_note] {
        headline.append(part);
    }
    let header = gtk::Box::new(
        match layout {
            CardLayout::Wide => gtk::Orientation::Horizontal,
            CardLayout::Narrow => gtk::Orientation::Vertical,
        },
        8,
    );
    header.append(&title);
    header.append(&headline);
    let (subtitle, badges, banner, message) = (
        label("chart-subtitle"),
        label("chart-note"),
        label("chart-banner"),
        label("chart-message"),
    );
    let plot = gtk::DrawingArea::new();
    plot.set_widget_name("chart-plot");
    plot.add_css_class("chart-plot");
    plot.set_hexpand(true);
    plot.set_focusable(true);
    plot.set_can_focus(true);
    let (readout, legend, footnote) = (
        label("chart-readout"),
        label("chart-legend"),
        label("chart-note"),
    );
    legend.set_use_markup(true);

    let table_view = gtk::TextView::new();
    table_view.set_editable(false);
    table_view.set_cursor_visible(false);
    table_view.set_monospace(true);
    table_view.add_css_class("chart-table");
    let table = table_view.buffer();
    let scroll = gtk::ScrolledWindow::builder()
        .min_content_width(320)
        .min_content_height(200)
        .max_content_height(320)
        .child(&table_view)
        .build();
    let info = label("chart-info");
    info.set_selectable(true);
    info.set_max_width_chars(48);
    let info_scroll = gtk::ScrolledWindow::builder()
        .min_content_width(320)
        .max_content_height(320)
        .propagate_natural_height(true)
        .child(&info)
        .build();
    let details = popover_button("chart-details", "Details", &scroll);
    let info_button = popover_button("chart-info-button", "Info", &info_scroll);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    actions.append(&details);
    actions.append(&info_button);

    for part in [
        header.upcast_ref::<gtk::Widget>(),
        subtitle.upcast_ref(),
        badges.upcast_ref(),
        banner.upcast_ref(),
        message.upcast_ref(),
        plot.upcast_ref(),
        readout.upcast_ref(),
        legend.upcast_ref(),
        footnote.upcast_ref(),
        actions.upcast_ref(),
    ] {
        card.append(part);
    }

    let shared: Shared = Rc::new(RefCell::new(State {
        model: model.clone(),
        palette: palette.clone(),
        selected: None,
        parts: Parts {
            title,
            headline_label,
            headline_value,
            headline_note,
            subtitle,
            badges,
            banner,
            message,
            plot: plot.clone(),
            readout,
            legend,
            footnote,
            table,
            info,
            details: details.clone(),
        },
    }));

    let draw_state = shared.clone();
    plot.set_draw_func(move |_, cr, width, height| {
        if let Ok(state) = draw_state.try_borrow() {
            draw_plot(
                cr,
                &state.model,
                &state.palette,
                f64::from(width),
                f64::from(height),
                state.selected,
            );
        }
    });
    let keys = gtk::EventControllerKey::new();
    let key_state = shared.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if matches!(
            key,
            gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter | gtk::gdk::Key::space
        ) {
            let details = key_state.borrow().parts.details.clone();
            details.popup();
            return glib::Propagation::Stop;
        }
        if step(&key_state, key) {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    plot.add_controller(keys);
    let motion = gtk::EventControllerMotion::new();
    let motion_state = shared.clone();
    let motion_area = plot.clone();
    motion.connect_motion(move |_, x, y| {
        let index = hit_test(
            &motion_state.borrow().model,
            f64::from(motion_area.width()),
            f64::from(motion_area.height()),
            x,
            y,
        );
        select(&motion_state, index);
    });
    plot.add_controller(motion);
    let click = gtk::GestureClick::new();
    let click_plot = plot.clone();
    click.connect_pressed(move |_, _, _, _| {
        click_plot.grab_focus();
    });
    plot.add_controller(click);

    refresh(&shared.borrow());
    card.set_tooltip_text(Some(model.accessible_name()));
    // SAFETY: the value is only ever read back as `Shared` under this key.
    unsafe { card.set_data(STATE_KEY, shared) };
    card.upcast()
}

/// Replace contents in place, keeping open Details and focus when `model` has the same card id; false => caller rebuilds.
pub fn update_card_widget(
    widget: &gtk::Widget,
    model: &ChartCardModel,
    palette: &ChartPalette,
) -> bool {
    if widget.widget_name() != widget_name(model.card_id()) {
        return false;
    }
    // SAFETY: see `card_widget`; the key only holds `Shared`.
    let Some(shared) =
        (unsafe { widget.data::<Shared>(STATE_KEY) }).map(|ptr| unsafe { ptr.as_ref().clone() })
    else {
        return false;
    };
    {
        let mut state = shared.borrow_mut();
        state.model = model.clone();
        state.palette = palette.clone();
        state.selected = state
            .selected
            .filter(|index| *index < model.selection_count());
        refresh(&state);
    }
    widget.set_tooltip_text(Some(model.accessible_name()));
    true
}
