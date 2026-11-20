use adw::prelude::*;
use glib::translate::{ToGlibPtr, ToGlibPtrMut};
use gtk::glib;
use sidescope::{
    AppConfig, Card, CardMetric, CardState, Graph, Pace, chart_mapping,
    chart_runtime::ChartRuntime,
    cli::{Command, LaunchMode, find_in_path, help_text, parse_args, run_doctor},
    config_path, dashboard,
    density::{self, Density},
    load_config_at, pace_context, pack_columns, plot, process, sensors, theme, visible_cards,
    weather,
};
mod chart_host;

use chart_host::ChartEntry;
use std::sync::mpsc;
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};

const DEMO: &str = include_str!("../tests/fixtures/usage.json");
/// Source revision this binary was built from; see `build.rs`. The suffix
/// `-dirty` means the working tree had uncommitted changes to tracked files.
const BUILD_REVISION: &str = env!("SIDESCOPE_REVISION");
fn build_label() -> String {
    format!("v{} · {}", env!("CARGO_PKG_VERSION"), BUILD_REVISION)
}
fn build_tooltip() -> String {
    let mut tooltip = format!(
        "Built from git revision {BUILD_REVISION} (version {})",
        env!("CARGO_PKG_VERSION")
    );
    if BUILD_REVISION.ends_with("-dirty") {
        tooltip.push_str("\n-dirty: tracked files had uncommitted changes when this was built.");
    }
    tooltip
}
thread_local! { static ACTIVE_THEME: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
thread_local! {
    static ACTIVE_DENSITY: std::cell::Cell<Density> = const { std::cell::Cell::new(Density::ScreenFit) };
    static FIT_CORE_COLUMNS: std::cell::Cell<usize> = const { std::cell::Cell::new(4) };
    static RENDER_CACHE: RefCell<std::collections::BTreeMap<usize, RenderSnapshot>> = const { RefCell::new(std::collections::BTreeMap::new()) };
}
fn screen_fit() -> bool {
    ACTIVE_DENSITY.get() == Density::ScreenFit
}
fn active_theme() -> &'static theme::Theme {
    &theme::THEMES[ACTIVE_THEME.get()]
}
fn source(cr: &gtk::cairo::Context, color: theme::Color) {
    cr.set_source_rgba(color.0, color.1, color.2, color.3);
}
/// Family name from `SIDESCOPE_METRIC_FONT`, a test aid for bisecting
/// host-specific text-rendering glitches by swapping the numeric font without a
/// rebuild. The value is interpolated into CSS, so only a plain family name is
/// accepted; anything else (empty, quotes, braces, semicolons) is rejected.
fn metric_font_name(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.')))
    .then_some(value)
}
/// Selects GTK's font rendering mode and, in the manual mode, asks for hinted
/// font metrics. Returns false when this GTK has no rendering mode to select.
///
/// A host whose fontconfig asks for `hintslight` leaves a line's ascent
/// fractional, so the glyph is rasterized across two pixel rows: a reading's top
/// bar came out as two half rows, which reads as a line above the digit. Manual
/// rendering plus `gtk-hint-font-metrics` rounds that ascent and puts the glyph
/// back on whole rows, and GTK honours the metrics setting only in the manual
/// mode, which is why setting it alone changes nothing.
///
/// Hinted metrics also round every line's ascent and descent, which would make
/// cards up to 5px taller and overflow the screen-fit dashboard. The stylesheet
/// pins `line-height`, so a line box no longer follows the font's metrics and
/// the layout this change lands on is the layout it replaces.
///
/// `gtk-font-rendering` arrived in GTK 4.16, after the 4.14 releases target, so
/// the typed binding is feature-gated and the enum goes through the property
/// system. Older GTK has no rendering mode to select.
fn set_font_rendering(settings: &gtk::Settings, manual: bool) -> bool {
    let Some(spec) = settings.find_property("gtk-font-rendering") else {
        return false;
    };
    let mut value = glib::Value::from_type(spec.value_type());
    unsafe {
        // GTK_FONT_RENDERING_AUTOMATIC / GTK_FONT_RENDERING_MANUAL
        glib::gobject_ffi::g_value_set_enum(value.to_glib_none_mut().0, i32::from(manual));
    }
    settings.set_property_from_value("gtk-font-rendering", &value);
    if manual {
        settings.set_property("gtk-hint-font-metrics", true);
    }
    true
}

/// The effective `gtk-font-rendering` for the diagnostic print: 0 automatic, 1
/// manual, `None` when this GTK has no such property.
fn font_rendering(settings: &gtk::Settings) -> Option<i32> {
    settings
        .find_property("gtk-font-rendering")
        .map(|_| unsafe {
            let value: glib::Value = settings.property_value("gtk-font-rendering");
            glib::gobject_ffi::g_value_get_enum(value.to_glib_none().0)
        })
}

/// Test aid for host-specific text-rendering glitches. `SIDESCOPE_FONT_DEBUG`
/// prints GTK's effective font settings before and after the overrides below;
/// the other `SIDESCOPE_*` variables force individual settings so a host can
/// be bisected without touching its desktop settings.
fn configure_font_rendering() {
    let Some(settings) = gtk::Settings::default() else {
        return;
    };
    if std::env::var_os("SIDESCOPE_FONT_DEBUG").is_some() {
        eprintln!(
            "FONT_SETTINGS font={:?} antialias={} hinting={} hintstyle={:?} rgba={:?} dpi={} hint_font_metrics={:?} font_rendering={:?}",
            settings.gtk_font_name(),
            settings.gtk_xft_antialias(),
            settings.gtk_xft_hinting(),
            settings.gtk_xft_hintstyle(),
            settings.gtk_xft_rgba(),
            settings.gtk_xft_dpi(),
            settings.property::<bool>("gtk-hint-font-metrics"),
            font_rendering(&settings),
        );
    }
    set_font_rendering(&settings, true);
    let flag = |name: &str| match std::env::var(name).as_deref() {
        Ok("on" | "1" | "true" | "yes") => Some(true),
        Ok("off" | "0" | "false" | "no") => Some(false),
        _ => None,
    };
    let antialias = flag("SIDESCOPE_ANTIALIAS");
    let hinting = flag("SIDESCOPE_HINTING");
    // Subpixel glyph positioning is what lets a glyph's rows straddle a pixel
    // row; SIDESCOPE_HINT_FONT_METRICS=off turns it on to reproduce that.
    let hint_metrics = flag("SIDESCOPE_HINT_FONT_METRICS");
    let hintstyle = std::env::var("SIDESCOPE_HINT_STYLE").ok().filter(|v| {
        matches!(
            v.as_str(),
            "hintnone" | "hintslight" | "hintmedium" | "hintfull"
        )
    });
    let rgba = std::env::var("SIDESCOPE_RGBA")
        .ok()
        .filter(|v| matches!(v.as_str(), "none" | "rgb" | "bgr" | "vrgb" | "vbgr"));
    let rendering = std::env::var("SIDESCOPE_FONT_RENDERING")
        .ok()
        .filter(|v| matches!(v.as_str(), "automatic" | "manual"));
    let overrides = antialias.is_some()
        || hinting.is_some()
        || hintstyle.is_some()
        || rgba.is_some()
        || hint_metrics.is_some()
        || rendering.is_some();
    if overrides {
        if let Some(mode) = rendering {
            set_font_rendering(&settings, mode == "manual");
        }
        if let Some(value) = hint_metrics {
            // Not in the generated bindings below GTK 4.6's feature gate, but the
            // property exists: set it by name.
            settings.set_property("gtk-hint-font-metrics", value);
        }
        if let Some(value) = antialias {
            settings.set_gtk_xft_antialias(i32::from(value));
        }
        if let Some(value) = hinting {
            settings.set_gtk_xft_hinting(i32::from(value));
        }
        if let Some(value) = hintstyle {
            settings.set_gtk_xft_hintstyle(Some(value.as_str()));
        }
        if let Some(value) = rgba {
            settings.set_gtk_xft_rgba(Some(value.as_str()));
        }
    }
    if std::env::var_os("SIDESCOPE_FONT_DEBUG").is_some() {
        eprintln!(
            "FONT_SETTINGS_AFTER antialias={} hinting={} hintstyle={:?} rgba={:?} hint_font_metrics={:?} font_rendering={:?}",
            settings.gtk_xft_antialias(),
            settings.gtk_xft_hinting(),
            settings.gtk_xft_hintstyle(),
            settings.gtk_xft_rgba(),
            settings.property::<bool>("gtk-hint-font-metrics"),
            font_rendering(&settings),
        );
    }
}
fn redraw_tree(root: &gtk::Widget) {
    root.queue_draw();
    let mut child = root.first_child();
    while let Some(widget) = child {
        redraw_tree(&widget);
        child = widget.next_sibling();
    }
}
// Provisional, conservative whole-report polling interval. ai-usagebar owns per-source caching.
const REFRESH_INTERVAL_SECS: u32 = 300;
const PACE_TICK_WIDTH: f64 = 4.0;
const PACE_TICK_OUTLINE_WIDTH: f64 = 7.0;
const _: () = assert!(PACE_TICK_OUTLINE_WIDTH > PACE_TICK_WIDTH);

fn pace_tick_x(width: i32, elapsed_pct: u8) -> f64 {
    let inset = PACE_TICK_OUTLINE_WIDTH / 2.0;
    let usable = (f64::from(width) - PACE_TICK_OUTLINE_WIDTH).max(0.0);
    inset + usable * f64::from(elapsed_pct.min(100)) / 100.0
}

enum ReportCheck<'a> {
    Checking,
    Checked(&'a str),
    Failed,
}

fn report_check_text(state: ReportCheck<'_>) -> String {
    match state {
        ReportCheck::Checking => "CHECKING REPORT…".into(),
        ReportCheck::Checked(time) => format!("REPORT CHECKED {time}"),
        ReportCheck::Failed => "CHECK FAILED · SEE ERROR ABOVE".into(),
    }
}

fn local_check_time() -> String {
    glib::DateTime::now_local()
        .ok()
        .and_then(|now| now.format("%H:%M:%S").ok())
        .map(|time| time.to_string())
        .unwrap_or_else(|| "JUST NOW".into())
}

#[derive(Debug, PartialEq, Eq)]
enum Shortcut {
    PreviousTheme,
    NextTheme,
    Refresh,
    ToggleHelp,
    ToggleDensity,
    CloseHelp,
    ScrollDown,
    ScrollUp,
    ScrollPageDown,
    ScrollPageUp,
}

fn allow_refresh(demo: bool, ready: bool) -> bool {
    !demo && ready
}

fn shortcut_action(key: gtk::gdk::Key, demo: bool, help_visible: bool) -> Option<Shortcut> {
    if key == gtk::gdk::Key::Escape {
        return help_visible.then_some(Shortcut::CloseHelp);
    }
    if !help_visible {
        match key {
            gtk::gdk::Key::Down => return Some(Shortcut::ScrollDown),
            gtk::gdk::Key::Up => return Some(Shortcut::ScrollUp),
            gtk::gdk::Key::Page_Down => return Some(Shortcut::ScrollPageDown),
            gtk::gdk::Key::Page_Up => return Some(Shortcut::ScrollPageUp),
            _ => {}
        }
    }
    match key.to_unicode() {
        Some('d' | 'D') if !help_visible => Some(Shortcut::ToggleDensity),
        Some('?') => Some(Shortcut::ToggleHelp),
        Some('[') if !help_visible => Some(Shortcut::PreviousTheme),
        Some(']') if !help_visible => Some(Shortcut::NextTheme),
        Some('r' | 'R') if allow_refresh(demo, !help_visible) => Some(Shortcut::Refresh),
        Some('j' | 'J') if !help_visible => Some(Shortcut::ScrollDown),
        Some('k' | 'K') if !help_visible => Some(Shortcut::ScrollUp),
        _ => None,
    }
}

fn main() {
    let command = parse_args(std::env::args()).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    let mode = match command {
        Command::Help => {
            println!("{}", help_text());
            return;
        }
        Command::Version => {
            println!(
                "sidescope {} ({})",
                env!("CARGO_PKG_VERSION"),
                BUILD_REVISION
            );
            return;
        }
        Command::Doctor => {
            let (report, ok) = run_doctor();
            println!("{report}");
            if !ok {
                std::process::exit(1);
            }
            return;
        }
        Command::Run { mode } => mode,
    };
    let demo = !matches!(mode, LaunchMode::Live);
    let browser = matches!(mode, LaunchMode::BrowserMock);
    let initial = match mode {
        LaunchMode::Report(path) => std::fs::read_to_string(&path).unwrap_or_else(|error| {
            eprintln!("cannot read report {path}: {error}");
            std::process::exit(2);
        }),
        _ => DEMO.into(),
    };
    let config = if browser {
        AppConfig::default()
    } else {
        config_path()
            .and_then(|path| load_config_at(&path))
            .unwrap_or_else(|error| {
                eprintln!("Cannot load Sidescope configuration: {error}");
                std::process::exit(2);
            })
    };
    if demo
        && !browser
        && let Err(error) = visible_cards(&initial, &config)
    {
        eprintln!("Cannot read usage report: {error}");
        std::process::exit(2);
    }
    let id = if browser {
        "org.mschulkind.BrowserMock"
    } else {
        "org.mschulkind.Sidescope"
    };
    let app = adw::Application::builder().application_id(id).build();
    app.connect_activate(move |app| {
        if browser {
            build_browser(app);
        } else {
            build(app, &initial, demo, &config);
        }
    });
    app.run_with_args(&["sidescope"]);
}

fn pace_label(pace: Pace) -> String {
    match pace.delta {
        delta if delta > 0 => format!(
            "+{delta} {} OVER PACE",
            if delta == 1 { "pt" } else { "pts" }
        ),
        delta if delta < 0 => format!(
            "{} {} UNDER PACE",
            -delta,
            if delta == -1 { "pt" } else { "pts" }
        ),
        _ => "ON PACE".into(),
    }
}

/// CSS class naming a pace direction, so a bar's fill and its verdict text
/// agree on both color and wording.
fn pace_class(pace: Pace) -> &'static str {
    if pace.delta > 0 {
        "pace-over"
    } else if pace.delta < 0 {
        "pace-under"
    } else {
        "pace-even"
    }
}

fn scroll_target(value: f64, delta: f64, lower: f64, upper: f64, page_size: f64) -> f64 {
    (value + delta).clamp(lower, (upper - page_size).max(lower))
}

fn scroll_by(scroll: &gtk::ScrolledWindow, direction: f64, page: bool) {
    let adjustment = scroll.vadjustment();
    let step = if page {
        (adjustment.page_size() * 0.85).max(120.0)
    } else {
        120.0
    };
    adjustment.set_value(scroll_target(
        adjustment.value(),
        direction * step,
        adjustment.lower(),
        adjustment.upper(),
        adjustment.page_size(),
    ));
}

fn build_browser(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Browser placeholder")
        .default_width(1100)
        .default_height(900)
        .build();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 30);
    page.set_margin_top(80);
    page.set_margin_start(80);
    page.append(&gtk::Label::new(Some(
        "Browser placeholder — tiling geometry only",
    )));
    window.set_content(Some(&page));
    window.set_size_request(320, 200);
    window.present();
}

fn build(app: &adw::Application, initial: &str, demo: bool, config: &AppConfig) {
    configure_font_rendering();
    ACTIVE_DENSITY.set(config.density);
    let density_css = gtk::CssProvider::new();
    density_css.load_from_data(include_str!("density.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("display"),
        &density_css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
    let css = gtk::CssProvider::new();
    ACTIVE_THEME.set(config.theme.as_deref().and_then(theme::index).unwrap_or(0));
    css.load_from_data(&theme::css(active_theme()));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("display"),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let metric_font = std::env::var("SIDESCOPE_METRIC_FONT").ok();
    if let Some(font) = metric_font.as_deref().and_then(metric_font_name) {
        let metric_css = gtk::CssProvider::new();
        metric_css.load_from_data(&format!(
            ".metric-value, .metric-label, .core-label, .day-label, .day-temperature, .pace-label, .pace-time {{ font-family: \"{font}\"; }}"
        ));
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("display"),
            &metric_css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 2,
        );
    }

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Sidescope")
        .default_width(640)
        .default_height(900)
        .build();
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.add_css_class("stripe");
    if screen_fit() {
        outer.add_css_class("screen-fit");
    }
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.add_css_class("stripe-header");
    let header_text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    header_text.set_hexpand(true);
    header_text.append(&text("SIDESCOPE", "kicker"));
    header_text.append(&text("Dashboard", "headline"));
    header.append(&header_text);
    let refresh = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh AI usage")
        .build();
    refresh.add_css_class("refresh-button");
    refresh.set_halign(gtk::Align::End);
    refresh.set_valign(gtk::Align::Start);
    refresh.set_hexpand(false);
    header.append(&refresh);
    outer.append(&header);

    let wide_scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let narrow_scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let wide_stack = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let narrow_stack = gtk::Box::new(gtk::Orientation::Vertical, 8);
    wide_stack.add_css_class("card-stack");
    narrow_stack.add_css_class("card-stack");
    wide_scroll.set_child(Some(&wide_stack));
    narrow_scroll.set_child(Some(&narrow_stack));
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_vexpand(true);
    narrow_scroll.set_visible(false);
    content.append(&wide_scroll);
    content.append(&narrow_scroll);
    let responsive = adw::BreakpointBin::new();
    responsive.set_vexpand(true);
    // BreakpointBin cannot derive a minimum from its responsive children.
    responsive.set_size_request(320, 200);
    let breakpoint = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 500px").expect("valid narrow breakpoint"),
    );
    breakpoint.add_setter(&wide_scroll, "visible", Some(&false.to_value()));
    breakpoint.add_setter(&narrow_scroll, "visible", Some(&true.to_value()));
    responsive.add_breakpoint(breakpoint);
    responsive.set_child(Some(&content));
    outer.append(&responsive);
    let footer = gtk::Box::new(gtk::Orientation::Vertical, 3);
    footer.add_css_class("footer");
    let mode = if demo {
        "●  SAMPLE DATA"
    } else {
        "●  LIVE MODE · AUTO 5 MIN"
    };
    // The running revision is always on screen: the footer sits outside the scroll area.
    let mode_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let mode_label = text(mode, "footer-label");
    mode_label.set_hexpand(true);
    mode_row.append(&mode_label);
    let build = text(&build_label(), "footer-right");
    build.set_widget_name("build-revision");
    build.set_tooltip_text(Some(&build_tooltip()));
    mode_row.append(&build);
    footer.append(&mode_row);
    let check_status = text("WAITING FOR REPORT", "footer-right");
    if !demo {
        check_status.set_tooltip_text(Some(
            "Report check time; ai-usagebar may return cached provider data.",
        ));
        footer.append(&check_status);
    }
    let selector = theme_selector(&css, &outer);
    footer.append(&selector);
    let density_toggle = gtk::Button::with_label(if screen_fit() {
        "Density: Screen-fit"
    } else {
        "Density: Comfortable"
    });
    density_toggle.set_widget_name("density-toggle");
    density_toggle.set_tooltip_text(Some("Toggle screen-fit density (D). Session only; no config is written. Scroll remains available if content cannot fit."));
    density_toggle.add_css_class("density-toggle");
    footer.append(&density_toggle);
    outer.append(&footer);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&outer));
    let (help, close_help) = shortcuts_overlay(demo);
    overlay.add_overlay(&help);
    window.set_content(Some(&overlay));
    {
        let outer = outer.clone();
        window.connect_notify_local(Some("is-active"), move |window, _| {
            if window.is_active() {
                outer.add_css_class("window-focused");
            } else {
                outer.remove_css_class("window-focused");
            }
        });
    }

    {
        let help = help.clone();
        close_help.connect_clicked(move |_| help.set_visible(false));
    }
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let help = help.clone();
        let refresh = refresh.clone();
        let wide_scroll = wide_scroll.clone();
        let narrow_scroll = narrow_scroll.clone();
        let close_help = close_help.clone();
        let css = css.clone();
        let outer = outer.clone();
        let selector = selector.clone();
        let density_toggle = density_toggle.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if find_widget(&selector.clone().upcast(), "theme-name")
                .and_then(|w| w.downcast::<gtk::MenuButton>().ok())
                .and_then(|picker| picker.popover())
                .is_some_and(|popover| popover.is_visible())
            {
                return glib::Propagation::Proceed; // Native picker owns navigation/scrolling while open.
            }
            if modifiers.intersects(
                gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::SUPER_MASK,
            ) {
                return glib::Propagation::Proceed;
            }
            let active_scroll = if narrow_scroll.is_visible() {
                &narrow_scroll
            } else {
                &wide_scroll
            };
            match shortcut_action(key, demo, help.is_visible()) {
                Some(Shortcut::PreviousTheme) => select_theme(
                    theme::cycle(ACTIVE_THEME.get(), -1),
                    &css,
                    &outer,
                    &selector,
                ),
                Some(Shortcut::NextTheme) => {
                    select_theme(theme::cycle(ACTIVE_THEME.get(), 1), &css, &outer, &selector)
                }
                Some(Shortcut::Refresh) if allow_refresh(demo, refresh.is_sensitive()) => {
                    refresh.emit_clicked();
                }
                Some(Shortcut::ToggleDensity) => density_toggle.emit_clicked(),
                Some(Shortcut::ToggleHelp) => {
                    let open = !help.is_visible();
                    help.set_visible(open);
                    if open {
                        close_help.grab_focus();
                    }
                }
                Some(Shortcut::CloseHelp) => help.set_visible(false),
                Some(Shortcut::ScrollDown) => scroll_by(active_scroll, 1.0, false),
                Some(Shortcut::ScrollUp) => scroll_by(active_scroll, -1.0, false),
                Some(Shortcut::ScrollPageDown) => scroll_by(active_scroll, 1.0, true),
                Some(Shortcut::ScrollPageUp) => scroll_by(active_scroll, -1.0, true),
                Some(Shortcut::Refresh) | None => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
    }
    window.add_controller(keys);
    refresh.set_visible(!demo);

    let chart_runtime: Rc<RefCell<Option<ChartRuntime>>> = Rc::new(RefCell::new(
        chart_host::starts_chart_runtime(demo, &config.chart_sources)
            .then(|| ChartRuntime::start(&config.chart_sources, chart_mapping::normalize_output)),
    ));
    let chart_scenario =
        demo && std::env::var("SIDESCOPE_VISUAL_SCENARIO").is_ok_and(|s| s.starts_with("charts-"));
    if chart_runtime.borrow().is_some() || chart_scenario {
        chart_host::install_css(active_theme());
    }
    let data = Rc::new(RefCell::new(DashboardData {
        charts: chart_runtime
            .borrow()
            .as_ref()
            .map(|runtime| {
                chart_host::entries_from_snapshots(&config.chart_cards, &runtime.snapshots())
            })
            .unwrap_or_default(),
        providers: if demo {
            visible_cards(initial, config).expect("validated initial report")
        } else if config.enabled.as_deref() != Some(&[]) && find_in_path("ai-usagebar").is_none() {
            vec![dashboard::unavailable(
                "ai-usage",
                "AI usage",
                "Optional: install ai-usagebar for AI quota cards, or set enabled = [] in config.toml to hide.",
            )]
        } else {
            vec![]
        },
        local: if demo {
            dashboard::demo_cards()
        } else {
            vec![dashboard::unavailable(
                "cpu",
                "CPU",
                "Sampling Linux local telemetry…",
            )]
        },
        remote: if demo {
            weather::demo_cards()
        } else if config.weather.is_some() {
            vec![
                dashboard::unavailable(
                    "weather",
                    "Weather",
                    "Fetching explicit-location forecast…",
                ),
                dashboard::unavailable(
                    "air",
                    "Air quality",
                    "Fetching modeled Open-Meteo/CAMS air quality…",
                ),
            ]
        } else {
            weather::unconfigured_cards()
        },
        error: None,
    }));
    if demo && let Ok(scenario) = std::env::var("SIDESCOPE_VISUAL_SCENARIO") {
        visual_scenario(&mut data.borrow_mut(), &scenario);
    }
    render_dashboard(&wide_stack, &narrow_stack, &data.borrow(), demo);
    {
        let (wide, narrow, data, outer) = (
            wide_stack.clone(),
            narrow_stack.clone(),
            data.clone(),
            outer.clone(),
        );
        density_toggle.connect_clicked(move |button| {
            ACTIVE_DENSITY.set(ACTIVE_DENSITY.get().toggle());
            if screen_fit() {
                outer.add_css_class("screen-fit");
            } else {
                outer.remove_css_class("screen-fit");
            }
            button.set_label(if screen_fit() {
                "Density: Screen-fit"
            } else {
                "Density: Comfortable"
            });
            render_dashboard(&wide, &narrow, &data.borrow(), demo);
            eprintln!("DENSITY_SELECTED {:?}", ACTIVE_DENSITY.get());
        });
    }
    // Check geometry only after allocations settle. No tick-driven layout rebuild:
    // unchanged window dimensions never invalidate card hosts or remote Details.
    {
        let window = window.downgrade();
        let (wide, narrow, data) = (wide_stack.clone(), narrow_stack.clone(), data.clone());
        let mut size = (0, 0);
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let Some(window) = window.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let next = (window.width(), window.height());
            if next != size {
                size = next;
                render_dashboard(&wide, &narrow, &data.borrow(), demo);
            }
            glib::ControlFlow::Continue
        });
    }
    // Theme changes intentionally rebuild geometry; telemetry ticks still preserve
    // unchanged remote subtrees and their open/focused Details controls.
    {
        let (wide, narrow, data) = (wide_stack.clone(), narrow_stack.clone(), data.clone());
        let picker = find_widget(&selector.clone().upcast(), "theme-name")
            .unwrap()
            .downcast::<gtk::MenuButton>()
            .unwrap();
        picker.connect_notify_local(Some("label"), move |_, _| {
            render_dashboard(&wide, &narrow, &data.borrow(), demo);
        });
    }

    if chart_runtime.borrow().is_some() {
        let (runtime, data) = (chart_runtime.clone(), data.clone());
        let (wide, narrow) = (wide_stack.clone(), narrow_stack.clone());
        let mut ticks = 0u32;
        // A terminated stripe must stop every chart command it started: the
        // handler only sets a flag, and the poll below quits through GTK so the
        // shutdown hook below runs.
        install_termination_flag();
        let quit_app = app.clone();
        glib::timeout_add_local(Duration::from_millis(200), move || {
            if TERMINATION_REQUESTED.load(std::sync::atomic::Ordering::Relaxed) {
                quit_app.quit();
                return glib::ControlFlow::Break;
            }
            ticks = ticks.wrapping_add(1);
            let Some(changed) = runtime.borrow().as_ref().map(ChartRuntime::poll) else {
                return glib::ControlFlow::Break;
            };
            // Every 5 s the cards are re-evaluated so freshness labels advance.
            if std::env::var_os("SIDESCOPE_VISUAL_GEOMETRY").is_some() {
                for snapshot in &changed {
                    eprintln!(
                        "CHART_SOURCE id={} dataset={} error={}",
                        snapshot.source_id,
                        snapshot.dataset.is_some(),
                        snapshot
                            .last_error
                            .as_ref()
                            .map_or("none", |error| error.kind.label())
                    );
                }
            }
            if !changed.is_empty() || ticks.is_multiple_of(25) {
                chart_host::apply_snapshots(&mut data.borrow_mut().charts, &changed);
                let data = data.borrow();
                sync_chart_section(&wide, true, &data);
                sync_chart_section(&narrow, false, &data);
            }
            glib::ControlFlow::Continue
        });
        let runtime = chart_runtime.clone();
        app.connect_shutdown(move |_| {
            if let Some(runtime) = runtime.borrow_mut().take() {
                runtime.shutdown();
            }
        });
    }
    if !demo {
        start_local_sampling(&window, &wide_stack, &narrow_stack, &data, &config.sensors);
        if let Some(location) = &config.weather {
            let busy = Rc::new(std::cell::Cell::new(false));
            refresh_weather(&window, &wide_stack, &narrow_stack, &data, location, &busy);
            let weak_window = window.downgrade();
            let wide = wide_stack.clone();
            let narrow = narrow_stack.clone();
            let data = data.clone();
            let location = location.clone();
            glib::timeout_add_seconds_local(weather::REFRESH_SECS, move || {
                let Some(window) = weak_window.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                refresh_weather(&window, &wide, &narrow, &data, &location, &busy);
                glib::ControlFlow::Continue
            });
        }
    }
    if !demo {
        let wide_stack = wide_stack.clone();
        let narrow_stack = narrow_stack.clone();
        let config = config.clone();
        let data = data.clone();
        let check_status = check_status.clone();
        let timer_click = Rc::new(chart_host::AutomaticRefresh::default());
        let chart_refresh = chart_runtime.clone();
        let manual = timer_click.clone();
        let sources: Vec<String> = config.chart_sources.iter().map(|s| s.id.clone()).collect();
        refresh.connect_clicked(move |button| {
            if manual.requests_chart_refresh()
                && let Some(runtime) = chart_refresh.borrow().as_ref()
            {
                for id in &sources {
                    runtime.refresh_now(id);
                }
            }
            if config.enabled.as_deref() == Some(&[]) {
                data.borrow_mut().providers.clear();
                data.borrow_mut().error = None;
                render_dashboard(&wide_stack, &narrow_stack, &data.borrow(), false);
                return;
            }
            if find_in_path("ai-usagebar").is_none() {
                data.borrow_mut().providers = vec![dashboard::unavailable(
                    "ai-usage",
                    "AI usage",
                    "Optional: install ai-usagebar for AI quota cards, or run `sidescope --demo` for sample data.",
                )];
                data.borrow_mut().error = None;
                render_dashboard(&wide_stack, &narrow_stack, &data.borrow(), false);
                return;
            }
            button.set_sensitive(false);
            check_status.set_text(&report_check_text(ReportCheck::Checking));
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let result = process::output(
                    std::process::Command::new("ai-usagebar").args(["usage", "--json"]),
                    Duration::from_secs(45),
                );
                let _ = tx.send(result);
            });
            let wide_stack = wide_stack.clone();
            let narrow_stack = narrow_stack.clone();
            let button = button.clone();
            let config = config.clone();
            let check_status = check_status.clone();
            let data = data.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
                Ok(Ok(json)) => {
                    if visible_cards(&json, &config).is_ok() {
                        data.borrow_mut().providers =
                            visible_cards(&json, &config).expect("validated report");
                        data.borrow_mut().error = None;
                        render_dashboard(&wide_stack, &narrow_stack, &data.borrow(), false);
                        check_status.set_text(&report_check_text(ReportCheck::Checked(
                            &local_check_time(),
                        )));
                    } else {
                        report_failure(&wide_stack, &narrow_stack, &data, "Invalid usage report");
                        check_status.set_text(&report_check_text(ReportCheck::Failed));
                    }
                    button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Ok(Err(error)) => {
                    report_failure(&wide_stack, &narrow_stack, &data, &error);
                    check_status.set_text(&report_check_text(ReportCheck::Failed));
                    button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    report_failure(
                        &wide_stack,
                        &narrow_stack,
                        &data,
                        "Usage report process disconnected",
                    );
                    check_status.set_text(&report_check_text(ReportCheck::Failed));
                    button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            });
        });
        timer_click.run(|| refresh.emit_clicked());
        let weak_refresh = refresh.downgrade();
        glib::timeout_add_seconds_local(REFRESH_INTERVAL_SECS, move || {
            match weak_refresh.upgrade() {
                Some(button) => {
                    if allow_refresh(false, button.is_sensitive()) {
                        timer_click.run(|| button.emit_clicked());
                    }
                    glib::ControlFlow::Continue
                }
                None => glib::ControlFlow::Break,
            }
        });
    } else {
        refresh.set_sensitive(false);
    }
    window.present();
    // Return freed startup pages to the OS once after startup settles.
    glib::timeout_add_seconds_local_once(2, || unsafe {
        libc::malloc_trim(0);
    });
    if demo && std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref() == Ok("long-condition") {
        let stack = wide_stack.clone();
        glib::timeout_add_local(Duration::from_millis(200), move || {
            let label = find_widget(&stack.clone().upcast(), "weather-condition")
                .unwrap()
                .downcast::<gtk::Label>()
                .unwrap();
            // The scenario's condition is the longest the fixture can produce.
            // Whether this card width has to ellipsize it depends on the active
            // density, so assert the honest invariants: one line, never wrapped,
            // and the full text available in the tooltip.
            if label.is_mapped() && label.width() > 0 && label.text() == "Thunderstorm with hail" {
                assert!(!label.wraps() && label.layout().line_count() == 1);
                assert_eq!(
                    label.tooltip_text().as_deref(),
                    Some("Thunderstorm with hail")
                );
                eprintln!(
                    "CONDITION_TIGHT ellipsized={} tooltip_complete=true width={}",
                    label.layout().is_ellipsized(),
                    label.width()
                );
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
    if demo && std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref() == Ok("one") {
        let stack = wide_stack.clone();
        glib::timeout_add_local_once(Duration::from_millis(500), move || {
            let menu = find_widget(&stack.clone().upcast(), "weather-details")
                .unwrap()
                .downcast::<gtk::MenuButton>()
                .unwrap();
            menu.popover()
                .unwrap()
                .connect_notify_local(Some("visible"), move |popover, _| {
                    eprintln!("HOURLY_DETAILS visible={}", popover.is_visible());
                });
            assert!(menu.grab_focus());
        });
    }
    if demo
        && matches!(
            std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref(),
            Ok("theme-live" | "theme-picker")
        )
    {
        test_theme_controls(&css, &outer, &selector);
    }
    if demo
        && matches!(
            std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref(),
            Ok("theme-live" | "fit-theme")
        )
    {
        test_theme_across_updates(
            &window,
            &css,
            &outer,
            &selector,
            &wide_stack,
            &narrow_stack,
            &data,
        );
    }
    if demo
        && matches!(
            std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref(),
            Ok("live-details" | "strip-live-details" | "fit-details" | "fit-info")
        )
    {
        test_details_across_local_updates(&window, &wide_stack, &narrow_stack, &data);
    }
    if demo && std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref() == Ok("charts-details") {
        test_chart_details_across_updates(&wide_stack, &narrow_stack, &data);
    }
    if demo
        && std::env::var("SIDESCOPE_VISUAL_SCENARIO")
            .is_ok_and(|s| s.starts_with("fit-") || s == "screen-fit")
    {
        test_fit_geometry(&window, &wide_scroll, &narrow_scroll, &data);
        if matches!(
            std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref(),
            Ok("fit-report-details" | "fit-report-info")
        ) {
            test_report_error_transitions(&window, &wide_stack, &narrow_stack, &data);
        }
    }
    if demo && std::env::var_os("SIDESCOPE_VISUAL_GEOMETRY").is_some() {
        let stack = wide_stack.clone();
        let visual_window = window.clone();
        let mut attempts = 0;
        glib::timeout_add_local(Duration::from_millis(200), move || {
            attempts += 1;
            if visual_window.width() != 636 {
                assert!(attempts < 30, "wide compositor allocation never settled");
                return glib::ControlFlow::Continue;
            }
            // A same-frame theme switch can map new labels before allocating them.
            // Wait for real geometry, but fail boundedly rather than skip checks.
            if find_widget(&stack.clone().upcast(), "weather-axis-0")
                .is_some_and(|label| label.width() == 0)
            {
                assert!(attempts < 15, "weather axis never received an allocation");
                return glib::ControlFlow::Continue;
            }
            if let Some(widget) = find_widget(&stack.clone().upcast(), "weather-condition") {
                let label = widget.downcast::<gtk::Label>().unwrap();
                assert!(!label.wraps() && label.layout().line_count() == 1);
                assert_eq!(label.tooltip_text().as_deref(), Some(label.text().as_str()));
                if label.text() == "Clear" {
                    assert!(
                        !label.layout().is_ellipsized(),
                        "Clear must fit next to 61°F in the narrow masonry card"
                    );
                }
                eprintln!(
                    "CONDITION_GEOMETRY text={} allocation={}x{} ellipsized={}",
                    label.text(),
                    label.width(),
                    label.height(),
                    label.layout().is_ellipsized()
                );
            }
            for name in ["weather-axis-0", "weather-axis-1", "weather-axis-2"] {
                if let Some(widget) = find_widget(&stack.clone().upcast(), name) {
                    let label = widget.downcast::<gtk::Label>().unwrap();
                    eprintln!(
                        "AXIS_GEOMETRY {name} allocated={}x{} text={} layout={:?} offsets={:?} wrap={} mapped={}",
                        label.width(),
                        label.height(),
                        label.text(),
                        label.layout().pixel_size(),
                        label.layout_offsets(),
                        label.wraps(),
                        label.is_mapped()
                    );
                    if let Some(rect) = label.compute_bounds(&visual_window) {
                        eprintln!(
                            "AXIS_RECT {name} {} {} {} {}",
                            rect.x() as i32,
                            rect.y() as i32,
                            rect.width() as i32,
                            rect.height() as i32
                        );
                    }
                    assert!(
                        !label.wraps(),
                        "hourly axis endpoints must be nonwrapping, explicitly allocated labels"
                    );
                    if label.is_mapped() {
                        let (text_width, text_height) = label.layout().pixel_size();
                        assert!(
                            label.width() >= text_width && label.height() >= text_height,
                            "hourly axis text was clipped by allocation"
                        );
                    }
                }
            }
            glib::ControlFlow::Break
        });
    }
}

fn mapped_labels(root: &gtk::Widget) -> Vec<String> {
    let mut labels = vec![];
    if let Ok(label) = root.clone().downcast::<gtk::Label>()
        && label.is_mapped()
    {
        let (w, h) = label.layout().pixel_size();
        assert!(
            w <= label.width() + 1 && h <= label.height() + 1,
            "clipped text: {} layout={w}x{h} allocation={}x{}",
            label.text(),
            label.width(),
            label.height()
        );
        labels.push(label.text().to_string());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        labels.extend(mapped_labels(&widget));
        child = widget.next_sibling();
    }
    labels
}

fn test_fit_geometry(
    window: &adw::ApplicationWindow,
    wide: &gtk::ScrolledWindow,
    narrow: &gtk::ScrolledWindow,
    data: &Rc<RefCell<DashboardData>>,
) {
    let (window, wide, narrow, data) = (window.clone(), wide.clone(), narrow.clone(), data.clone());
    let mut last = None;
    let mut settled = 0;
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let scroll = if narrow.is_visible() { &narrow } else { &wide };
        let adj = scroll.vadjustment();
        let key = (
            window.width(),
            window.height(),
            ACTIVE_THEME.get(),
            ACTIVE_DENSITY.get(),
            adj.upper() as i32,
            adj.page_size() as i32,
        );
        if last == Some(key) {
            settled += 1;
        } else {
            last = Some(key);
            settled = 0;
        }
        if settled == 2 {
            let root = scroll.child().unwrap();
            let cards: Vec<_> = data
                .borrow()
                .providers
                .iter()
                .chain(&data.borrow().local)
                .chain(&data.borrow().remote)
                .cloned()
                .collect();
            assert_eq!(
                cards.len(),
                13,
                "full dashboard fixture must retain thirteen cards including sensors"
            );
            // Readings are digits a glance has to resolve: a glyph whose metrics
            // are not hinted rasterizes across two pixel rows, so a 5's top bar
            // comes out as two half rows. GTK honours gtk-hint-font-metrics only
            // while font rendering is manual, so the pair is asserted together.
            // GTK before 4.16 has no rendering mode to choose.
            let settings = gtk::Settings::default().expect("settings");
            if let Some(rendering) = font_rendering(&settings) {
                assert_eq!(
                    rendering, 1,
                    "readings need manual font rendering (GTK_FONT_RENDERING_MANUAL)"
                );
                assert!(
                    settings.property::<bool>("gtk-hint-font-metrics"),
                    "readings need hinted font metrics to stay on whole pixel rows"
                );
            }
            for card in &cards {
                let view = find_widget(&root, &format!("card-{}", card.id)).unwrap();
                let bounds = view.compute_bounds(scroll).unwrap();
                assert!(
                    bounds.x() >= 0.0 && bounds.x() + bounds.width() <= scroll.width() as f32 + 1.0,
                    "horizontal clipping: {} {:?}",
                    card.id,
                    bounds
                );
                let labels = mapped_labels(&view);
                for metric in &card.metrics {
                    let normalize =
                        |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                    assert!(
                        labels
                            .iter()
                            .any(|s| normalize(s) == normalize(&metric.value)),
                        "reading missing from {}: {}",
                        card.id,
                        metric.value
                    );
                    assert!(
                        labels.iter().any(|s| s == &metric.label),
                        "budget/metric label missing from {}: {}",
                        card.id,
                        metric.label
                    );
                }
                if screen_fit() {
                    let menu = find_widget(&view, &format!("info-{}", card.id))
                        .unwrap()
                        .downcast::<gtk::MenuButton>()
                        .unwrap();
                    let info = find_widget(&menu.popover().unwrap().upcast(), "info-context")
                        .unwrap()
                        .downcast::<gtk::Label>()
                        .unwrap();
                    for graph in &card.graphs {
                        assert!(
                            info.text().contains(&graph.label),
                            "keyboard Info omitted graph context: {}",
                            graph.label
                        );
                    }
                }
                if screen_fit() && card.id == "ram" {
                    for graph in &card.graphs {
                        let percent = graph.label.rsplit(" · ").next().unwrap();
                        assert!(
                            labels.iter().any(|s| s == percent),
                            "memory utilization missing from visible readings: {percent}"
                        );
                    }
                }
                eprintln!(
                    "FIT_ALLOCATION {} width={} height={}",
                    card.id,
                    bounds.width(),
                    bounds.height()
                );
            }
            assert!(find_widget(&root, "weather-details").is_some());
            let scenario = std::env::var("SIDESCOPE_VISUAL_SCENARIO").unwrap();
            if scenario == "fit-info" && window.width() < 500 {
                let menu = find_widget(&root, "info-openrouter")
                    .unwrap()
                    .downcast::<gtk::MenuButton>()
                    .unwrap();
                menu.popup();
                glib::timeout_add_local_once(Duration::from_millis(150), move || {
                    let label = find_widget(&menu.popover().unwrap().upcast(), "info-context")
                        .unwrap()
                        .downcast::<gtk::Label>()
                        .unwrap();
                    assert!(
                        label.is_mapped() && (200..=320).contains(&label.width()),
                        "narrow source Info must remain readable within its bounded width: mapped={} width={}",
                        label.is_mapped(),
                        label.width()
                    );
                    eprintln!("NARROW_INFO_GEOMETRY width={}", label.width());
                });
            }
            let must_fit = screen_fit()
                && data.borrow().error.is_none()
                && window.width() > 500
                && window.height() >= 1400
                && !matches!(scenario.as_str(), "fit-high" | "fit-errors" | "fit-extreme");
            if must_fit {
                assert!(
                    adj.upper() <= adj.page_size() + 1.0,
                    "screen-fit fixture overflow: upper={} page={}",
                    adj.upper(),
                    adj.page_size()
                );
            }
            if screen_fit()
                && (window.width() < 500 || scenario == "fit-high" || scenario == "fit-errors")
            {
                assert!(
                    adj.upper() > adj.page_size(),
                    "genuine overflow must retain scrolling"
                );
            }
            // horizontal_clip was a hardcoded `false`; the density suite greps it, so
            // keep printing the real measurement for both.
            let cards_visible = cards.len();
            let horizontally_clipped = cards.iter().any(|card| {
                find_widget(&root, &format!("card-{}", card.id)).is_some_and(|view| {
                    view.compute_bounds(scroll).is_some_and(|bounds| {
                        bounds.x() < 0.0
                            || bounds.x() + bounds.width() > scroll.width() as f32 + 1.0
                    })
                })
            });
            eprintln!(
                "DENSITY_GEOMETRY mode={:?} theme={} width={} height={} upper={} page={} no_scroll={} cards={} horizontal_clip={}",
                ACTIVE_DENSITY.get(),
                active_theme().id,
                window.width(),
                window.height(),
                adj.upper(),
                adj.page_size(),
                adj.upper() <= adj.page_size() + 1.0,
                cards_visible,
                horizontally_clipped
            );
        }
        glib::ControlFlow::Continue
    });
}

fn select_theme(index: usize, css: &gtk::CssProvider, outer: &gtk::Box, selector: &gtk::Box) {
    ACTIVE_THEME.set(index);
    css.load_from_data(&theme::css(active_theme()));
    chart_host::reload_css(active_theme());
    if let Some(label) = find_widget(&selector.clone().upcast(), "theme-name")
        .and_then(|w| w.downcast::<gtk::MenuButton>().ok())
    {
        label.set_label(&format!(
            "{}  ·  {}/{}",
            active_theme().name,
            index + 1,
            theme::THEMES.len()
        ));
        label.set_tooltip_text(Some(active_theme().direction));
    }
    redraw_tree(&outer.clone().upcast());
    if std::env::var_os("SIDESCOPE_VISUAL_GEOMETRY").is_some() {
        eprintln!("THEME_SELECTED {}", active_theme().id);
    }
}

fn theme_selector(css: &gtk::CssProvider, outer: &gtk::Box) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.add_css_class("theme-selector");
    let previous = gtk::Button::with_label("‹");
    previous.set_widget_name("theme-previous");
    previous.set_tooltip_text(Some("Previous theme ([); wheel here to browse"));
    let next = gtk::Button::with_label("›");
    next.set_widget_name("theme-next");
    next.set_tooltip_text(Some("Next theme (])"));
    let picker = gtk::MenuButton::new();
    picker.set_tooltip_text(Some(
        "Choose a theme; session only, no configuration is written",
    ));
    picker.set_hexpand(true);
    picker.set_widget_name("theme-name");
    picker.set_label(&format!(
        "{}  ·  {}/{}",
        active_theme().name,
        ACTIVE_THEME.get() + 1,
        theme::THEMES.len()
    ));
    let popover = gtk::Popover::new();
    let scroll = gtk::ScrolledWindow::builder()
        .min_content_width(260)
        .max_content_height(320)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    scroll.set_widget_name("theme-options");
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    for (index, t) in theme::THEMES.iter().enumerate() {
        let button = gtk::Button::with_label(&format!("{:02}  {}", index + 1, t.name));
        button.set_widget_name(&format!("theme-option-{}", t.id));
        button.set_tooltip_text(Some(t.direction));
        let (css, outer, row, popover) = (
            css.clone(),
            outer.downgrade(),
            row.downgrade(),
            popover.downgrade(),
        );
        button.connect_clicked(move |_| {
            if let (Some(outer), Some(row)) = (outer.upgrade(), row.upgrade()) {
                select_theme(index, &css, &outer, &row);
            }
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
        });
        list.append(&button);
    }
    scroll.set_child(Some(&list));
    popover.set_child(Some(&scroll));
    picker.set_popover(Some(&popover));
    for (button, delta) in [(&previous, -1), (&next, 1)] {
        let (css, outer, row) = (css.clone(), outer.downgrade(), row.downgrade());
        button.connect_clicked(move |_| {
            if let (Some(outer), Some(row)) = (outer.upgrade(), row.upgrade()) {
                select_theme(theme::cycle(ACTIVE_THEME.get(), delta), &css, &outer, &row);
            }
        });
    }
    row.append(&previous);
    row.append(&picker);
    row.append(&next);
    let wheel = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
    );
    let (css, weak_outer, weak_row) = (css.clone(), outer.downgrade(), row.downgrade());
    wheel.connect_scroll(move |_, _, dy| {
        if dy == 0.0 {
            return glib::Propagation::Proceed;
        }
        if let (Some(outer), Some(row)) = (weak_outer.upgrade(), weak_row.upgrade()) {
            select_theme(
                theme::cycle(ACTIVE_THEME.get(), if dy > 0.0 { 1 } else { -1 }),
                &css,
                &outer,
                &row,
            );
        }
        glib::Propagation::Stop
    });
    row.add_controller(wheel);
    row
}

fn shortcuts_overlay(demo: bool) -> (gtk::Box, gtk::Button) {
    let scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
    scrim.add_css_class("shortcuts-scrim");
    scrim.set_halign(gtk::Align::Fill);
    scrim.set_valign(gtk::Align::Fill);
    scrim.set_visible(false);

    let above = gtk::Box::new(gtk::Orientation::Vertical, 0);
    above.set_vexpand(true);
    scrim.append(&above);
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 12);
    panel.add_css_class("shortcuts-panel");
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Center);
    panel.append(&text("KEYBOARD SHORTCUTS", "section-title"));
    panel.append(&text(
        "Work without reaching for the mouse.",
        "shortcut-description",
    ));
    if !demo {
        panel.append(&shortcut_row("R", "Refresh usage now"));
    }
    panel.append(&shortcut_row("J / ↓", "Scroll down"));
    panel.append(&shortcut_row("K / ↑", "Scroll up"));
    panel.append(&shortcut_row("PgDn / PgUp", "Scroll a page"));
    panel.append(&shortcut_row("[ / ]", "Previous / next theme"));
    panel.append(&shortcut_row("D", "Toggle screen-fit density"));
    panel.append(&shortcut_row("?", "Show or hide this guide"));
    panel.append(&shortcut_row("Esc", "Close this guide"));
    let close = gtk::Button::with_label("Close guide");
    close.add_css_class("shortcut-close");
    close.set_halign(gtk::Align::End);
    panel.append(&close);
    scrim.append(&panel);
    let below = gtk::Box::new(gtk::Orientation::Vertical, 0);
    below.set_vexpand(true);
    scrim.append(&below);
    (scrim, close)
}

fn shortcut_row(key: &str, description: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    row.add_css_class("shortcut-row");
    let badge = text(key, "shortcut-key");
    badge.set_width_chars(4);
    row.append(&badge);
    row.append(&text(description, "shortcut-description"));
    row
}

#[derive(Default)]
struct DashboardData {
    charts: Vec<ChartEntry>,
    providers: Vec<Card>,
    local: Vec<Card>,
    remote: Vec<Card>,
    error: Option<String>,
}

/// Synthetic visual fixtures only; never invoked for live sources.
fn visual_scenario(data: &mut DashboardData, scenario: &str) {
    match scenario {
        scenario if scenario.starts_with("charts-") => {
            data.charts = chart_host::demo_entries(scenario)
                .unwrap_or_else(|| panic!("unknown synthetic visual scenario: {scenario}"));
        }
        "strip-missing" => visual_scenario(data, "missing"),
        "strip-fahrenheit" => visual_scenario(data, "fahrenheit"),
        "one" | "two" | "full" | "rolling" | "missing" => {
            let len = match scenario {
                "one" => 1,
                "two" => 2,
                "missing" => 60,
                _ => 120,
            };
            let start = if scenario == "rolling" { 1 } else { 0 };
            for card in &mut data.local {
                for graph in &mut card.graphs {
                    let current = graph.samples.last().copied().flatten();
                    graph.samples = (0..len)
                        .map(|i| {
                            if scenario == "missing" && (20..30).contains(&i) {
                                None
                            } else {
                                Some(((i * 7 + 20) % 100) as u64 * graph.ceiling / 100)
                            }
                        })
                        .collect();
                    if let Some(last) = graph.samples.last_mut() {
                        *last = current;
                    }
                    if start == 1 {
                        graph.samples.remove(0);
                        graph.samples.push(current);
                    }
                }
            }
            if scenario == "missing"
                && let Some(f) = data.remote.iter_mut().find_map(|c| c.weather.as_mut())
            {
                for h in &mut f.hours[3..6] {
                    h.temperature = None;
                    h.code = None;
                }
                f.days[1].low = None;
                f.days[2].high = None;
            }
        }
        "stale" => {
            for card in data.local.iter_mut().chain(&mut data.remote) {
                card.state = CardState::Stale;
                card.note
                    .push_str("\nREFRESH FAILED · last success retained");
            }
        }
        "unconfigured" => data.remote = weather::unconfigured_cards(),
        "clear" | "long-condition" | "fahrenheit" => {
            for f in data.remote.iter_mut().filter_map(|c| c.weather.as_mut()) {
                let convert = |n: i32| n * 9 / 5 + 320;
                f.units = weather::Units::Fahrenheit;
                f.temperature = convert(f.temperature);
                if matches!(scenario, "clear" | "long-condition") {
                    f.code = if scenario == "clear" { 0 } else { 99 };
                    f.temperature = 610;
                }
                for h in &mut f.hours {
                    h.temperature = h.temperature.map(convert);
                }
                for d in &mut f.days {
                    d.low = d.low.map(convert);
                    d.high = d.high.map(convert);
                }
            }
        }
        "busy-cpu" | "screen-fit" | "fit-details" | "fit-controls" | "fit-errors"
        | "fit-missing" => {
            let cpu = data.local.iter_mut().find(|c| c.id == "cpu").unwrap();
            *cpu = dashboard::busy_cpu_demo();
            if scenario == "fit-errors" {
                for card in data.providers.iter_mut().chain(&mut data.remote) {
                    card.state = CardState::Stale;
                    card.note.push_str("\nREFRESH FAILED · last success retained. Synthetic source unreachable; retry on next scheduled check. The synthetic adapter could not complete its scheduled check. Values above are from the last successful source result, not a fresh request. See Info for complete source context.");
                }
            }
            if scenario == "fit-missing" {
                visual_scenario(data, "missing");
            }
        }
        "fit-sensors-zero"
        | "fit-sensors-missing"
        | "fit-sensors-error"
        | "fit-sensors-recovery" => {
            let cpu = data.local.iter_mut().find(|c| c.id == "cpu").unwrap();
            *cpu = dashboard::busy_cpu_demo();
            let card = data.local.iter_mut().find(|c| c.id == "sensors").unwrap();
            if scenario == "fit-sensors-missing" {
                card.state = CardState::Stale;
                card.metrics[0].detail = "STALE · Source missing\nSynthetic coretemp · temp1\nLast success 3 min ago · missing history slots are gaps".into();
                card.note.push_str("\nSome sensors unavailable");
            } else if scenario == "fit-sensors-error" {
                card.state = CardState::Unavailable("Selected sensors unavailable".into());
                for (metric, reason) in card.metrics.iter_mut().zip([
                    "Permission denied",
                    "Malformed reading",
                    "Sensor fault",
                    "Source missing",
                ]) {
                    metric.value = "—".into();
                    metric.detail =
                        format!("UNAVAILABLE · {reason}\nSynthetic hwmon · no successful sample");
                }
                card.note.push_str("\nNo successful samples");
            } else if scenario == "fit-sensors-recovery" {
                // A source retired after absent polls, then came back at a new
                // address: fresh one-slot history, prior reading gone, and the
                // session retirement count still visible on the card.
                card.metrics[0].value = "55.0 °C".into();
                card.metrics[0].detail = "Ready\nSynthetic coretemp · /sys/devices/platform/coretemp.0 · temp1\nLast success just now\nSession history: 1 / 120 raw 2s slots, 0 gaps; unit millidegrees Celsius".into();
                card.note.push_str("\nRetired 1 absent sensor · session");
            }
        }
        "live-details" | "strip-live-details" | "theme-live" | "theme-picker" => {}
        "fit-info" | "fit-theme" | "fit-report-details" | "fit-report-info" => {
            visual_scenario(data, "busy-cpu")
        }
        "fit-extreme" => {
            visual_scenario(data, "busy-cpu");
            data.providers[0].metrics[0].value = "$123456789012345.67".into();
            data.local[2].metrics[0].value = "18446744073709551615 bytes/s".into();
            data.local[2].graphs[0].ceiling = u64::MAX;
            data.local[2].graphs[0].samples = vec![Some(u64::MAX); 120];
            let forecast = data.remote[0].weather.as_mut().unwrap();
            forecast.temperature = -400;
            forecast.days[0].low = Some(-500);
            forecast.days[0].high = Some(600);
            forecast.days[1].code = None;
        }
        "fit-stale-quota" => {
            // A stale reading from a source that states its reset window: the
            // parser keeps the window's elapsed fraction and drops only the
            // verdict, which needs a current reading. A committed fixture cannot
            // carry a `reset_at` that stays in the future, so inject the parser's
            // own output. No note is added, so the pack height is unchanged and
            // the fit assertions still measure the same layout; stale notes are
            // fit-errors' subject.
            let card = data
                .providers
                .iter_mut()
                .find(|card| card.id == "anthropic")
                .unwrap();
            card.state = CardState::Stale;
            card.metrics[1].elapsed_pct = Some(58);
            card.metrics[1].pace = None;
        }
        "high-cores" | "fit-high" => {
            let cpu = data.local.iter_mut().find(|c| c.id == "cpu").unwrap();
            let template = cpu.graphs[1].clone();
            cpu.graphs.truncate(1);
            for core in 0..64 {
                let mut graph = template.clone();
                graph.label = format!("Core {core} · 61%");
                if let Some(last) = graph.samples.last_mut() {
                    *last = Some(61);
                }
                cpu.graphs.push(graph);
            }
            cpu.subtitle = "64 logical cores · synthetic utilization".into();
        }
        _ => panic!("unknown synthetic visual scenario: {scenario}"),
    }
}

/// Focuses the first chart's plot so a real Enter key opens its Details, then
/// replaces the snapshot underneath the open popover and checks that the same
/// widgets and the open popover survived the update.
fn test_chart_details_across_updates(
    wide: &gtk::Box,
    narrow: &gtk::Box,
    data: &Rc<RefCell<DashboardData>>,
) {
    let (wide, narrow, data) = (wide.clone(), narrow.clone(), data.clone());
    let mut armed = None;
    let mut done = false;
    glib::timeout_add_local(Duration::from_millis(300), move || {
        let root: gtk::Widget = wide.clone().upcast();
        let (Some(plot), Some(details)) = (
            find_widget(&root, "chart-plot"),
            find_widget(&root, "chart-details").and_then(|w| w.downcast::<gtk::MenuButton>().ok()),
        ) else {
            return glib::ControlFlow::Continue;
        };
        let Some(popover) = details.popover() else {
            return glib::ControlFlow::Continue;
        };
        if armed.is_none() {
            if plot.is_mapped() && plot.grab_focus() {
                popover.connect_notify_local(Some("visible"), move |popover, _| {
                    eprintln!("CHART_DETAILS visible={}", popover.is_visible());
                });
                eprintln!("CHART_FOCUS plot_focused=true");
                let card = find_widget(&root, "chart-card-demo-spend");
                eprintln!(
                    "CHART_A11Y plot_label={} card_label={}",
                    gtk::test_accessible_has_property(&plot, gtk::AccessibleProperty::Label),
                    card.is_some_and(|card| gtk::test_accessible_has_property(
                        &card,
                        gtk::AccessibleProperty::Label
                    ))
                );
                armed = Some(plot.clone());
            }
            return glib::ControlFlow::Continue;
        }
        if done || !popover.is_visible() {
            return glib::ControlFlow::Continue;
        }
        done = true;
        {
            let mut data = data.borrow_mut();
            for entry in &mut data.charts {
                if let Some(snapshot) = entry.snapshot.as_mut() {
                    snapshot.info.revision += 1;
                }
            }
        }
        let data = data.borrow();
        sync_chart_section(&wide, true, &data);
        sync_chart_section(&narrow, false, &data);
        let same_plot =
            find_widget(&root, "chart-plot").is_some_and(|now| Some(&now) == armed.as_ref());
        eprintln!(
            "CHART_DETAILS_UPDATE visible={} same_widgets={}",
            popover.is_visible(),
            same_plot
        );
        glib::ControlFlow::Break
    });
}

fn find_widget(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_widget(&widget, name) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

/// Real GTK regression in the private compositor; updates use the production render path.
fn test_details_across_local_updates(
    window: &adw::ApplicationWindow,
    wide: &gtk::Box,
    narrow: &gtk::Box,
    data: &Rc<RefCell<DashboardData>>,
) {
    let (window, wide, narrow, data) = (window.clone(), wide.clone(), narrow.clone(), data.clone());
    // Resolve the control after initial responsive allocation, not from an
    // unrooted launch-size tree that the compositor is about to replace.
    glib::timeout_add_local_once(Duration::from_millis(900), move || {
        let menu_name = if std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref() == Ok("fit-info") {
            "info-cpu"
        } else {
            "weather-details"
        };
        let menu = find_widget(&wide.clone().upcast(), menu_name)
            .unwrap()
            .downcast::<gtk::MenuButton>()
            .unwrap();
        let popover = menu.popover().unwrap();
        let updates = Rc::new(std::cell::Cell::new(0u64));
        let count = updates.clone();
        popover.connect_notify_local(Some("visible"), move |popover, _| {
            eprintln!(
                "LIVE_DETAILS visible={} updates={}",
                popover.is_visible(),
                count.get()
            );
        });
        menu.popup();
        assert!(menu.grab_focus());
        glib::timeout_add_local(Duration::from_secs(dashboard::SAMPLE_SECS), move || {
            assert!(
                popover.is_visible(),
                "hourly Details closed before local update"
            );
            let focused = gtk::prelude::GtkWindowExt::focus(&window);
            assert!(focused.is_some(), "Details must have keyboard focus");
            if menu_name == "info-cpu" {
                let context = find_widget(&popover.clone().upcast(), "info-context")
                    .unwrap()
                    .downcast::<gtk::Label>()
                    .unwrap();
                assert!(
                    context.width() >= 200 && context.layout().line_count() <= 24,
                    "source Info must be a readable text panel, not a character-width column"
                );
                eprintln!(
                    "INFO_GEOMETRY width={} lines={}",
                    context.width(),
                    context.layout().line_count()
                );
            }
            let tick = updates.get() + 1;
            let mut cards = dashboard::demo_cards();
            if screen_fit() {
                cards[0] = dashboard::busy_cpu_demo();
            }
            cards[0].graphs[0].label = format!("Total · {tick}%");
            cards[0].graphs[0].samples.push(Some(tick));
            apply_local_sample(&wide, &narrow, &data, cards, true);
            // The whole-report update path also leaves unchanged remote Details intact.
            if screen_fit() {
                // Providers are optional in this scenario; update one only if present.
                if let Some(metric) = data
                    .borrow_mut()
                    .providers
                    .first_mut()
                    .and_then(|card| card.metrics.first_mut())
                {
                    metric.value = format!("${tick}.00");
                }
                render_dashboard(&wide, &narrow, &data.borrow(), true);
                assert_eq!(ACTIVE_DENSITY.get(), Density::ScreenFit);
            }
            updates.set(tick);
            assert_eq!(
                find_widget(&wide.clone().upcast(), menu_name),
                Some(menu.clone().upcast()),
                "local sample replaced unchanged weather Details widget"
            );
            assert!(
                popover.is_visible() && menu.is_mapped(),
                "local sample closed hourly Details"
            );
            assert_eq!(
                gtk::prelude::GtkWindowExt::focus(&window),
                focused,
                "local sample lost Details keyboard focus"
            );
            eprintln!("LIVE_DETAILS_UPDATE tick={tick} visible=true focus_preserved=true");
            if tick == 3 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    });
}

/// Real GTK regression: changing report banners must not invalidate same-ID hosts.
fn test_report_error_transitions(
    window: &adw::ApplicationWindow,
    wide: &gtk::Box,
    narrow: &gtk::Box,
    data: &Rc<RefCell<DashboardData>>,
) {
    let (window, wide, narrow, data) = (window.clone(), wide.clone(), narrow.clone(), data.clone());
    glib::timeout_add_local_once(Duration::from_millis(900), move || {
        let name = if std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref() == Ok("fit-report-info")
        {
            "info-openrouter"
        } else {
            "weather-details"
        };
        let menu = find_widget(&wide.clone().upcast(), name)
            .unwrap()
            .downcast::<gtk::MenuButton>()
            .unwrap();
        let popover = menu.popover().unwrap();
        let providers = data.borrow().providers.clone();
        let hosts: Vec<_> = providers
            .iter()
            .chain(&data.borrow().local)
            .chain(&data.borrow().remote)
            .map(|card| find_widget(&wide.clone().upcast(), &format!("card-{}", card.id)).unwrap())
            .collect();
        let parents: Vec<_> = hosts.iter().map(|host| host.parent()).collect();
        menu.popup();
        assert!(menu.grab_focus());
        let mut tick = 0;
        glib::timeout_add_local(Duration::from_secs(2), move || {
            tick += 1;
            let focused = gtk::prelude::GtkWindowExt::focus(&window);
            assert!(focused.is_some() && popover.is_visible());
            match tick {
                1 => report_failure(&wide, &narrow, &data, "Synthetic report unavailable"),
                2 => report_failure(
                    &wide,
                    &narrow,
                    &data,
                    &"Changed synthetic failure; retained readings are not fresh. ".repeat(24),
                ),
                _ => {
                    data.borrow_mut().providers = providers.clone();
                    data.borrow_mut().error = None;
                    render_dashboard(&wide, &narrow, &data.borrow(), true);
                }
            }
            assert_eq!(
                find_widget(&wide.clone().upcast(), name),
                Some(menu.clone().upcast()),
                "report error transition replaced open control"
            );
            for (host, parent) in hosts.iter().zip(&parents) {
                assert_eq!(
                    find_widget(&wide.clone().upcast(), host.widget_name().as_str()),
                    Some(host.clone()),
                    "report transition replaced card host"
                );
                assert_eq!(
                    host.parent(),
                    *parent,
                    "report transition lost column packing"
                );
            }
            assert!(popover.is_visible() && menu.is_mapped());
            assert_eq!(
                gtk::prelude::GtkWindowExt::focus(&window),
                focused,
                "report transition lost popover focus"
            );
            let stage = tick;
            let (wide, popover, window) = (wide.clone(), popover.clone(), window.clone());
            glib::timeout_add_local_once(Duration::from_millis(150), move || {
                let banner = find_widget(&wide.clone().upcast(), "report-error-banner")
                    .unwrap()
                    .downcast::<gtk::Label>()
                    .unwrap();
                assert_eq!(
                    banner.is_visible(),
                    stage < 3,
                    "report banner failed to update/recover"
                );
                if stage < 3 {
                    assert!(banner.text().contains(if stage == 1 {
                        "Synthetic report unavailable"
                    } else {
                        "Changed synthetic failure"
                    }));
                    mapped_labels(&banner.clone().upcast());
                }
                let scroll = wide
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .downcast::<gtk::ScrolledWindow>()
                    .unwrap();
                let adjustment = scroll.vadjustment();
                if stage == 2 {
                    assert!(
                        adjustment.upper() > adjustment.page_size(),
                        "long error banner must overflow honestly"
                    );
                }
                assert!(popover.is_visible());
                assert_eq!(
                    gtk::prelude::GtkWindowExt::focus(&window),
                    focused,
                    "allocated banner transition lost focus"
                );
                eprintln!(
                    "REPORT_BANNER tick={stage} visible={} upper={} page={} focus_preserved=true",
                    banner.is_visible(),
                    adjustment.upper(),
                    adjustment.page_size()
                );
            });
            eprintln!(
                "REPORT_TRANSITION tick={tick} control={name} visible=true focus_preserved=true hosts_preserved=true packing_preserved=true"
            );
            if tick == 3 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    });
}

fn test_theme_controls(css: &gtk::CssProvider, outer: &gtk::Box, selector: &gtk::Box) {
    let (css, outer, selector) = (css.clone(), outer.clone(), selector.clone());
    glib::timeout_add_local_once(Duration::from_millis(400), move || {
        let button = |name| {
            find_widget(&selector.clone().upcast(), name)
                .unwrap()
                .downcast::<gtk::Button>()
                .unwrap()
        };
        button("theme-previous").emit_clicked();
        assert_eq!(ACTIVE_THEME.get(), 20);
        button("theme-next").emit_clicked();
        assert_eq!(ACTIVE_THEME.get(), 0);
        let controllers = selector.observe_controllers();
        let wheel = controllers
            .item(0)
            .unwrap()
            .downcast::<gtk::EventControllerScroll>()
            .unwrap();
        assert!(wheel.emit_by_name::<bool>("scroll", &[&0.0f64, &1.0f64]));
        assert_eq!(ACTIVE_THEME.get(), 1);
        select_theme(0, &css, &outer, &selector);
        let picker = find_widget(&selector.clone().upcast(), "theme-name")
            .unwrap()
            .downcast::<gtk::MenuButton>()
            .unwrap();
        picker.popup();
        glib::timeout_add_local_once(Duration::from_millis(150), move || {
            let options = find_widget(&picker.popover().unwrap().upcast(), "theme-options")
                .unwrap()
                .downcast::<gtk::ScrolledWindow>()
                .unwrap();
            assert!(
                options.vadjustment().upper() > options.vadjustment().page_size(),
                "all 21 choices must fit a scrollable picker"
            );
            eprintln!(
                "THEME_CONTROLS previous_next=true selector_wheel=true scrollable_picker=true"
            );
            if std::env::var("SIDESCOPE_VISUAL_SCENARIO").as_deref() == Ok("theme-live") {
                find_widget(&picker.popover().unwrap().upcast(), "theme-option-arcade")
                    .unwrap()
                    .downcast::<gtk::Button>()
                    .unwrap()
                    .emit_clicked();
                assert_eq!(
                    ACTIVE_THEME.get(),
                    20,
                    "picker must select its final choice"
                );
                assert!(!picker.popover().unwrap().is_visible());
                select_theme(0, &css, &outer, &selector);
                eprintln!("THEME_PICKER selected=arcade popdown=true");
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn test_theme_across_updates(
    window: &adw::ApplicationWindow,
    css: &gtk::CssProvider,
    outer: &gtk::Box,
    selector: &gtk::Box,
    wide: &gtk::Box,
    narrow: &gtk::Box,
    data: &Rc<RefCell<DashboardData>>,
) {
    let (window, css, outer, selector, wide, narrow, data) = (
        window.clone(),
        css.clone(),
        outer.clone(),
        selector.clone(),
        wide.clone(),
        narrow.clone(),
        data.clone(),
    );
    let mut tick = 0;
    glib::timeout_add_local(Duration::from_secs(2), move || {
        tick += 1;
        select_theme(tick, &css, &outer, &selector);
        let selected = ACTIVE_THEME.get();
        let mut cards = dashboard::demo_cards();
        if screen_fit() {
            cards[0] = dashboard::busy_cpu_demo();
        }
        cards[0].graphs[0].label = format!("Total · {tick}%");
        apply_local_sample(&wide, &narrow, &data, cards, true);
        assert_eq!(
            ACTIVE_THEME.get(),
            selected,
            "local telemetry reset selected theme"
        );
        let mut layout = active_theme().layout;
        if screen_fit() {
            assert_eq!(ACTIVE_DENSITY.get(), Density::ScreenFit);
            layout.cpu = theme::CpuLayout::Compact;
            layout.weather = theme::WeatherLayout::CurveRows;
            layout.telemetry = theme::TelemetryLayout::Inline;
        }
        let assert_layout = |stack: &gtk::Box| {
            let root = stack.clone().upcast();
            let cpu = match layout.cpu {
                theme::CpuLayout::Compact => "cpu-compact",
                theme::CpuLayout::Roomy => "cpu-roomy",
            };
            let weather = match layout.weather {
                theme::WeatherLayout::CurveRows => "weather-curve",
                theme::WeatherLayout::ConditionStrip => "weather-strip",
            };
            assert!(
                find_widget(&root, cpu).is_some(),
                "selected CPU geometry lost on update"
            );
            assert!(
                find_widget(&root, weather).is_some(),
                "selected weather geometry lost on update"
            );
            assert_eq!(
                find_widget(&root, "telemetry-inline").is_some(),
                layout.telemetry == theme::TelemetryLayout::Inline
            );
        };
        assert_layout(&wide);
        assert_layout(&narrow);
        // Exercise the full report-render path separately from local-only updates.
        render_dashboard(&wide, &narrow, &data.borrow(), true);
        assert_eq!(
            ACTIVE_THEME.get(),
            selected,
            "report render reset selected theme"
        );
        assert_layout(&wide);
        assert_layout(&narrow);
        let picker = find_widget(&selector.clone().upcast(), "theme-name")
            .unwrap()
            .downcast::<gtk::MenuButton>()
            .unwrap();
        assert!(picker.label().unwrap().contains(active_theme().name));
        assert!(window.is_visible());
        eprintln!(
            "THEME_LIVE_UPDATE tick={tick} selected={} local_and_report_preserved=true",
            active_theme().id
        );
        if tick == 3 {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn report_failure(
    wide: &gtk::Box,
    narrow: &gtk::Box,
    data: &Rc<RefCell<DashboardData>>,
    error: &str,
) {
    let mut data = data.borrow_mut();
    data.error = Some(error.into());
    for card in &mut data.providers {
        if card.state == CardState::Ready {
            card.state = CardState::Stale;
        }
        for metric in &mut card.metrics {
            metric.pace = None;
        }
    }
    render_dashboard(wide, narrow, &data, false);
}

#[derive(Clone)]
struct RenderSnapshot {
    cards: Vec<Card>,
    key: (usize, Density, i32, i32),
}

fn viewport(stack: &gtk::Box, wide: bool) -> (i32, i32) {
    let scroll = stack
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.downcast::<gtk::ScrolledWindow>().ok());
    scroll
        .map(|s| (s.width(), s.height()))
        .filter(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or((if wide { 640 } else { 378 }, 1100))
}
/// Update `old` in place from `new` where the two widget trees line up, so GTK
/// invalidates the widgets whose content actually changed instead of destroying
/// a subtree and rebuilding it at the same allocation. Destroying and recreating
/// at the same spot can leave the removed widget's ink on the surface: a
/// previous reading's digit stroke survived over a new digit that way. Returns
/// false when the shapes differ, so the caller can replace the subtree instead.
fn reconcile(old: &gtk::Widget, new: &gtk::Widget) -> bool {
    if let (Ok(old), Ok(new)) = (
        old.clone().downcast::<gtk::Label>(),
        new.clone().downcast::<gtk::Label>(),
    ) {
        if old.text() != new.text() {
            old.set_text(&new.text());
        }
        old.set_css_classes(
            &new.css_classes()
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        );
        old.set_tooltip_text(new.tooltip_text().as_deref());
        return true;
    }
    if let (Ok(old), Ok(new)) = (
        old.clone().downcast::<gtk::MenuButton>(),
        new.clone().downcast::<gtk::MenuButton>(),
    ) {
        let context = |button: &gtk::MenuButton| {
            button
                .popover()
                .and_then(|popover| find_widget(&popover.upcast(), "info-context"))
                .and_then(|label| label.downcast::<gtk::Label>().ok())
        };
        if let (Some(old), Some(new)) = (context(&old), context(&new))
            && old.text() != new.text()
        {
            old.set_text(&new.text());
        }
        return true;
    }
    // A graph is a DrawingArea whose draw closure captured its samples, so it
    // cannot be updated in place and the whole subtree is rebuilt instead.
    // Moving a widget between parents to swap it out is not safe for every
    // layout, so this never relocates children.
    if old.clone().downcast::<gtk::DrawingArea>().is_ok()
        || new.clone().downcast::<gtk::DrawingArea>().is_ok()
    {
        return false;
    }
    // Only a Box keeps child order as layout; a Grid would lose its cell.
    if old.type_() != new.type_()
        || old.clone().downcast::<gtk::Box>().is_err()
        || new.clone().downcast::<gtk::Box>().is_err()
    {
        return false;
    }
    let children = |widget: &gtk::Widget| {
        let mut out = Vec::new();
        let mut child = widget.first_child();
        while let Some(widget) = child {
            out.push(widget.clone());
            child = widget.next_sibling();
        }
        out
    };
    let (old_children, new_children) = (children(old), children(new));
    if old_children.len() != new_children.len() {
        return false;
    }
    old_children
        .iter()
        .zip(&new_children)
        .all(|(old, new)| reconcile(old, new))
}

fn replace_contents(view: &gtk::Box, fresh: &gtk::Box) {
    if screen_fit() {
        // Keep the keyboard-reachable source Info control rooted even when this
        // card's readings change. Only its text and the noninteractive body update.
        let header = view.first_child().unwrap();
        let fresh_header = fresh.first_child().unwrap();
        let mut old_child = header.first_child();
        let mut new_child = fresh_header.first_child();
        while let (Some(old), Some(new)) = (old_child, new_child) {
            if let (Ok(old), Ok(new)) = (
                old.clone().downcast::<gtk::Label>(),
                new.clone().downcast::<gtk::Label>(),
            ) {
                old.set_text(&new.text());
                old.set_css_classes(
                    &new.css_classes()
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>(),
                );
                old.set_tooltip_text(new.tooltip_text().as_deref());
            }
            if let (Ok(old), Ok(new)) = (
                old.clone().downcast::<gtk::MenuButton>(),
                new.clone().downcast::<gtk::MenuButton>(),
            ) {
                let old_text = find_widget(&old.popover().unwrap().upcast(), "info-context")
                    .unwrap()
                    .downcast::<gtk::Label>()
                    .unwrap();
                let new_text = find_widget(&new.popover().unwrap().upcast(), "info-context")
                    .unwrap()
                    .downcast::<gtk::Label>()
                    .unwrap();
                old_text.set_text(&new_text.text());
            }
            old_child = old.next_sibling();
            new_child = new.next_sibling();
        }
        // Update the body in place when the two trees line up, so a changed
        // reading is a label whose text was set rather than a subtree destroyed
        // and rebuilt at the same allocation. The latter can leave the previous
        // value's ink on the surface, which showed up as a stray stroke from an
        // old digit sitting on the new one.
        let mut old = header.next_sibling();
        let mut new = fresh_header.next_sibling();
        let mut reconciled = true;
        while let (Some(old_widget), Some(new_widget)) = (old.clone(), new.clone()) {
            if !reconcile(&old_widget, &new_widget) {
                reconciled = false;
                break;
            }
            old = old_widget.next_sibling();
            new = new_widget.next_sibling();
        }
        if reconciled && old.is_none() && new.is_none() {
            if std::env::var_os("SIDESCOPE_DEBUG_REPLACE").is_some() {
                eprintln!("RECONCILE {}", view.widget_name());
            }
            return;
        }
        if std::env::var_os("SIDESCOPE_DEBUG_REPLACE").is_some() {
            eprintln!("REPLACE {}", view.widget_name());
        }
        while let Some(child) = header.next_sibling() {
            view.remove(&child);
        }
        fresh.remove(&fresh_header);
    } else {
        while let Some(child) = view.first_child() {
            view.remove(&child);
        }
    }
    while let Some(child) = fresh.first_child() {
        fresh.remove(&child);
        view.append(&child);
    }
    // Re-created body widgets can be slightly narrower than the ones they replace
    // (for example a sensor reading whose digits change width). Without an explicit
    // full repaint the card keeps stale ink from the previous value, which shows up
    // as a stray horizontal stroke over a new digit. Repaint the whole card so the
    // old reading is never composited under the new one.
    view.queue_draw();
}
fn reconcile_report_banner(stack: &gtk::Box, error: Option<&str>) {
    let banner = find_widget(&stack.clone().upcast(), "report-error-banner")
        .map(|w| w.downcast::<gtk::Label>().unwrap())
        .unwrap_or_else(|| {
            let label = text("", "error-banner");
            label.set_widget_name("report-error-banner");
            stack.prepend(&label);
            label
        });
    banner.set_text(
        &error
            .map(|e| format!("USAGE CHECK FAILED · {e}"))
            .unwrap_or_default(),
    );
    banner.set_visible(error.is_some());
}

static TERMINATION_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

extern "C" fn note_termination(_signal: libc::c_int) {
    TERMINATION_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn install_termination_flag() {
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = note_termination as extern "C" fn(libc::c_int) as usize;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGTERM, libc::SIGINT] {
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
}

fn sync_chart_section(stack: &gtk::Box, is_wide: bool, data: &DashboardData) {
    chart_host::sync_charts(
        stack,
        &data.charts,
        if is_wide {
            sidescope::chart_view::CardLayout::Wide
        } else {
            sidescope::chart_view::CardLayout::Narrow
        },
        active_theme(),
    );
}

fn render_dashboard(wide: &gtk::Box, narrow: &gtk::Box, data: &DashboardData, demo: bool) {
    let cards: Vec<_> = data
        .providers
        .iter()
        .chain(&data.local)
        .chain(&data.remote)
        .cloned()
        .collect();
    for (stack, is_wide) in [(wide, true), (narrow, false)] {
        let (width, height) = viewport(stack, is_wide);
        // Hidden responsive stacks do not have useful allocations; use their intended width.
        let width = if is_wide {
            width.max(512)
        } else {
            width.min(500)
        };
        let key = (ACTIVE_THEME.get(), ACTIVE_DENSITY.get(), width, height);
        let id = stack.as_ptr() as usize;
        let previous = RENDER_CACHE.with_borrow(|cache| cache.get(&id).cloned());
        let reusable = previous.as_ref().is_some_and(|p| {
            p.key == key
                && p.cards
                    .iter()
                    .map(|c| &c.id)
                    .eq(cards.iter().map(|c| &c.id))
        });
        if reusable {
            let previous = previous.unwrap();
            for (index, card) in cards.iter().enumerate() {
                if previous.cards[index] == *card {
                    continue;
                }
                let view = find_widget(&stack.clone().upcast(), &format!("card-{}", card.id))
                    .unwrap()
                    .downcast::<gtk::Box>()
                    .unwrap();
                FIT_CORE_COLUMNS.set(if view.has_css_class("fit-three") {
                    3
                } else if is_wide {
                    4
                } else {
                    2
                });
                replace_contents(&view, &card_view(card, index, cards.len(), demo));
            }
        } else {
            // The chart section survives a rebuild so its focus and open Details do.
            let mut child = stack.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                if widget.widget_name() != "chart-section" {
                    stack.remove(&widget);
                }
            }
            if data.providers.is_empty() {
                stack.append(&text(
                    "No AI providers shown. Configure ai-usagebar / enabled provider IDs.",
                    "footnote",
                ));
            }
            if !screen_fit() {
                let section = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                section.add_css_class("section-heading");
                let heading = text("DASHBOARD", "section-title");
                heading.set_hexpand(true);
                section.append(&heading);
                section.append(&text(&format!("{:02} CARDS", cards.len()), "section-count"));
                stack.append(&section);
            }
            stack.append(&if screen_fit() {
                fit_cards_layout(&cards, demo, is_wide, width - 24, height - 24)
            } else {
                cards_layout(&cards, demo, is_wide)
            });
        }
        reconcile_report_banner(stack, data.error.as_deref());
        sync_chart_section(stack, is_wide, data);
        RENDER_CACHE.with_borrow_mut(|cache| {
            cache.insert(
                id,
                RenderSnapshot {
                    cards: cards.clone(),
                    key,
                },
            );
        });
    }
}

/// Updates change only differing card contents. Unchanged provider/remote hosts,
/// focus and open Details survive both local sampling and whole-report checks.
fn apply_local_sample(
    wide: &gtk::Box,
    narrow: &gtk::Box,
    data: &Rc<RefCell<DashboardData>>,
    cards: Vec<Card>,
    demo: bool,
) {
    data.borrow_mut().local = cards;
    render_dashboard(wide, narrow, &data.borrow(), demo);
}

fn fit_cards_layout(
    cards: &[Card],
    demo: bool,
    wide_stack: bool,
    width: i32,
    available: i32,
) -> gtk::Widget {
    FIT_CORE_COLUMNS.set(if wide_stack { 4 } else { 2 });
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let cpu_index = cards.iter().position(|c| c.id == "cpu");
    let mut views: Vec<_> = cards
        .iter()
        .enumerate()
        .map(|(index, card)| card_view(card, index, cards.len(), demo))
        .collect();
    // The responsive breakpoint picks this stack, but a two-column pack also
    // needs each column to hold its widest card. Below that, one column in
    // source order is the honest layout, and it avoids measuring a card under
    // its own minimum width.
    let two_column_width = (width - 8) / 2;
    let wide = wide_stack
        && two_column_width
            >= views
                .iter()
                .enumerate()
                .filter(|(index, _)| Some(*index) != cpu_index)
                .map(|(_, view)| view.measure(gtk::Orientation::Horizontal, -1).0)
                .max()
                .unwrap_or(0);
    if wide_stack && !wide {
        FIT_CORE_COLUMNS.set(2);
        if let Some(index) = cpu_index {
            views[index] = card_view(&cards[index], index, cards.len(), demo);
        }
    }
    let cpu = if wide { cpu_index } else { None };
    let card_width = if wide { two_column_width } else { width };
    let heights: Vec<_> = views
        .iter()
        .enumerate()
        .map(|(index, view)| {
            // A spanned CPU card is measured at its real, full width.
            let probe = if wide && Some(index) == cpu {
                width
            } else {
                card_width
            };
            view.measure(gtk::Orientation::Vertical, probe).1
        })
        .collect();
    let mut cpu_height = cpu
        .map(|i| views[i].measure(gtk::Orientation::Vertical, width).1)
        .unwrap_or(0);
    let mut plan = density::plan(
        &heights,
        cpu,
        cpu_height,
        if wide { width.max(501) } else { width.min(500) },
        available,
    );
    // Exactly two candidates: prefer broader three-cell traces when they fit the
    // actual viewport; otherwise use four. Never repeatedly compress or hide data.
    if let Some(i) = cpu {
        FIT_CORE_COLUMNS.set(3);
        let roomy = card_view(&cards[i], i, cards.len(), demo);
        let candidate = density::plan(
            &heights,
            cpu,
            roomy.measure(gtk::Orientation::Vertical, width).1,
            width.max(501),
            available,
        );
        let prefer_three = !candidate.overflows;
        plan = density::choose_core_plan(plan, candidate);
        if prefer_three {
            cpu_height = roomy.measure(gtk::Orientation::Vertical, width).1;
            views[i] = roomy;
        }
        FIT_CORE_COLUMNS.set(plan.core_columns);
    }
    if let Some(i) = cpu {
        body.append(&views[i]);
    }
    let columns = [
        gtk::Box::new(gtk::Orientation::Vertical, 8),
        gtk::Box::new(gtk::Orientation::Vertical, 8),
    ];
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_homogeneous(true);
    for column in &columns {
        column.set_hexpand(true);
        column.set_valign(gtk::Align::Start);
        row.append(column);
    }
    for (i, (view, column)) in views.into_iter().zip(&plan.columns).enumerate() {
        if Some(i) == cpu {
            continue;
        }
        if wide {
            columns[*column].append(&view);
        } else {
            body.append(&view);
        }
        if std::env::var_os("SIDESCOPE_VISUAL_GEOMETRY").is_some() {
            eprintln!(
                "FIT_CARD {} height={} width={card_width}",
                cards[i].id, heights[i]
            );
        }
    }
    if wide {
        body.append(&row);
    }
    if std::env::var_os("SIDESCOPE_VISUAL_GEOMETRY").is_some() {
        eprintln!(
            "FIT_PLAN width={width} available={available} height={} overflow={} core_columns={} cpu_height={cpu_height}",
            plan.height, plan.overflows, plan.core_columns
        );
    }
    body.upcast()
}

fn start_local_sampling(
    window: &adw::ApplicationWindow,
    wide: &gtk::Box,
    narrow: &gtk::Box,
    data: &Rc<RefCell<DashboardData>>,
    sensor_config: &sensors::SensorConfig,
) {
    let sensor_config = sensor_config.clone();
    // One bounded channel prevents an inactive UI from accumulating samples.
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut sampler = dashboard::Sampler::default();
        let mut sensor_sampler = sensors::Sampler::default();
        loop {
            let mut cards = sampler.sample();
            cards.push(sensor_sampler.sample(
                std::path::Path::new("/sys/class/hwmon"),
                &sensor_config,
                sidescope::current_unix_time(),
            ));
            if tx.send(cards).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_secs(dashboard::SAMPLE_SECS));
        }
    });
    let window = window.downgrade();
    let wide = wide.clone();
    let narrow = narrow.clone();
    let data = data.clone();
    glib::timeout_add_local(Duration::from_millis(200), move || {
        if window.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        if let Ok(cards) = rx.try_recv() {
            apply_local_sample(&wide, &narrow, &data, cards, false);
        }
        glib::ControlFlow::Continue
    });
}

fn merge_remote(previous: &mut Vec<Card>, id: &str, title: &str, result: Result<Card, String>) {
    let old = previous.iter().position(|c| c.id == id);
    let card = match result {
        Ok(mut card) => {
            card.note
                .push_str(&format!("\nChecked {}", local_check_time()));
            card
        }
        Err(error) => match old
            .and_then(|i| previous.get(i))
            .filter(|c| !matches!(c.state, CardState::Unavailable(_)))
        {
            Some(card) => {
                let mut card = card.clone();
                card.state = CardState::Stale;
                card.note = format!(
                    "{}\nREFRESH FAILED: {error} · last success retained",
                    card.note.split("\nREFRESH FAILED:").next().unwrap_or("")
                );
                card
            }
            None => dashboard::unavailable(id, title, &error),
        },
    };
    if let Some(i) = old {
        previous[i] = card;
    } else {
        previous.push(card);
    }
}

fn refresh_weather(
    window: &adw::ApplicationWindow,
    wide: &gtk::Box,
    narrow: &gtk::Box,
    data: &Rc<RefCell<DashboardData>>,
    location: &weather::WeatherConfig,
    busy: &Rc<std::cell::Cell<bool>>,
) {
    if busy.replace(true) {
        return;
    }
    let (tx, rx) = mpsc::channel();
    let location = location.clone();
    std::thread::spawn(move || {
        let forecast = process::json_url(&location.forecast_url())
            .and_then(|json| weather::forecast_card(&json, &location));
        let air = process::json_url(&location.air_url())
            .and_then(|json| weather::air_card(&json, &location));
        let _ = tx.send((forecast, air));
    });
    let window = window.downgrade();
    let wide = wide.clone();
    let narrow = narrow.clone();
    let data = data.clone();
    let busy = busy.clone();
    glib::timeout_add_local(Duration::from_millis(200), move || {
        if window.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        match rx.try_recv() {
            Ok((forecast, air)) => {
                let mut data = data.borrow_mut();
                merge_remote(&mut data.remote, "weather", "Weather", forecast);
                merge_remote(&mut data.remote, "air", "Air quality", air);
                render_dashboard(&wide, &narrow, &data, false);
                busy.set(false);
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                busy.set(false);
                glib::ControlFlow::Break
            }
        }
    });
}

fn cards_layout(cards: &[Card], demo: bool, wide_mode: bool) -> gtk::Widget {
    // Screen-fit packs the same readings into the window the density budgets are
    // measured at; a tighter gap between cards buys that back without narrowing
    // the space inside a card.
    let gap = if screen_fit() { 4 } else { 8 };
    if !wide_mode || cards.len() <= 1 {
        let narrow = gtk::Box::new(gtk::Orientation::Vertical, gap);
        for (index, card) in cards.iter().enumerate() {
            narrow.append(&card_view(card, index, cards.len(), demo));
        }
        return narrow.upcast();
    }

    let wide = gtk::Box::new(gtk::Orientation::Horizontal, gap);
    wide.set_homogeneous(true);
    let columns = [
        gtk::Box::new(gtk::Orientation::Vertical, gap),
        gtk::Box::new(gtk::Orientation::Vertical, gap),
    ];
    for column in &columns {
        column.set_hexpand(true);
        column.set_valign(gtk::Align::Start);
        wide.append(column);
    }
    let views: Vec<_> = cards
        .iter()
        .enumerate()
        .map(|(index, card)| {
            let view = card_view(card, index, cards.len(), demo);
            view.set_size_request(220, -1);
            view
        })
        .collect();
    let heights: Vec<_> = views
        .iter()
        .map(|view| view.measure(gtk::Orientation::Vertical, 288).1)
        .collect();
    if std::env::var_os("SIDESCOPE_VISUAL_GEOMETRY").is_some() {
        for (card, view) in cards.iter().zip(&views) {
            let wide_height = view.measure(gtk::Orientation::Vertical, 288).1;
            let narrow_height = view.measure(gtk::Orientation::Vertical, 350).1;
            eprintln!(
                "CARD_GEOMETRY {} wide={wide_height} narrow={narrow_height}",
                card.id
            );
            if demo && card.state == CardState::Ready && active_theme().id == "original" {
                if card.weather.is_some() {
                    let condition = find_widget(&view.clone().upcast(), "weather-condition")
                        .unwrap()
                        .downcast::<gtk::Label>()
                        .unwrap();
                    assert!(
                        !condition.wraps(),
                        "ordinary weather conditions must never word-wrap at 288px card width"
                    );
                    // 376 before the renderer hinted font metrics, 383 after:
                    // rounding each line's ascent and descent costs a seven-line
                    // card about seven pixels. The budget keeps the four pixels
                    // of slack over the measurement that it kept before.
                    assert!(
                        wide_height <= 387 && narrow_height <= 387,
                        "weather density regressed: {wide_height}/{narrow_height}"
                    );
                }
                if card.id == "cpu" {
                    let rows = card.graphs.len().saturating_sub(1).div_ceil(2);
                    assert!(
                        wide_height
                            <= 180
                                + rows as i32
                                    * (active_theme().layout.cpu.row_max_height()
                                        + active_theme().layout.cpu.row_spacing() as i32),
                        "CPU density regressed: {wide_height}"
                    );
                }
            }
        }
    }
    for (view, column) in views.into_iter().zip(pack_columns(&heights)) {
        columns[column].append(&view);
    }

    wide.upcast()
}

/// Whether a metric's supporting text gets its own line under the row.
///
/// Screen-fit carries provider reset context in the pace legend instead, so
/// nothing earns a line there; the swap card's explicit absence is the
/// exception and is never hidden in either density.
fn shows_metric_detail(card_id: &str, screen_fit: bool, detail: &str) -> bool {
    if detail.is_empty() {
        return false;
    }
    let no_swap = detail == "No swap configured";
    let telemetry = matches!(card_id, "ram" | "network" | "disk");
    no_swap || (!telemetry && !screen_fit)
}

/// The marks a quota row draws. Elapsed time is measured from the reset window
/// and the clock, so it outlives the reading it is measured against; a verdict
/// compares that reading with the elapsed fraction and needs it to be current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuotaMarks {
    tick: Option<u8>,
    verdict: Option<Pace>,
}

fn quota_marks(state: &CardState, metric: &CardMetric) -> QuotaMarks {
    let tick = metric.elapsed_pct;
    QuotaMarks {
        tick,
        // A verdict compares the reading with the elapsed fraction, so it
        // cannot outlive the tick it is measured against; only a current
        // reading may state one at all.
        verdict: tick.and(metric.pace).filter(|_| *state == CardState::Ready),
    }
}

/// Whether the generic full-width graph rows render for this card.
/// Dedicated views own their graphs and must not see them repeated:
/// cpu_view, inline_telemetry (ram/network in the inline layouts), and
/// sensor_view (each temperature channel's mini-plot). A sensors card
/// without metrics never reaches sensor_view, so it keeps the fallback.
fn shows_generic_graphs(card: &Card) -> bool {
    if card.id == "cpu" {
        return false;
    }
    if card.id == "sensors" && !card.metrics.is_empty() {
        return false;
    }
    !(matches!(card.id.as_str(), "ram" | "network")
        && (screen_fit() || active_theme().layout.telemetry == theme::TelemetryLayout::Inline))
}

fn card_view(card: &Card, index: usize, total: usize, demo: bool) -> gtk::Box {
    let box_ = gtk::Box::new(
        gtk::Orientation::Vertical,
        if screen_fit() {
            0
        } else {
            active_theme().layout.card_spacing
        },
    );
    box_.set_widget_name(&format!("card-{}", card.id));
    box_.add_css_class("usage-card");
    box_.add_css_class(
        if matches!(
            card.id.as_str(),
            "cpu" | "ram" | "network" | "disk" | "sensors"
        ) {
            "local-card"
        } else if matches!(card.id.as_str(), "weather" | "air") {
            "remote-card"
        } else {
            "provider-card"
        },
    );
    if screen_fit() {
        box_.add_css_class("fit-card");
        box_.add_css_class(if FIT_CORE_COLUMNS.get() == 3 {
            "fit-three"
        } else {
            "fit-four"
        });
    }
    if index == 0 {
        box_.add_css_class("featured");
    }
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    // The header row carries the title, status and Info button; the density
    // stylesheet gives it the breathing room above the first reading row.
    top.add_css_class("card-head");
    let kicker = text(&format!("{:02}  /  {:02}", index + 1, total), "card-kicker");
    if !screen_fit() {
        top.append(&kicker);
    }
    let space = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    space.set_hexpand(true);
    top.append(&space);
    // History cards name the span their trend lines cover; without it the
    // sparkline has no visible time axis at all. Cards without graphs stay silent.
    if !card.graphs.is_empty() {
        let span = text(&dashboard::history_window(), "metric-detail");
        span.set_wrap(false);
        span.set_ellipsize(gtk::pango::EllipsizeMode::End);
        span.set_tooltip_text(Some(&format!(
            "Trend window: {} fixed slots × {}s, newest right ({})",
            dashboard::HISTORY_LIMIT,
            dashboard::SAMPLE_SECS,
            dashboard::history_window()
        )));
        top.append(&span);
    }
    let dot = text(
        match card.state {
            CardState::Ready if demo => "●  SAMPLE",
            CardState::Ready => "●  LIVE",
            CardState::Stale => "●  STALE",
            CardState::Unavailable(_) => "○  OFFLINE",
        },
        "state",
    );
    if !matches!(card.state, CardState::Ready) {
        dot.add_css_class("muted");
    }
    dot.set_halign(gtk::Align::End);
    dot.set_hexpand(false);
    top.append(&dot);
    if screen_fit() {
        let title = text(&card.title, "card-title");
        title.set_hexpand(true);
        title.set_tooltip_text(Some(&format!("{}\n{}", card.subtitle, card.note)));
        top.prepend(&title);
        let details = gtk::MenuButton::new();
        details.set_label("Info");
        details.add_css_class("forecast-detail");
        details.set_widget_name(&format!("info-{}", card.id));
        let popover = gtk::Popover::new();
        let context = card_context(card);
        let label = text(&context, "hourly-details");
        label.set_widget_name("info-context");
        label.set_max_width_chars(38);
        let scroll = gtk::ScrolledWindow::builder()
            .min_content_width(240)
            .max_content_width(320)
            .propagate_natural_width(true)
            .max_content_height(420)
            .propagate_natural_height(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        scroll.set_child(Some(&label));
        popover.set_child(Some(&scroll));
        details.set_popover(Some(&popover));
        top.append(&details);
    }
    box_.append(&top);
    if !screen_fit() {
        box_.append(&text(&card.title, "card-title"));
    }
    if !screen_fit() && !card.subtitle.is_empty() {
        let subtitle = text(&card.subtitle, "card-subtitle");
        subtitle.set_tooltip_text(Some(&card.subtitle));
        box_.append(&subtitle);
    }
    match &card.state {
        _ if card.id == "sensors" && !card.metrics.is_empty() => box_.append(&sensor_view(card)),
        CardState::Unavailable(reason) => box_.append(&text(reason, "unavailable")),
        _ if card.weather.is_some() => box_.append(&weather_view(card.weather.as_ref().unwrap())),
        _ if matches!(card.id.as_str(), "ram" | "network")
            && (screen_fit()
                || active_theme().layout.telemetry == theme::TelemetryLayout::Inline) =>
        {
            box_.append(&inline_telemetry(card));
        }
        _ if card.metrics.is_empty() && card.graphs.is_empty() => {
            box_.append(&text("No metrics reported", "unavailable"))
        }
        _ => {
            for (index, metric) in card.metrics.iter().enumerate() {
                let marks = quota_marks(&card.state, metric);
                let detail = if marks.tick.is_some() {
                    pace_context(&metric.detail)
                } else {
                    metric.detail.clone()
                };
                let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
                row.add_css_class("metric");
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                line.append(&text(&metric.label, "metric-label"));
                let value = text(&metric.value, "metric-value");
                value.set_hexpand(true);
                value.set_halign(gtk::Align::End);
                line.append(&value);
                row.append(&line);
                if let Some(percent) = metric.fraction {
                    let bar = gtk::ProgressBar::new();
                    bar.set_fraction(percent as f64 / 100.0);
                    bar.add_css_class("meter");
                    if let Some(elapsed) = marks.tick {
                        // A marked bar is taller so the tick reads as a mark. The
                        // tick itself is the time position; only a current
                        // reading also colors the bar by pace direction.
                        bar.add_css_class("time-marked");
                        let mut tooltip =
                            format!("{percent}% used; {elapsed}% of the window elapsed");
                        if let Some(pace) = marks.verdict {
                            bar.add_css_class(pace_class(pace));
                            tooltip.push_str(&format!("; {}", pace_label(pace)));
                        }
                        bar.set_tooltip_text(Some(&tooltip));
                        if demo {
                            eprintln!(
                                "TIME_MARK card={} metric={index} elapsed={elapsed} verdict={}",
                                card.id,
                                marks.verdict.map_or("none", |pace| pace_class(pace))
                            );
                        }
                        let marked_bar = gtk::Overlay::new();
                        marked_bar.set_child(Some(&bar));
                        let marker = gtk::DrawingArea::new();
                        marker.set_can_target(false);
                        marker.set_draw_func(move |_, cr, width, height| {
                            if f64::from(width) < PACE_TICK_OUTLINE_WIDTH {
                                return;
                            }
                            let x = pace_tick_x(width, elapsed);
                            source(cr, active_theme().plot.marker_outline);
                            cr.set_line_width(PACE_TICK_OUTLINE_WIDTH);
                            cr.move_to(x, 0.0);
                            cr.line_to(x, f64::from(height));
                            let _ = cr.stroke();
                            source(cr, active_theme().plot.marker);
                            cr.set_line_width(PACE_TICK_WIDTH);
                            cr.move_to(x, 0.0);
                            cr.line_to(x, f64::from(height));
                            let _ = cr.stroke();
                        });
                        marked_bar.add_overlay(&marker);
                        row.append(&marked_bar);
                        let legend = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                        match marks.verdict {
                            Some(pace) => {
                                let summary = text(&pace_label(pace), "pace-label");
                                summary.add_css_class(pace_class(pace));
                                summary.set_hexpand(true);
                                legend.append(&summary);
                            }
                            // Nothing to state about direction, but the time
                            // position below the bar still needs its legend on
                            // the trailing edge.
                            None => {
                                let space = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                                space.set_hexpand(true);
                                legend.append(&space);
                            }
                        }
                        // Screen-fit has no room for a separate reset line, so the
                        // time legend carries the reset window that Comfortable
                        // shows below it. The bar's tick still marks elapsed time.
                        if screen_fit() && !detail.is_empty() {
                            let reset = text(&detail, "pace-time");
                            reset.set_wrap(false);
                            reset.set_xalign(1.0);
                            reset.set_ellipsize(gtk::pango::EllipsizeMode::End);
                            reset.set_tooltip_text(Some(&detail));
                            legend.append(&reset);
                        } else {
                            let time = text(&format!("TIME {elapsed}%"), "pace-time");
                            time.set_wrap(false);
                            time.set_xalign(1.0);
                            legend.append(&time);
                        }
                        row.append(&legend);
                    } else {
                        bar.set_tooltip_text(Some(&format!("{percent}% used")));
                        row.append(&bar);
                    }
                }
                if !detail.is_empty() {
                    row.set_tooltip_text(Some(&detail));
                    if shows_metric_detail(&card.id, screen_fit(), &detail) {
                        row.append(&text(&detail, "metric-detail"));
                    }
                }
                box_.append(&row);
            }
        }
    }
    if card.id == "cpu" {
        box_.append(&cpu_view(&card.graphs));
    } else if shows_generic_graphs(card) {
        for graph in &card.graphs {
            box_.append(&graph_view(graph));
        }
    }
    if !card.note.is_empty() {
        let summary = if screen_fit() {
            fit_card_note(card)
        } else {
            compact_card_note(card)
        };
        let note = text(&summary, "source-note");
        note.set_tooltip_text(Some(&card.note));
        if !summary.is_empty() {
            box_.append(&note);
        }
    }
    box_
}

fn sensor_view(card: &Card) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
    body.add_css_class("sensor-readings");
    for metric in &card.metrics {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
        row.set_tooltip_text(Some(&metric.detail));
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label = text(&metric.label, "metric-label");
        label.set_hexpand(true);
        let value = text(&metric.value, "metric-value");
        value.set_halign(gtk::Align::End);
        line.append(&label);
        line.append(&value);
        row.append(&line);
        // Temperature channels carry a stable auto-ranged mini plot beneath
        // their reading row; fans stay text-only.
        if let Some(graph) = card
            .graphs
            .iter()
            .find(|g| g.label == metric.label && g.signed_samples.is_some())
        {
            row.append(&history_plot(graph, 30, false));
        }
        if let Some(status) = metric
            .detail
            .lines()
            .next()
            .filter(|s| s.starts_with("STALE") || s.starts_with("UNAVAILABLE"))
        {
            row.append(&text(status, "metric-detail"));
        }
        body.append(&row);
    }
    body
}

fn inline_telemetry(card: &Card) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body.set_widget_name("telemetry-inline");
    for (index, metric) in card.metrics.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let readings = gtk::Box::new(gtk::Orientation::Vertical, 2);
        readings.set_size_request(90, -1);
        let label = text(&metric.label, "metric-label");
        if screen_fit()
            && card.id == "ram"
            && let Some(percent) = card
                .graphs
                .get(index)
                .and_then(|g| g.label.rsplit_once(" · ").map(|(_, p)| p))
                .filter(|p| p.ends_with('%'))
        {
            let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            label.set_hexpand(true);
            header.append(&label);
            header.append(&text(percent, "metric-detail"));
            readings.append(&header);
        } else {
            readings.append(&label);
        }
        let value = text(&metric.value.replace(" / ", "\n/ "), "metric-value");
        value.set_max_width_chars(11);
        value.set_tooltip_text(Some(&metric.value));
        readings.append(&value);
        readings.set_tooltip_text(Some(&metric.detail));
        row.append(&readings);
        if let Some(graph) = card.graphs.get(index) {
            let plot = if screen_fit() {
                let p = gtk::Box::new(gtk::Orientation::Vertical, 0);
                p.append(&history_plot(graph, 30, false));
                p
            } else {
                graph_view(graph)
            };
            plot.set_hexpand(true);
            row.append(&plot);
        } else if !metric.detail.is_empty() {
            row.append(&text(&metric.detail, "metric-detail"));
        }
        body.append(&row);
    }
    body
}

fn card_context(card: &Card) -> String {
    let mut lines = vec![card.title.clone(), card.subtitle.clone(), card.note.clone()];
    for m in &card.metrics {
        lines.push(format!("{} · {}\n{}", m.label, m.value, m.detail));
        // How far along the window is comes from the clock, so Info states it
        // for a stale reading too; only the verdict needs a current one.
        if let Some(elapsed) = m.elapsed_pct {
            lines.push(match m.pace.filter(|_| card.state == CardState::Ready) {
                Some(pace) => format!("{}; time {elapsed}%", pace_label(pace)),
                None => format!("time {elapsed}% elapsed"),
            });
        }
    }
    for graph in &card.graphs {
        // Millidegree ranges are multiples of 5 °C by construction, so the
        // division below is exact; presentation owns the unit scaling, the
        // same way sensor readings own theirs.
        let scale = match (&graph.unit, graph.range) {
            (Some(unit), Some((floor, ceiling))) => format!(
                "Y {}–{} {unit}",
                floor.div_euclid(1000),
                ceiling.div_euclid(1000)
            ),
            _ if graph.ceiling == 100 => "Y 0–100%".into(),
            _ => format!("Y 0–{} bytes/s (auto)", graph.ceiling),
        };
        lines.push(format!(
            "{}; {scale}; {} window",
            graph.label,
            dashboard::history_window()
        ));
    }
    lines.join("\n")
}
fn sensor_card_note(card: &Card) -> String {
    card.note
        .lines()
        .filter(|line| !line.starts_with("Auto retention:"))
        .collect::<Vec<_>>()
        .join("\n")
}
fn fit_card_note(card: &Card) -> String {
    if card.id == "sensors" {
        return sensor_card_note(card);
    }
    card.note
        .lines()
        .filter(|line| {
            !line.starts_with("Daily spend history unavailable")
                && !line.starts_with("Linux ·")
                && !line.starts_with("Auto 30 min")
                // Key tier and recent-model prose lives in Info only.
                && !line.starts_with("Key tier:")
                && !line.starts_with("Recent models:")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact_card_note(card: &Card) -> String {
    if card.id == "sensors" {
        return sensor_card_note(card);
    }
    // Shorten prose only; source-fetched timestamps remain visible, including stale ones.
    // Key tier and recent-model prose lives in Info only.
    card.note
        .lines()
        .filter(|line| !line.starts_with("Key tier:") && !line.starts_with("Recent models:"))
        .collect::<Vec<_>>()
        .join("\n")
        .replacen(
            "Daily spend history unavailable: no dated provider history in this report.",
            "Daily spend history unavailable",
            1,
        )
}

fn graph_view(graph: &Graph) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    row.add_css_class("history-row");
    row.append(&text(&graph.label, "metric-detail"));
    row.append(&history_plot(graph, 30, false));
    row
}

fn history_plot(graph: &Graph, height: i32, compact: bool) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_height(height);
    area.set_hexpand(true);
    let graph = graph.clone();
    // A unit-carrying graph names its own stable range; the tooltip must
    // never claim bytes/s for a temperature trace.
    let (scale, slots) = match (&graph.unit, graph.range, &graph.signed_samples) {
        (Some(unit), Some((floor, ceiling)), Some(samples)) => (
            format!(
                "Y {}–{} {unit}",
                floor.div_euclid(1000),
                ceiling.div_euclid(1000)
            ),
            samples.len(),
        ),
        _ => (
            format!(
                "Y 0–{}{}",
                graph.ceiling,
                if graph.ceiling == 100 {
                    "%"
                } else {
                    " bytes/s (auto)"
                }
            ),
            graph.samples.len(),
        ),
    };
    area.set_tooltip_text(Some(&format!(
        "{}; fixed {} slots × {}s ({} window), newest right; {} slots collected; gaps mean missing data. {scale}",
        graph.label,
        dashboard::HISTORY_LIMIT,
        dashboard::SAMPLE_SECS,
        dashboard::history_window(),
        slots,
    )));
    area.set_draw_func(move |_, cr, width, height| {
        let w = f64::from((width - 2).max(0));
        let h = f64::from((height - 4).max(0));
        cr.translate(1.0, 2.0);
        let style = plot::effective_style(
            active_theme().plot,
            w / (dashboard::HISTORY_LIMIT - 1) as f64,
            compact,
        );
        if compact
            && height == theme::CpuLayout::Roomy.core_height()
            && std::env::var_os("SIDESCOPE_VISUAL_GEOMETRY").is_some()
        {
            // Breakpoint transitions can allocate the 220px minimum card briefly.
            assert!(width >= 90, "core plot width regressed: {width}");
        }
        draw_grid(cr, w, h, false, style);
        let points = match (&graph.signed_samples, graph.range) {
            (Some(samples), Some((floor, ceiling))) => {
                plot::ranged_history_points(samples, floor, ceiling, w, h)
            }
            _ => plot::history_points(&graph.samples, graph.ceiling, w, h),
        };
        stroke_points(
            cr,
            &points,
            theme::Series::from_label(&graph.label),
            h,
            style,
        );
    });
    area
}

fn draw_grid(cr: &gtk::cairo::Context, w: f64, h: f64, vertical: bool, style: theme::PlotStyle) {
    source(cr, style.background);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();
    source(cr, style.grid);
    cr.set_line_width(style.grid_width);
    if style.grid_rows == 0 {
        return;
    }
    if vertical {
        for i in [0, 3, 6, 9, 11] {
            let x = w * f64::from(i) / 11.0;
            cr.move_to(x, 0.0);
            cr.line_to(x, h);
        }
    } else {
        for i in 0..style.grid_rows {
            let y = h * f64::from(i) / f64::from(style.grid_rows - 1);
            cr.move_to(0.0, y);
            cr.line_to(w, y);
        }
    }
    let _ = cr.stroke();
}

fn stroke_points(
    cr: &gtk::cairo::Context,
    points: &[Option<(f64, f64)>],
    series: theme::Series,
    baseline: f64,
    style: theme::PlotStyle,
) {
    let color = style.color(series);
    // Fill each contiguous run independently: a missing slot never becomes a bridge.
    if style.fill_alpha > 0.0 {
        for run in points.split(Option::is_none).filter(|run| !run.is_empty()) {
            let first = run[0].unwrap();
            let last = run.last().unwrap().unwrap();
            source(
                cr,
                theme::Color(color.0, color.1, color.2, style.fill_alpha),
            );
            cr.move_to(first.0, baseline);
            for (x, y) in run.iter().flatten() {
                cr.line_to(*x, *y);
            }
            cr.line_to(last.0, baseline);
            cr.close_path();
            let _ = cr.fill();
        }
    }
    source(cr, color);
    cr.set_line_width(style.width);
    cr.set_dash(style.dash, 0.0);
    let mut connected = false;
    for point in points {
        match point {
            Some((x, y)) => {
                if connected {
                    cr.line_to(*x, *y);
                } else {
                    cr.move_to(*x, *y);
                }
                connected = true;
            }
            None => {
                let _ = cr.stroke();
                connected = false;
            }
        }
    }
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
    // Isolated values remain visible even in dense histories; never dot every vertex.
    for (i, point) in points.iter().enumerate() {
        if let Some((x, y)) = point
            && (style.points
                || ((i == 0 || points[i - 1].is_none())
                    && (i + 1 == points.len() || points[i + 1].is_none())))
        {
            cr.arc(*x, *y, 1.7, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
    }
}

fn cpu_view(graphs: &[Graph]) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let cores = gtk::Grid::new();
    cores.set_column_homogeneous(true);
    cores.set_column_spacing(8);
    let layout = if screen_fit() {
        theme::CpuLayout::Compact
    } else {
        active_theme().layout.cpu
    };
    body.set_widget_name(match layout {
        theme::CpuLayout::Compact => "cpu-compact",
        theme::CpuLayout::Roomy => "cpu-roomy",
    });
    cores.set_row_spacing(layout.row_spacing());
    let mut index = 0;
    for graph in graphs {
        if graph.label.starts_with("Total") {
            let total = gtk::Box::new(gtk::Orientation::Vertical, 2);
            if layout == theme::CpuLayout::Compact {
                total.add_css_class("history-row");
            }
            total.append(&text(&graph.label, "metric-detail"));
            let height = if layout == theme::CpuLayout::Compact {
                30
            } else {
                40
            };
            total.append(&history_plot(graph, height, true));
            body.append(&total);
        } else {
            let compact = layout == theme::CpuLayout::Compact;
            let row = gtk::Box::new(
                if compact {
                    gtk::Orientation::Horizontal
                } else {
                    gtk::Orientation::Vertical
                },
                if compact { 4 } else { 0 },
            );
            row.add_css_class("core-row");
            if compact {
                let label = text(
                    graph.label.strip_prefix("Core ").unwrap_or(&graph.label),
                    "core-label",
                );
                label.set_wrap(false);
                label.set_width_chars(7);
                label.set_tooltip_text(Some(&graph.label));
                row.append(&label);
            } else {
                let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                let (id, current) = plot::core_label_parts(&graph.label);
                let label = text(id, "core-label");
                label.set_wrap(false);
                label.set_width_chars(3);
                label.set_hexpand(true);
                label.set_tooltip_text(Some(&graph.label));
                let value = text(current, "core-label");
                value.set_wrap(false);
                value.set_width_chars(4);
                value.set_xalign(1.0);
                header.append(&label);
                header.append(&value);
                row.append(&header);
                if std::env::var_os("SIDESCOPE_VISUAL_GEOMETRY").is_some() {
                    assert!(label.hexpands() && !label.wraps() && !value.wraps());
                    assert_eq!(value.xalign(), 1.0, "core percentages must align right");
                    assert_eq!(value.width_chars(), 4, "reserve space through 100%");
                }
            }
            row.append(&history_plot(graph, layout.core_height(), true));
            if std::env::var_os("SIDESCOPE_VISUAL_GEOMETRY").is_some() {
                let height = row.measure(gtk::Orientation::Vertical, 128).1;
                assert!(
                    height <= layout.row_max_height(),
                    "core row density regressed: {height}"
                );
                if index == 0 {
                    eprintln!("CORE_ROW_GEOMETRY layout={layout:?} height={height}");
                }
            }
            let (column, line) = if screen_fit() {
                (
                    (index % FIT_CORE_COLUMNS.get()) as i32,
                    (index / FIT_CORE_COLUMNS.get()) as i32,
                )
            } else {
                plot::core_cell(index)
            };
            cores.attach(&row, column, line, 1, 1);
            index += 1;
        }
    }
    if index > 0 {
        body.append(&text("Logical cores · Y 0–100%", "metric-detail"));
        body.append(&cores);
    }
    body
}

fn weather_icon(code: Option<u16>, size: i32) -> gtk::DrawingArea {
    let icon = gtk::DrawingArea::new();
    icon.set_content_width(size);
    icon.set_content_height(size);
    icon.set_valign(gtk::Align::Center);
    icon.set_tooltip_text(Some(
        code.map(weather::conditions)
            .unwrap_or("Conditions unavailable"),
    ));
    // Own simple line glyphs: no dependency on a distribution's optional weather icons.
    icon.set_draw_func(move |_, cr, width, height| {
        use weather::Glyph;
        let kind = weather::glyph(code);
        cr.scale(f64::from(width) / 24.0, f64::from(height) / 24.0);
        source(cr, active_theme().plot.glyph);
        cr.set_line_width(active_theme().plot.glyph_width);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        if matches!(kind, Glyph::Sun | Glyph::PartlyCloudy) {
            let (x, y, r) = if kind == Glyph::Sun {
                (12.0, 12.0, 4.0)
            } else {
                (7.0, 7.0, 3.0)
            };
            cr.arc(x, y, r, 0.0, std::f64::consts::TAU);
            let _ = cr.stroke();
            for i in 0..8 {
                let angle = f64::from(i) * std::f64::consts::TAU / 8.0;
                cr.move_to(x + (r + 2.0) * angle.cos(), y + (r + 2.0) * angle.sin());
                cr.line_to(x + (r + 4.0) * angle.cos(), y + (r + 4.0) * angle.sin());
            }
            let _ = cr.stroke();
        }
        if !matches!(kind, Glyph::Sun | Glyph::Unknown) {
            cr.move_to(5.0, 16.0);
            cr.curve_to(0.0, 15.0, 2.0, 9.0, 6.0, 10.0);
            cr.curve_to(7.0, 3.0, 16.0, 4.0, 17.0, 10.0);
            cr.curve_to(23.0, 8.0, 25.0, 16.0, 19.0, 16.0);
            cr.close_path();
            let _ = cr.stroke();
        }
        match kind {
            Glyph::Rain => {
                for x in [7.0, 12.0, 17.0] {
                    cr.move_to(x, 19.0);
                    cr.line_to(x - 1.0, 22.0);
                }
            }
            Glyph::Snow => {
                for x in [7.0, 16.0] {
                    cr.move_to(x - 2.0, 20.0);
                    cr.line_to(x + 2.0, 20.0);
                    cr.move_to(x, 18.0);
                    cr.line_to(x, 22.0);
                }
            }
            Glyph::Fog => {
                for y in [19.0, 22.0] {
                    cr.move_to(4.0, y);
                    cr.line_to(20.0, y);
                }
            }
            Glyph::Storm => {
                cr.move_to(13.0, 17.0);
                cr.line_to(9.0, 20.0);
                cr.line_to(14.0, 20.0);
                cr.line_to(11.0, 23.0);
            }
            Glyph::Unknown => {
                cr.move_to(8.0, 8.0);
                cr.curve_to(8.0, 3.0, 18.0, 3.0, 16.0, 10.0);
                cr.line_to(12.0, 14.0);
                cr.move_to(12.0, 19.0);
                cr.line_to(12.0, 19.5);
            }
            _ => {}
        }
        let _ = cr.stroke();
    });
    icon
}

fn weather_view(forecast: &weather::WeatherDisplay) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let strip =
        !screen_fit() && active_theme().layout.weather == theme::WeatherLayout::ConditionStrip;
    body.add_css_class("weather-body");
    body.set_widget_name(if strip {
        "weather-strip"
    } else {
        "weather-curve"
    });
    let now = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    now.append(&weather_icon(Some(forecast.code), 28));
    now.append(&text(
        &weather::temperature_text(Some(forecast.temperature), forecast.units),
        "weather-temperature",
    ));
    if let Some(temp) = now
        .last_child()
        .and_then(|w| w.downcast::<gtk::Label>().ok())
    {
        temp.set_wrap(false);
    }
    let condition = text(weather::conditions(forecast.code), "metric-detail");
    condition.set_widget_name("weather-condition");
    condition.set_wrap(false);
    condition.set_ellipsize(gtk::pango::EllipsizeMode::End);
    condition.set_hexpand(true);
    condition.set_tooltip_text(Some(weather::conditions(forecast.code)));
    now.append(&condition);
    body.append(&now);

    let temperatures: Vec<_> = forecast.hours.iter().map(|h| h.temperature).collect();
    let range = plot::temperature_range(temperatures.iter().copied());
    let range_text = range
        .map(|(lo, hi)| {
            format!(
                "{}–{}",
                weather::temperature_text(Some(lo), forecast.units),
                weather::temperature_text(Some(hi), forecast.units)
            )
        })
        .unwrap_or_else(|| "Unavailable".into());
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let title = text(&format!("12 hours · {range_text}"), "metric-detail");
    title.set_hexpand(true);
    heading.append(&title);
    let details = gtk::MenuButton::new();
    details.set_label("Details");
    details.set_widget_name("weather-details");
    details.add_css_class("forecast-detail");
    let popover = gtk::Popover::new();
    details.set_popover(Some(&popover));
    heading.append(&details);
    body.append(&heading);
    let area = gtk::DrawingArea::new();
    area.set_content_height(40);
    area.set_hexpand(true);
    let description = forecast.hourly_description();
    // Keyboard-reachable text alternative preserves all twelve temperatures/conditions.
    popover.set_child(Some(&text(
        &format!(
            "Now · {} · {}\n{description}",
            weather::temperature_text(Some(forecast.temperature), forecast.units),
            weather::conditions(forecast.code)
        ),
        "hourly-details",
    )));
    area.set_tooltip_text(Some(&description));
    area.set_focusable(true);
    area.set_draw_func(move |_, cr, width, height| {
        cr.translate(3.0, 4.0);
        let w = f64::from((width - 6).max(0));
        let h = f64::from((height - 8).max(0));
        let style = plot::effective_style(active_theme().plot, w / 11.0, false);
        draw_grid(cr, w, h, true, style);
        stroke_points(
            cr,
            &plot::forecast_points(&temperatures, w, h),
            theme::Series::Weather,
            h,
            style,
        );
    });
    if !screen_fit() && strip {
        let conditions = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        conditions.set_homogeneous(true);
        for hour in &forecast.hours {
            let icon = weather_icon(hour.code, 16);
            icon.set_hexpand(true);
            icon.set_halign(gtk::Align::Center);
            icon.set_focusable(true);
            icon.set_tooltip_text(Some(&format!(
                "{} · {} · {}",
                hour.time,
                weather::temperature_text(hour.temperature, forecast.units),
                hour.code
                    .map(weather::conditions)
                    .unwrap_or("Conditions unavailable")
            )));
            conditions.append(&icon);
        }
        body.append(&conditions);
    } else if !screen_fit() {
        body.append(&area);
    }
    // Endpoint labels keep their natural width; no wrap/reflow or flex shrink on resize.
    let times = gtk::CenterBox::new();
    for (position, hour) in [0, 5, 11].into_iter().enumerate() {
        let label = text(&forecast.hours[hour].time[11..16], "axis-label");
        label.set_widget_name(&format!("weather-axis-{position}"));
        label.set_wrap(false);
        match position {
            0 => times.set_start_widget(Some(&label)),
            1 => times.set_center_widget(Some(&label)),
            _ => times.set_end_widget(Some(&label)),
        }
    }
    if !screen_fit() {
        body.append(&times);
    }
    body.append(&text("5 days · low / high", "metric-detail"));

    let bounds = plot::temperature_range(forecast.days.iter().flat_map(|day| [day.low, day.high]));
    let days = gtk::Box::new(
        if strip {
            gtk::Orientation::Horizontal
        } else {
            gtk::Orientation::Vertical
        },
        if strip { 4 } else { 3 },
    );
    days.set_homogeneous(strip);
    for day in &forecast.days {
        let row = gtk::Box::new(
            if strip {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            },
            if strip { 1 } else { 5 },
        );
        row.add_css_class("forecast-day");
        let date = glib::DateTime::from_iso8601(&format!("{}T00:00:00Z", day.date), None)
            .ok()
            .and_then(|d| d.format("%a %d").ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| day.date.clone());
        let label = text(
            if strip {
                date.split_whitespace().next().unwrap_or(&date)
            } else {
                &date
            },
            "day-label",
        );
        label.set_width_chars(if strip { 3 } else { 7 });
        if strip {
            label.set_xalign(0.5);
        }
        label.set_wrap(false);
        row.append(&label);
        let icon = weather_icon(day.code, 16);
        if strip {
            icon.set_halign(gtk::Align::Center);
        }
        row.append(&icon);
        let degrees = |value: Option<i32>| {
            value
                .map(|n| format!("{:.0}°", f64::from(n) / 10.0))
                .unwrap_or_else(|| "—".into())
        };
        for (value, is_high) in [(day.low, false), (day.high, true)] {
            if is_high {
                let area = gtk::DrawingArea::new();
                area.set_hexpand(true);
                area.set_content_height(if strip { 8 } else { 22 });
                let (low, high) = (day.low, day.high);
                area.set_draw_func(move |_, cr, width, height| {
                    let w = f64::from((width - 4).max(0));
                    let y = f64::from(height) / 2.0;
                    source(cr, active_theme().plot.range_track);
                    cr.set_line_width(3.0);
                    cr.move_to(2.0, y);
                    cr.line_to(w + 2.0, y);
                    let _ = cr.stroke();
                    if let Some((lo, hi)) = plot::daily_range(low, high, bounds, w) {
                        let style = active_theme().plot;
                        if style.range_low == style.range_high {
                            source(cr, style.range_low);
                        } else {
                            let gradient =
                                gtk::cairo::LinearGradient::new(lo + 2.0, y, hi + 2.0, y);
                            let a = style.range_low;
                            let b = style.range_high;
                            gradient.add_color_stop_rgb(0.0, a.0, a.1, a.2);
                            gradient.add_color_stop_rgb(1.0, b.0, b.1, b.2);
                            let _ = cr.set_source(&gradient);
                        }
                        cr.set_line_width(style.range_width);
                        cr.move_to(lo + 2.0, y);
                        cr.line_to(hi + 2.0, y);
                        let _ = cr.stroke();
                        if lo == hi {
                            cr.arc(lo + 2.0, y, 2.0, 0.0, std::f64::consts::TAU);
                            let _ = cr.fill();
                        }
                    }
                });
                row.append(&area);
            }
            let label = text(&degrees(value), "day-temperature");
            label.set_wrap(false);
            label.set_width_chars(4);
            label.set_xalign(if strip { 0.5 } else { 1.0 });
            row.append(&label);
        }
        let description = format!(
            "{} · {} · low {} / high {}; range on shared five-day scale",
            day.date,
            day.code
                .map(weather::conditions)
                .unwrap_or("Conditions unavailable"),
            weather::temperature_text(day.low, forecast.units),
            weather::temperature_text(day.high, forecast.units)
        );
        row.set_tooltip_text(Some(&description));
        row.set_focusable(true);
        days.append(&row);
    }
    body.append(&days);
    body
}

fn text(value: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.add_css_class(class);
    label.set_xalign(0.0);
    let compact = matches!(
        class,
        "section-title"
            | "section-count"
            | "card-kicker"
            | "state"
            | "footer-label"
            | "footer-right"
    );
    label.set_wrap(!compact);
    if !compact {
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    }
    if matches!(
        class,
        "card-title"
            | "card-subtitle"
            | "metric-label"
            | "metric-value"
            | "metric-detail"
            | "unavailable"
    ) {
        label.set_max_width_chars(25);
    }
    // Readings change every couple of seconds, and a glyph laid out at a
    // fractional position can land with its rows split across two pixel rows:
    // a 5's top bar came out as two half rows, and a 6's as a doubled one. Pin
    // every text widget to whole-pixel glyph positions so a redraw cannot
    // straddle a row, whatever the host's font settings say. Subpixel glyph
    // positioning is what puts a glyph between rows in the first place;
    // SIDESCOPE_ROUND_GLYPHS=off leaves it on to reproduce that.
    if std::env::var("SIDESCOPE_ROUND_GLYPHS").as_deref() != Ok("off") {
        label.pango_context().set_round_glyph_positions(true);
    }
    label
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use gtk::gdk::Key;

    #[test]
    fn sensor_retirement_count_is_visible_but_full_bounds_stay_in_info() {
        let mut card = sensors::demo_card();
        card.note.push_str("\nRetired 1 absent sensor · session\nAuto retention: 120 absent polls; max 32 absent channels; reconnect after retirement starts new history");
        for visible in [compact_card_note(&card), fit_card_note(&card)] {
            assert!(visible.contains("Retired 1 absent sensor"));
            assert!(!visible.contains("Auto retention:"));
        }
        assert!(
            card_context(&card)
                .contains("Auto retention: 120 absent polls; max 32 absent channels")
        );
    }

    #[test]
    fn sensor_info_states_degree_range_not_bytes_per_second() {
        let card = sensors::demo_card();
        assert_eq!(card.graphs.len(), 2);
        let context = card_context(&card);
        assert!(context.contains("Y 50–60 °C"), "{context}");
        assert!(context.contains("Y 45–55 °C"), "{context}");
        assert!(context.contains(&dashboard::history_window()));
        assert!(!context.contains("bytes/s"), "{context}");
    }

    #[test]
    fn sensor_plots_render_once_with_fans_text_only() {
        // Logic level (no display in unit tests): sensor_view pairs each
        // temperature metric with exactly one mini-plot, and the generic
        // full-width graph rows must stay off so those plots appear once.
        let card = sensors::demo_card();
        assert_eq!(card.metrics.len(), 4);
        assert_eq!(card.graphs.len(), 2);
        assert!(!shows_generic_graphs(&card));
        for metric in &card.metrics {
            let plots = card
                .graphs
                .iter()
                .filter(|g| g.label == metric.label && g.signed_samples.is_some())
                .count();
            if metric.value.contains("°C") {
                assert_eq!(
                    plots, 1,
                    "temperature channel lost its plot: {}",
                    metric.label
                );
            } else {
                assert_eq!(plots, 0, "fan gained a plot: {}", metric.label);
            }
        }
        // A sensors card without metrics never reaches sensor_view, so the
        // generic rows stay as its fallback.
        let mut empty = card.clone();
        empty.metrics.clear();
        assert!(shows_generic_graphs(&empty));
        // The other dedicated views: cpu owns its graphs, and weather
        // carries none, so neither can double-render.
        let cards = dashboard::demo_cards();
        let cpu = cards.iter().find(|c| c.id == "cpu").unwrap();
        assert!(!shows_generic_graphs(cpu));
        for weather in weather::demo_cards() {
            assert!(weather.graphs.is_empty(), "weather card carries graphs");
        }
        let mut generic = dashboard::card("codex", "Codex", "", vec![]);
        generic.graphs = card.graphs.clone();
        assert!(shows_generic_graphs(&generic));
    }

    #[test]
    fn sensor_fixture_and_states_keep_every_reading_and_context() {
        for scenario in [
            "fit-sensors-zero",
            "fit-sensors-missing",
            "fit-sensors-error",
            "fit-sensors-recovery",
        ] {
            let mut data = DashboardData {
                local: dashboard::demo_cards(),
                ..Default::default()
            };
            visual_scenario(&mut data, scenario);
            let card = data.local.iter().find(|c| c.id == "sensors").unwrap();
            assert_eq!(card.metrics.len(), 4);
            assert!(
                card.metrics.iter().any(|m| m.value.contains("0 RPM"))
                    || scenario == "fit-sensors-error"
            );
            for m in &card.metrics {
                assert!(card_context(card).contains(&m.value));
                assert!(card_context(card).contains(&m.label));
                assert!(card_context(card).contains(&m.detail));
            }
            match scenario {
                "fit-sensors-zero" => assert_eq!(card.state, CardState::Ready),
                "fit-sensors-recovery" => {
                    assert_eq!(card.state, CardState::Ready);
                    assert!(card.note.contains("Retired 1 absent sensor"));
                    assert_eq!(card.metrics[0].value, "55.0 °C");
                    assert!(card.metrics[0].detail.contains("1 / 120 raw 2s slots"));
                }
                "fit-sensors-missing" => {
                    assert_eq!(card.state, CardState::Stale);
                    assert!(card.metrics[0].detail.starts_with("STALE"));
                }
                _ => {
                    assert!(matches!(card.state, CardState::Unavailable(_)));
                    assert!(
                        card.metrics
                            .iter()
                            .all(|m| m.value == "—" && m.detail.starts_with("UNAVAILABLE"))
                    );
                }
            }
        }
    }

    #[test]
    fn screen_fit_moves_reset_context_into_the_pace_legend() {
        // Comfortable keeps a line under provider rows; Screen-fit does not,
        // because the legend carries the reset window there.
        assert!(shows_metric_detail("codex", false, "Resets in 2d 22h"));
        assert!(!shows_metric_detail("codex", true, "Resets in 2d 22h"));
        // Pace arithmetic the legend already states stays in Info when screened.
        assert!(shows_metric_detail(
            "codex",
            false,
            "57% elapsed · 1pts ahead"
        ));
        assert!(!shows_metric_detail(
            "codex",
            true,
            "57% elapsed · 1pts ahead"
        ));
        // Telemetry support text is compacted in either density.
        assert!(!shows_metric_detail("ram", false, "43% used"));
        assert!(!shows_metric_detail("ram", true, "43% used"));
        // Swap's explicit absence is never hidden.
        assert!(shows_metric_detail("ram", false, "No swap configured"));
        assert!(shows_metric_detail("ram", true, "No swap configured"));
        assert!(!shows_metric_detail("codex", true, ""));
    }

    #[test]
    fn screen_fit_info_retains_graph_readings_and_scales() {
        let cards = dashboard::demo_cards();
        for id in ["ram", "network", "cpu"] {
            let card = cards.iter().find(|c| c.id == id).unwrap();
            let context = card_context(card);
            for graph in &card.graphs {
                assert!(
                    context.contains(&graph.label),
                    "Info omitted graph reading: {}",
                    graph.label
                );
                let scale = if graph.ceiling == 100 {
                    "Y 0–100%".into()
                } else {
                    format!("Y 0–{} bytes/s (auto)", graph.ceiling)
                };
                assert!(context.contains(&scale), "Info omitted scale: {scale}");
                assert!(
                    context.contains(&format!("{} window", dashboard::history_window())),
                    "Info omitted the trend window"
                );
            }
        }
        let mut no_graphs = cards[1].clone();
        no_graphs.graphs.clear();
        assert!(!card_context(&no_graphs).contains("Y 0–100%"));
    }

    #[test]
    fn density_shortcut_is_independent_and_context_remains_complete() {
        assert_eq!(
            shortcut_action(Key::d, true, false),
            Some(Shortcut::ToggleDensity)
        );
        assert_eq!(
            shortcut_action(Key::D, false, false),
            Some(Shortcut::ToggleDensity)
        );
        assert_eq!(shortcut_action(Key::d, true, true), None);
        let mut c = dashboard::busy_cpu_demo();
        c.note = "Linux · 2s · history\nSampled just now\nREFRESH FAILED · retained".into();
        assert_eq!(
            fit_card_note(&c),
            "Sampled just now\nREFRESH FAILED · retained"
        );
        assert!(card_context(&c).contains("12 logical cores"));
        let mut stale = dashboard::card(
            "test",
            "Test",
            "",
            vec![dashboard::metric(
                "Weekly",
                "90%".into(),
                "Historical source detail",
            )],
        );
        stale.metrics[0].elapsed_pct = Some(20);
        stale.metrics[0].pace = Some(Pace { delta: 70 });
        stale.state = CardState::Stale;
        // A stale reading keeps the window's time position and loses only the
        // verdict it can no longer support.
        let context = card_context(&stale);
        assert!(!context.contains("OVER PACE"));
        assert!(context.contains("time 20% elapsed"));
        let config =
            sidescope::parse_config("density = 'screen-fit'\nshow_agy_gpt_oss = true").unwrap();
        let cards = visible_cards(
            include_str!("../tests/fixtures/screen-fit-usage.json"),
            &config,
        )
        .unwrap();
        assert_eq!(cards.len(), 6);
        assert!(cards.iter().all(|c| c.state == CardState::Ready));
        assert_eq!(cards[5].metrics.len(), 4);
        for c in cards {
            for m in &c.metrics {
                assert!(card_context(&c).contains(&m.value));
                assert!(card_context(&c).contains(&m.label));
            }
        }
    }

    #[test]
    fn remote_failures_retain_values_and_attribution_without_accumulating_errors() {
        let mut previous = weather::demo_cards();
        let values = previous[0].weather.clone();
        for _ in 0..10 {
            merge_remote(
                &mut previous,
                "weather",
                "Weather",
                Err("network unavailable".into()),
            );
        }
        assert_eq!(previous[0].state, CardState::Stale);
        assert_eq!(previous[0].weather, values);
        assert!(previous[0].note.contains("Open-Meteo"));
        assert_eq!(previous[0].note.matches("REFRESH FAILED").count(), 1);
        assert_eq!(previous[1].state, CardState::Ready);
        merge_remote(
            &mut previous,
            "weather",
            "Weather",
            Ok(weather::demo_cards().remove(0)),
        );
        assert_eq!(previous[0].state, CardState::Ready);
        assert!(!previous[0].note.contains("REFRESH FAILED"));
        let mut absent = vec![];
        merge_remote(&mut absent, "weather", "Weather", Err("offline".into()));
        assert!(matches!(absent[0].state, CardState::Unavailable(_)));
    }

    #[test]
    fn compact_balance_notes_keep_source_timestamps_and_full_tooltip() {
        let mut card = dashboard::card("openrouter", "OpenRouter", "", vec![]);
        card.note = "Daily spend history unavailable: no dated provider history in this report.\nSource fetched 24 min ago".into();
        assert_eq!(
            compact_card_note(&card),
            "Daily spend history unavailable\nSource fetched 24 min ago"
        );
        assert!(card.note.contains("no dated provider history"));
    }

    #[test]
    fn key_tier_context_stays_in_info_not_on_the_face() {
        let mut card = dashboard::card("openrouter", "OpenRouter", "", vec![]);
        card.note =
            "Daily spend history unavailable: no dated provider history in this report.\nKey tier: paid tier\nRecent models: anthropic/claude\nSource fetched 24 min ago"
                .into();
        for visible in [compact_card_note(&card), fit_card_note(&card)] {
            assert!(!visible.contains("Key tier:"));
            assert!(!visible.contains("Recent models:"));
            assert!(visible.contains("Source fetched 24 min ago"));
        }
        // Info keeps the full note, so the context is still reachable.
        assert!(card_context(&card).contains("Key tier: paid tier"));
        assert!(card_context(&card).contains("Recent models: anthropic/claude"));
    }

    #[test]
    fn synthetic_visual_scenarios_cover_stable_history_and_remote_states() {
        let data = || DashboardData {
            local: dashboard::demo_cards(),
            remote: weather::demo_cards(),
            ..Default::default()
        };
        for (scenario, slots) in [
            ("one", 1),
            ("two", 2),
            ("full", 120),
            ("missing", 60),
            ("strip-missing", 60),
        ] {
            let mut d = data();
            visual_scenario(&mut d, scenario);
            assert_eq!(d.local[0].graphs[0].samples.len(), slots);
        }
        let (mut full, mut rolling) = (data(), data());
        visual_scenario(&mut full, "full");
        visual_scenario(&mut rolling, "rolling");
        assert_eq!(
            &full.local[0].graphs[0].samples[1..],
            &rolling.local[0].graphs[0].samples[..119]
        );
        let mut missing = data();
        visual_scenario(&mut missing, "missing");
        assert_eq!(missing.local[0].graphs[0].samples[20], None);
        assert_eq!(
            missing.remote[0].weather.as_ref().unwrap().hours[3].temperature,
            None
        );
        let mut fahrenheit = data();
        visual_scenario(&mut fahrenheit, "fahrenheit");
        assert_eq!(
            fahrenheit.remote[0].weather.as_ref().unwrap().units,
            weather::Units::Fahrenheit
        );
        let mut high = data();
        visual_scenario(&mut high, "high-cores");
        assert_eq!(high.local[0].graphs.len(), 65);
        let mut stale = data();
        visual_scenario(&mut stale, "stale");
        assert!(
            stale
                .local
                .iter()
                .chain(&stale.remote)
                .all(|c| c.state == CardState::Stale)
        );
        let mut strip = data();
        visual_scenario(&mut strip, "strip-fahrenheit");
        assert_eq!(
            strip.remote[0].weather.as_ref().unwrap().units,
            weather::Units::Fahrenheit
        );
        visual_scenario(&mut strip, "strip-live-details");
        let mut unconfigured = data();
        visual_scenario(&mut unconfigured, "unconfigured");
        assert!(
            unconfigured
                .remote
                .iter()
                .all(|c| matches!(c.state, CardState::Unavailable(_)) && c.weather.is_none())
        );
    }

    #[test]
    fn responsive_bin_declares_a_minimum_and_remote_refresh_is_conservative() {
        assert_eq!(weather::REFRESH_SECS, 1800);
        let source = include_str!("main.rs");
        assert!(source.contains("responsive.set_size_request(320, 200)"));
    }

    #[test]
    fn compositor_focus_uses_frame_color_without_a_status_badge() {
        assert!(include_str!("style.css").contains(".stripe.window-focused .stripe-header"));
        assert!(include_str!("style.css").contains(".stripe.window-focused .footer"));
        assert!(!include_str!("style.css").contains(".focus-status"));
    }

    #[test]
    fn metric_font_override_accepts_only_a_plain_family() {
        assert_eq!(metric_font_name("DejaVu Sans"), Some("DejaVu Sans"));
        assert_eq!(metric_font_name("  Fira Sans  "), Some("Fira Sans"));
        assert_eq!(metric_font_name("Noto Sans Mono"), Some("Noto Sans Mono"));
        assert_eq!(metric_font_name(""), None);
        assert_eq!(metric_font_name("   "), None);
        assert_eq!(metric_font_name("Sans; } .x { color: red"), None);
        assert_eq!(metric_font_name("a\"b"), None);
    }

    #[test]
    fn the_window_names_the_build_it_runs() {
        let label = build_label();
        assert!(label.starts_with('v'), "{label}");
        assert!(label.contains(env!("CARGO_PKG_VERSION")), "{label}");
        assert!(label.contains(BUILD_REVISION), "{label}");
        assert!(build_tooltip().contains(BUILD_REVISION));
        let hash = BUILD_REVISION
            .strip_suffix("-dirty")
            .unwrap_or(BUILD_REVISION);
        assert!(
            hash == "unknown" || (hash.len() >= 7 && hash.chars().all(|c| c.is_ascii_hexdigit())),
            "unexpected revision {BUILD_REVISION}"
        );
    }

    #[test]
    fn keyboard_shortcuts_respect_sample_mode_and_help_state() {
        assert_eq!(
            shortcut_action(Key::question, false, false),
            Some(Shortcut::ToggleHelp)
        );
        assert_eq!(
            shortcut_action(Key::question, true, true),
            Some(Shortcut::ToggleHelp)
        );
        assert_eq!(
            shortcut_action(Key::Escape, false, true),
            Some(Shortcut::CloseHelp)
        );
        assert_eq!(shortcut_action(Key::Escape, false, false), None);
        assert_eq!(
            shortcut_action(Key::r, false, false),
            Some(Shortcut::Refresh)
        );
        assert_eq!(shortcut_action(Key::r, true, false), None);
        assert_eq!(shortcut_action(Key::q, true, false), None);
        assert_eq!(shortcut_action(Key::Q, false, false), None);
        assert_eq!(
            shortcut_action(Key::j, true, false),
            Some(Shortcut::ScrollDown)
        );
        assert_eq!(
            shortcut_action(Key::k, false, false),
            Some(Shortcut::ScrollUp)
        );
        assert_eq!(
            shortcut_action(Key::Page_Down, false, false),
            Some(Shortcut::ScrollPageDown)
        );
        assert_eq!(
            shortcut_action(Key::Page_Up, false, false),
            Some(Shortcut::ScrollPageUp)
        );
        assert_eq!(shortcut_action(Key::j, true, true), None);
    }

    #[test]
    fn theme_shortcuts_cycle_only_outside_the_help_overlay() {
        for demo in [true, false] {
            assert_eq!(
                shortcut_action(gtk::gdk::Key::bracketleft, demo, false),
                Some(Shortcut::PreviousTheme)
            );
            assert_eq!(
                shortcut_action(gtk::gdk::Key::bracketright, demo, false),
                Some(Shortcut::NextTheme)
            );
            assert_eq!(
                shortcut_action(gtk::gdk::Key::bracketright, demo, true),
                None
            );
        }
    }

    #[test]
    fn report_check_status_describes_the_request_not_provider_freshness() {
        assert_eq!(report_check_text(ReportCheck::Checking), "CHECKING REPORT…");
        assert_eq!(
            report_check_text(ReportCheck::Checked("12:34:56")),
            "REPORT CHECKED 12:34:56"
        );
        assert_eq!(
            report_check_text(ReportCheck::Failed),
            "CHECK FAILED · SEE ERROR ABOVE"
        );
    }

    #[test]
    fn time_marker_is_wide_and_visible_even_at_bar_edges() {
        assert_eq!(pace_tick_x(100, 0), PACE_TICK_OUTLINE_WIDTH / 2.0);
        assert_eq!(pace_tick_x(100, 100), 100.0 - PACE_TICK_OUTLINE_WIDTH / 2.0);
        assert!(pace_tick_x(100, 58) > pace_tick_x(100, 57));
    }

    #[test]
    fn pace_labels_state_direction_without_relying_on_color() {
        assert_eq!(pace_label(Pace { delta: 1 }), "+1 pt OVER PACE");
        assert_eq!(pace_label(Pace { delta: -20 }), "20 pts UNDER PACE");
        assert_eq!(pace_label(Pace { delta: 0 }), "ON PACE");
        assert_eq!(pace_class(Pace { delta: 1 }), "pace-over");
        assert_eq!(pace_class(Pace { delta: -1 }), "pace-under");
        assert_eq!(pace_class(Pace { delta: 0 }), "pace-even");
    }

    #[test]
    fn stale_quota_keeps_its_time_mark_and_loses_only_the_verdict() {
        let mut metric = dashboard::metric("Weekly", "54%".into(), "Resets in 3d 20h");
        metric.elapsed_pct = Some(58);
        metric.pace = Some(Pace { delta: -4 });
        assert_eq!(
            quota_marks(&CardState::Ready, &metric),
            QuotaMarks {
                tick: Some(58),
                verdict: Some(Pace { delta: -4 })
            }
        );
        // A stale or failed source has no current reading to compare with the
        // window, so the verdict goes and the time mark stays.
        for state in [
            CardState::Stale,
            CardState::Unavailable("Local source unavailable".into()),
        ] {
            assert_eq!(
                quota_marks(&state, &metric),
                QuotaMarks {
                    tick: Some(58),
                    verdict: None
                }
            );
        }
        // Without a stated window there is no time position to mark, and so
        // nothing for a verdict to be measured against.
        let mut unwindowed = dashboard::metric("Weekly", "54%".into(), "Previously reported");
        unwindowed.pace = Some(Pace { delta: -4 });
        assert_eq!(
            quota_marks(&CardState::Ready, &unwindowed),
            QuotaMarks {
                tick: None,
                verdict: None
            }
        );
    }

    #[test]
    fn scroll_target_stays_within_the_available_page() {
        assert_eq!(scroll_target(0.0, 120.0, 0.0, 900.0, 500.0), 120.0);
        assert_eq!(scroll_target(380.0, 120.0, 0.0, 900.0, 500.0), 400.0);
        assert_eq!(scroll_target(5.0, -120.0, 0.0, 900.0, 500.0), 0.0);
    }

    #[test]
    fn automatic_refresh_is_bounded_and_never_overlaps() {
        assert_eq!(REFRESH_INTERVAL_SECS, 300);
        assert!(allow_refresh(false, true));
        assert!(!allow_refresh(true, true));
        assert!(!allow_refresh(false, false));
    }
}
