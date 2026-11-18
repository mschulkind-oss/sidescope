//! Read-only Linux hwmon measurements. Enumeration numbers are never identities.
use crate::{Card, CardState, Graph, dashboard};
use serde::Deserialize;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorConfig {
    /// When present, replaces automatic selection (an empty list selects none).
    pub channels: Option<Vec<Selector>>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    pub chip: String,
    /// Canonical /sys/devices path, without the transient /hwmon/hwmonN suffix.
    pub device: String,
    pub channel: String,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identity {
    pub chip: String,
    pub device: String,
    pub channel: String,
}
impl Selector {
    fn identity(&self) -> Identity {
        Identity {
            chip: self.chip.clone(),
            device: self.device.clone(),
            channel: self.channel.clone(),
        }
    }
}
fn temperature(channel: &str) -> bool {
    channel.starts_with("temp")
}
fn valid_channel(channel: &str) -> bool {
    ["temp", "fan"].iter().any(|prefix| {
        channel.strip_prefix(prefix).is_some_and(|n| {
            !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && n.parse::<u32>().is_ok_and(|v| v > 0)
                && !n.starts_with('0')
        })
    })
}
impl SensorConfig {
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for s in self.channels.iter().flatten() {
            if s.chip.trim().is_empty()
                || s.chip.trim() != s.chip
                || s.chip.contains('/')
                || s.chip.chars().any(char::is_control)
                || !valid_channel(&s.channel)
                || s.label.trim().is_empty()
                || s.label.len() > 80
                || s.label.chars().any(char::is_control)
                || !s.device.starts_with("/sys/devices/")
                || s.device.ends_with('/')
                || s.device.contains("//")
                || s.device.chars().any(char::is_control)
                || s.device
                    .split('/')
                    .any(|p| p == ".." || p == "." || p.starts_with("hwmon"))
            {
                return Err("Invalid sensor selector: use chip, canonical /sys/devices device (no hwmonN), tempN/fanN channel and nonempty label".into());
            }
            if !seen.insert(s.identity()) {
                return Err("Duplicate sensor selector".into());
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct Channel {
    pub identity: Identity,
    pub label: Option<String>,
    path: PathBuf,
}

/// The device symlink is preferable; hwmon devices without one live directly
/// beneath their device. Strip only the final transient hwmon suffix.
fn device_path(path: &Path) -> Result<PathBuf, String> {
    if let Ok(device) = std::fs::canonicalize(path.join("device")) {
        return Ok(device);
    }
    let canonical = std::fs::canonicalize(path).map_err(io_reason)?;
    let parent = canonical
        .parent()
        .ok_or("Sensor device identity unavailable")?;
    if parent.file_name().is_some_and(|n| n == "hwmon") {
        return parent
            .parent()
            .map(Path::to_path_buf)
            .ok_or("Sensor device identity unavailable".into());
    }
    Err("Sensor device identity unavailable".into())
}
fn io_reason(error: std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => "Permission denied".into(),
        std::io::ErrorKind::NotFound => "Source missing".into(),
        _ => "Source read failed".into(),
    }
}
pub fn discover(root: &Path) -> Result<Vec<Channel>, String> {
    let mut channels = Vec::new();
    for entry in std::fs::read_dir(root).map_err(io_reason)? {
        let path = entry.map_err(io_reason)?.path();
        let chip = match std::fs::read_to_string(path.join("name")) {
            Ok(chip) if !chip.trim().is_empty() => chip.trim().to_string(),
            _ => continue,
        };
        let Ok(device) = device_path(&path) else {
            continue;
        };
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(channel) = name.strip_suffix("_input").filter(|c| valid_channel(c)) else {
                continue;
            };
            let label = std::fs::read_to_string(path.join(format!("{channel}_label")))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            channels.push(Channel {
                identity: Identity {
                    chip: chip.clone(),
                    device: device.to_string_lossy().into_owned(),
                    channel: channel.into(),
                },
                label,
                path: path.clone(),
            });
        }
    }
    channels.sort_by(|a, b| a.identity.cmp(&b.identity));
    Ok(channels)
}
fn automatic_label(channel: &Channel) -> Option<String> {
    let id = &channel.identity;
    let label = channel.label.as_deref()?;
    if id.channel.starts_with("fan") {
        return Some(format!("{} · {label}", id.chip));
    }
    if matches!(id.chip.as_str(), "coretemp" | "k10temp" | "zenpower")
        && (label.starts_with("Package id ") || matches!(label, "Tctl" | "Tdie"))
    {
        return Some(format!("CPU · {label}"));
    }
    if matches!(id.chip.as_str(), "amdgpu" | "nouveau") && label.eq_ignore_ascii_case("edge") {
        return Some("GPU · edge".into());
    }
    None
}
fn parse_value(channel: &str, source: &str) -> Result<i64, String> {
    let value = source
        .trim()
        .parse::<i64>()
        .map_err(|_| "Malformed reading")?;
    if !temperature(channel) && value < 0 {
        return Err("Invalid negative RPM".into());
    }
    Ok(value)
}
fn read_channel_with(
    channel: &Channel,
    read: impl Fn(&Path) -> std::io::Result<String>,
) -> Result<i64, String> {
    for (suffix, invalid) in [("fault", 1), ("enable", 0)] {
        match read(
            &channel
                .path
                .join(format!("{}_{suffix}", channel.identity.channel)),
        ) {
            Ok(source) => {
                let value = source
                    .trim()
                    .parse::<u8>()
                    .map_err(|_| "Malformed sensor flag")?;
                if value > 1 {
                    return Err("Malformed sensor flag".into());
                }
                if value == invalid {
                    return Err(if suffix == "fault" {
                        "Sensor fault"
                    } else {
                        "Sensor disabled"
                    }
                    .into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_reason(e)),
        }
    }
    read(
        &channel
            .path
            .join(format!("{}_input", channel.identity.channel)),
    )
    .map_err(io_reason)
    .and_then(|s| parse_value(&channel.identity.channel, &s))
}
fn display_value(channel: &str, value: i64) -> String {
    if temperature(channel) {
        format!("{:.1} °C", value as f64 / 1000.0)
    } else if value == 0 {
        "0 RPM · stopped".into()
    } else {
        format!("{value} RPM")
    }
}
/// Automatic stale rows survive short hotplug, not unbounded device churn.
pub const AUTO_ABSENT_POLLS: usize = dashboard::HISTORY_LIMIT;
pub const AUTO_ABSENT_LIMIT: usize = 32;
pub const AUTO_RETENTION_CONTEXT: &str = "Auto retention: 120 consecutive absent polls (nominal 4 min); max 32 absent channels, oldest absence first with identity tie-break. Present but failing channels and explicit selectors never expire. Reconnect after retirement starts new history.";

#[derive(Default)]
struct Reading {
    label: String,
    last: Option<(i64, i64)>, // (value, unix seconds of last success)
    history: VecDeque<Option<i64>>,
    absent_polls: usize,
    /// Held temperature Y range in millidegrees: initialized from history
    /// via temperature_autorange on the first ranged sample, expanded
    /// outward to 5 °C steps when a later sample falls outside, never
    /// shrunk — so early extremes keep the scale after window eviction.
    range: Option<(i64, i64)>,
}

/// Expand a held temperature range outward to 5 °C steps for a new sample,
/// keeping the minimum 10 °C span. In-range samples leave it unchanged.
fn expand_range(held: (i64, i64), value: i64) -> (i64, i64) {
    const STEP: i64 = 5000;
    const MIN_SPAN: i64 = 10_000;
    let (mut floor, mut ceiling) = held;
    if value < floor {
        floor = value.div_euclid(STEP) * STEP;
    } else if value > ceiling {
        ceiling = value.div_euclid(STEP) * STEP;
        if value.rem_euclid(STEP) != 0 {
            ceiling += STEP;
        }
    }
    while ceiling - floor < MIN_SPAN {
        ceiling += STEP;
    }
    (floor, ceiling)
}
#[derive(Default)]
pub struct Sampler {
    readings: BTreeMap<Identity, Reading>,
    retired_auto: usize,
}
impl Sampler {
    fn retire_absent(&mut self, available: &BTreeMap<Identity, Vec<&Channel>>) {
        let before = self.readings.len();
        for (id, reading) in &mut self.readings {
            reading.absent_polls = if available.contains_key(id) {
                0
            } else {
                reading.absent_polls.saturating_add(1)
            };
        }
        self.readings
            .retain(|_, r| r.absent_polls < AUTO_ABSENT_POLLS);
        let mut absent: Vec<_> = self
            .readings
            .iter()
            .filter(|(_, r)| r.absent_polls > 0)
            .map(|(id, r)| (id.clone(), r.absent_polls))
            .collect();
        absent.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let excess = absent.len().saturating_sub(AUTO_ABSENT_LIMIT);
        for (id, _) in absent.into_iter().take(excess) {
            self.readings.remove(&id);
        }
        self.retired_auto = self
            .retired_auto
            .saturating_add(before - self.readings.len());
    }

    pub fn sample(&mut self, root: &Path, config: &SensorConfig, now_unix: i64) -> Card {
        if config.channels.as_ref().is_some_and(Vec::is_empty) {
            self.readings.clear();
            return dashboard::unavailable("sensors", "Sensors", "No sensor channels selected");
        }
        let discovered = discover(root);
        let mut available: BTreeMap<Identity, Vec<&Channel>> = BTreeMap::new();
        if let Ok(channels) = &discovered {
            for c in channels {
                available.entry(c.identity.clone()).or_default().push(c);
            }
        }
        if let Some(selectors) = &config.channels {
            self.readings
                .retain(|id, _| selectors.iter().any(|s| s.identity() == *id));
            for s in selectors {
                let reading = self.readings.entry(s.identity()).or_default();
                reading.label = s.label.clone();
                reading.absent_polls = 0;
            }
        } else {
            for channels in available.values() {
                if let Some(label) = automatic_label(channels[0]) {
                    self.readings
                        .entry(channels[0].identity.clone())
                        .or_default()
                        .label = label;
                }
            }
            self.retire_absent(&available);
        }
        let mut card = dashboard::card(
            "sensors",
            "Sensors",
            "Hardware temperatures and fans",
            vec![],
        );
        let mut failed = false;
        for (id, reading) in &mut self.readings {
            let result = match available.get(id) {
                Some(channels) if channels.len() == 1 => {
                    read_channel_with(channels[0], |p| std::fs::read_to_string(p))
                }
                Some(_) => Err("Ambiguous duplicate identity".into()),
                None => Err(discovered
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_else(|| "Source missing".into())),
            };
            if reading.history.len() == dashboard::HISTORY_LIMIT {
                reading.history.pop_front();
            }
            reading.history.push_back(result.as_ref().ok().copied());
            let sample = result.as_ref().ok().copied();
            let status = match result {
                Ok(value) => {
                    reading.last = Some((value, now_unix));
                    "Sampled just now".to_owned()
                }
                Err(error) => {
                    failed = true;
                    format!(
                        "{} · {error}",
                        if reading.last.is_some() {
                            "STALE"
                        } else {
                            "UNAVAILABLE"
                        }
                    )
                }
            };
            let (value, last) = reading
                .last
                .as_ref()
                .map(|(value, last)| {
                    (
                        display_value(&id.channel, *value),
                        format!("Last success {}", crate::freshness(now_unix, *last)),
                    )
                })
                .unwrap_or_else(|| ("—".into(), "No successful sample".into()));
            card.metrics.push(dashboard::metric(&reading.label, value, &format!("{status}\n{} · {} · {}\n{last}\nSession history: {} / 120 raw 2s slots, {} gaps; unit {}", id.chip, id.device, id.channel, reading.history.len(), reading.history.iter().filter(|v| v.is_none()).count(), if temperature(&id.channel) { "millidegrees Celsius" } else { "RPM" })));
            // Temperature channels carry a held auto-ranged mini plot aligned
            // with their reading row; fans stay text-only. The held range
            // only ever expands, so evicting an early extreme from the
            // 120-slot window cannot rescale the Y axis mid-session.
            if temperature(&id.channel) {
                if reading.range.is_none() {
                    reading.range =
                        crate::plot::temperature_autorange(reading.history.iter().copied());
                } else if let Some(value) = sample {
                    reading.range = Some(expand_range(reading.range.unwrap(), value));
                }
            }
            if temperature(&id.channel)
                && let Some((floor, ceiling)) = reading.range
            {
                card.graphs.push(Graph {
                    label: reading.label.clone(),
                    samples: Vec::new(),
                    ceiling: 0,
                    signed_samples: Some(reading.history.iter().copied().collect()),
                    range: Some((floor, ceiling)),
                    unit: Some("°C".into()),
                });
            }
        }
        card.note = format!("hwmon · Sampled just now · {}s", dashboard::SAMPLE_SECS);
        if failed {
            card.state = if self.readings.values().any(|r| r.last.is_some()) {
                CardState::Stale
            } else {
                CardState::Unavailable("Selected sensors unavailable".into())
            };
            card.note.push_str("\nSome sensors unavailable · per-channel last success retained; missing history slots are gaps");
        }
        if card.metrics.is_empty() {
            card.state = CardState::Unavailable(discovered.err().unwrap_or_else(|| "No default labeled CPU/GPU temperatures or fans exposed; configure explicit sensors.channels".into()));
        }
        if self.retired_auto > 0 {
            card.note.push_str(&format!(
                "\nRetired {} absent sensor{} · session",
                self.retired_auto,
                if self.retired_auto == 1 { "" } else { "s" }
            ));
        }
        if config.channels.is_none() || self.retired_auto > 0 {
            card.note.push_str(&format!("\n{AUTO_RETENTION_CONTEXT}"));
        }
        card
    }
}

/// Deterministic synthetic trace around a raw millidegree reading: a ±0.8 °C
/// sawtooth keyed only by the raw value, so demo captures show a temperature
/// trace while demo output stays exactly reproducible (no clock, no randomness).
fn demo_history(raw: i64) -> Vec<Option<i64>> {
    (0..60)
        .map(|i| Some(raw + ((i as i64 * 37 + raw.rem_euclid(1000)) % 1600) - 800))
        .collect()
}

/// Entirely synthetic; never generated from the running machine.
pub fn demo_card() -> Card {
    #[derive(Deserialize)]
    struct Fixture {
        chip: String,
        channel: String,
        label: String,
        raw: i64,
    }
    let fixtures: Vec<Fixture> =
        serde_json::from_str(include_str!("../tests/fixtures/sensors.json"))
            .expect("synthetic sensor fixture");
    let mut graphs = Vec::new();
    let metrics = fixtures
        .into_iter()
        .map(|f| {
            if temperature(&f.channel) {
                let history = demo_history(f.raw);
                if let Some((floor, ceiling)) =
                    crate::plot::temperature_autorange(history.iter().copied())
                {
                    graphs.push(Graph {
                        label: f.label.clone(),
                        samples: Vec::new(),
                        ceiling: 0,
                        signed_samples: Some(history),
                        range: Some((floor, ceiling)),
                        unit: Some("°C".into()),
                    });
                }
            }
            dashboard::metric(
                &f.label,
                display_value(&f.channel, f.raw),
                &format!("Synthetic {} · {} · Sampled just now", f.chip, f.channel),
            )
        })
        .collect();
    let mut card = dashboard::card(
        "sensors",
        "Sensors",
        "Synthetic hardware temperatures and fans",
        metrics,
    );
    card.graphs = graphs;
    card.note = "hwmon · synthetic · 2s".into();
    card
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    /// Test clock: consecutive samples a minute apart, so "last success" text
    /// is deterministic without sleeping or depending on the wall clock.
    const T0: i64 = 1_790_000_000;
    const T1: i64 = T0 + 60;
    const T2: i64 = T0 + 120;
    const T3: i64 = T0 + 180;
    struct Tree(PathBuf);
    impl Tree {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "stripe-sensors-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(path.join("class")).unwrap();
            Self(path)
        }
        fn chip(&self, n: usize, device: &str, chip: &str) -> PathBuf {
            let p = self.0.join(format!("devices/{device}/hwmon/hwmon{n}"));
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("name"), chip).unwrap();
            symlink(&p, self.0.join(format!("class/hwmon{n}"))).unwrap();
            p
        }
        fn root(&self) -> PathBuf {
            self.0.join("class")
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn put(path: &Path, name: &str, value: &str) {
        std::fs::write(path.join(name), value).unwrap();
    }
    #[test]
    fn sensor_config_is_closed_and_rejects_transient_duplicate_and_unsafe_selectors() {
        let valid = "[sensors]\nchannels = [{chip='coretemp',device='/sys/devices/platform/coretemp.0',channel='temp1',label='CPU package'}]";
        assert!(crate::parse_config(valid).is_ok());
        for source in [
            "[sensors]\nunknown = true",
            "[sensors]\nchannels = [{chip='x',device='/sys/devices/a/hwmon/hwmon2',channel='temp1',label='a'}]",
            "[sensors]\nchannels = [{chip='x',device='/sys/devices/a',channel='pwm1',label='a'}]",
            "[sensors]\nchannels = [{chip='x',device='/sys/devices/a',channel='fan+1',label='a'}]",
            "[sensors]\nchannels = [{chip='x',device='/sys/devices/a',channel='fan01',label='a'}]",
            "[sensors]\nchannels = [{chip='x',device='/sys/devices/a',channel='fan1',label='a',threshold=10}]",
            "[sensors]\nchannels = [{chip='x',device='/sys/devices/a',channel='fan1',label=''}]",
            "[sensors]\nchannels = [{chip='x',device='/sys/devices/../a',channel='temp1',label='a'}]",
        ] {
            assert!(crate::parse_config(source).is_err(), "{source}");
        }
        let duplicate = valid.replace("}]", "},{chip='coretemp',device='/sys/devices/platform/coretemp.0',channel='temp1',label='Other'}]");
        assert!(crate::parse_config(&duplicate).is_err());
        assert!(crate::parse_config("[sensors]\nchannels=[]").is_ok());
    }
    #[test]
    fn empty_selection_and_missing_hwmon_have_intentional_distinct_states() {
        let missing = Path::new("/definitely/missing/hwmon");
        let mut sampler = Sampler::default();
        let absent = sampler.sample(missing, &SensorConfig::default(), T0);
        assert_eq!(
            absent.state,
            CardState::Unavailable("Source missing".into())
        );
        let disabled = sampler.sample(
            missing,
            &SensorConfig {
                channels: Some(vec![]),
            },
            T0,
        );
        assert_eq!(
            disabled.state,
            CardState::Unavailable("No sensor channels selected".into())
        );
        assert!(disabled.metrics.is_empty());
    }
    #[test]
    fn units_zero_and_malformed_are_not_inferred() {
        assert_eq!(parse_value("temp1", "35000\n"), Ok(35000));
        assert_eq!(display_value("temp1", -1250), "-1.2 °C");
        assert_eq!(display_value("fan1", 0), "0 RPM · stopped");
        assert_eq!(display_value("fan1", 820), "820 RPM");
        for source in ["", "35C", "1.2", "9223372036854775808"] {
            assert!(parse_value("temp1", source).is_err());
        }
        assert!(parse_value("fan1", "-1").is_err());
    }
    #[test]
    fn conservative_defaults_do_not_expose_unlabeled_board_channels() {
        let t = Tree::new();
        let cpu = t.chip(0, "cpu", "coretemp");
        put(&cpu, "temp1_input", "54200");
        put(&cpu, "temp1_label", "Package id 0");
        put(&cpu, "temp2_input", "50000");
        put(&cpu, "temp2_label", "Core 0");
        let gpu = t.chip(1, "gpu", "amdgpu");
        put(&gpu, "temp1_input", "46000");
        put(&gpu, "temp1_label", "edge");
        put(&gpu, "temp2_input", "78000");
        put(&gpu, "temp2_label", "junction");
        put(&gpu, "fan1_input", "0");
        let board = t.chip(2, "board", "nct6791");
        put(&board, "temp1_input", "999999");
        put(&board, "temp2_input", "35000");
        put(&board, "temp2_label", "SYSTIN");
        put(&board, "fan1_input", "820");
        put(&board, "fan1_label", "Case");
        let card = Sampler::default().sample(&t.root(), &SensorConfig::default(), T0);
        assert_eq!(card.metrics.len(), 3);
        assert!(card.metrics.iter().any(|m| m.label == "GPU · edge"));
        assert!(card.metrics.iter().any(|m| m.value == "820 RPM"));
    }
    #[test]
    fn faults_permissions_disabled_and_malformed_flags_are_unavailable_not_zero() {
        let c = Channel {
            identity: Identity {
                chip: "x".into(),
                device: "/sys/devices/x".into(),
                channel: "fan1".into(),
            },
            label: None,
            path: "/test".into(),
        };
        let read = |fault: &'static str, enable: &'static str| {
            move |p: &Path| {
                Ok(if p.ends_with("fan1_fault") {
                    fault
                } else if p.ends_with("fan1_enable") {
                    enable
                } else {
                    "0"
                }
                .into())
            }
        };
        assert_eq!(read_channel_with(&c, read("0", "1")), Ok(0));
        for (fault, enable) in [("1", "1"), ("0", "0"), ("x", "1"), ("0", "2")] {
            assert!(read_channel_with(&c, read(fault, enable)).is_err());
        }
        assert_eq!(
            read_channel_with(&c, |_| Err(std::io::ErrorKind::PermissionDenied.into())),
            Err("Permission denied".into())
        );
        assert_eq!(
            read_channel_with(&c, |_| Err(std::io::ErrorKind::NotFound.into())),
            Err("Source missing".into())
        );
    }
    #[test]
    fn stable_identity_hotplug_last_success_and_bounded_gaps() {
        let t = Tree::new();
        let p = t.chip(0, "cpu", "coretemp");
        put(&p, "temp1_input", "54200");
        put(&p, "temp1_label", "Package id 0");
        let id = discover(&t.root()).unwrap()[0].identity.clone();
        assert!(!id.device.contains("hwmon"));
        let mut sampler = Sampler::default();
        let first = sampler.sample(&t.root(), &SensorConfig::default(), T0);
        std::fs::remove_file(t.root().join("hwmon0")).unwrap();
        for _ in 0..30 {
            let card = sampler.sample(&t.root(), &SensorConfig::default(), T1);
            assert_eq!(card.state, CardState::Stale);
            assert_eq!(card.metrics[0].value, first.metrics[0].value);
            assert!(card.metrics[0].detail.contains("Last success 1 min ago"));
        }
        assert_eq!(sampler.readings[&id].history.len(), 31);
        assert_eq!(sampler.readings[&id].history.front(), Some(&Some(54200)));
        assert!(
            sampler.readings[&id]
                .history
                .iter()
                .skip(1)
                .all(Option::is_none)
        );
        let new = p.parent().unwrap().join("hwmon7");
        std::fs::rename(&p, &new).unwrap();
        symlink(&new, t.root().join("hwmon7")).unwrap();
        assert_eq!(discover(&t.root()).unwrap()[0].identity, id);
        put(&new, "temp1_input", "55000");
        let recovered = sampler.sample(&t.root(), &SensorConfig::default(), T2);
        assert_eq!(recovered.metrics.len(), 1);
        assert_eq!(recovered.state, CardState::Ready);
        assert_eq!(recovered.metrics[0].value, "55.0 °C");
        assert_eq!(sampler.readings[&id].history.back(), Some(&Some(55000)));
    }
    #[test]
    fn automatic_absence_expires_at_120_polls_and_reconnection_starts_new_history() {
        let t = Tree::new();
        let p = t.chip(0, "cpu", "coretemp");
        put(&p, "temp1_input", "54200");
        put(&p, "temp1_label", "Package id 0");
        let id = discover(&t.root()).unwrap()[0].identity.clone();
        let mut sampler = Sampler::default();
        sampler.sample(&t.root(), &SensorConfig::default(), T0);
        std::fs::remove_file(t.root().join("hwmon0")).unwrap();
        for _ in 0..119 {
            let card = sampler.sample(&t.root(), &SensorConfig::default(), T1);
            assert_eq!(card.state, CardState::Stale);
            assert_eq!(card.metrics[0].value, "54.2 °C");
            assert!(card.metrics[0].detail.contains("Last success 1 min ago"));
            assert!(!card.note.contains("Retired"));
        }
        let retired = sampler.sample(&t.root(), &SensorConfig::default(), T2);
        assert!(
            retired.metrics.is_empty(),
            "expired automatic reading was retained"
        );
        assert!(!sampler.readings.contains_key(&id));
        assert!(retired.note.contains("Retired 1 absent sensor"));
        symlink(&p, t.root().join("hwmon7")).unwrap();
        put(&p, "temp1_input", "55000");
        let recovered = sampler.sample(&t.root(), &SensorConfig::default(), T3);
        assert_eq!(recovered.state, CardState::Ready);
        assert_eq!(recovered.metrics[0].value, "55.0 °C");
        assert!(recovered.metrics[0].detail.contains("1 / 120 raw 2s slots"));
        assert_eq!(sampler.readings[&id].history.len(), 1);
        assert!(recovered.note.contains("Retired 1 absent sensor"));
    }

    #[test]
    fn distinct_device_churn_caps_absent_records_and_retires_oldest_first() {
        let t = Tree::new();
        let mut sampler = Sampler::default();
        let mut ids = Vec::new();
        for n in 0..80 {
            if n > 0 {
                std::fs::remove_file(t.root().join(format!("hwmon{}", n - 1))).unwrap();
            }
            let p = t.chip(n, &format!("cpu{n}"), "coretemp");
            put(&p, "temp1_input", "54200");
            put(&p, "temp1_label", "Package id 0");
            let id = discover(&t.root()).unwrap()[0].identity.clone();
            let card = sampler.sample(&t.root(), &SensorConfig::default(), T0);
            assert!(
                sampler.readings.len() <= 33,
                "distinct canonical device churn retained {} records",
                sampler.readings.len()
            );
            assert!(
                sampler.readings.contains_key(&id),
                "connected reading omitted"
            );
            assert!(card.metrics.iter().any(|m| m.detail.contains(&id.device)));
            ids.push(id);
        }
        assert_eq!(sampler.readings.len(), 33);
        assert!(
            ids[..47]
                .iter()
                .all(|id| !sampler.readings.contains_key(id))
        );
        assert!(ids[47..].iter().all(|id| sampler.readings.contains_key(id)));
        let card = sampler.sample(&t.root(), &SensorConfig::default(), T1);
        assert!(card.note.contains("Retired 47 absent sensors"));
    }

    #[test]
    fn retirement_ties_use_stable_identity_and_cumulative_count_saturates() {
        let t = Tree::new();
        for n in 0..34 {
            let p = t.chip(n, &format!("cpu{n:03}"), "coretemp");
            put(&p, "temp1_input", "54200");
            put(&p, "temp1_label", "Package id 0");
        }
        let ids: Vec<_> = discover(&t.root())
            .unwrap()
            .into_iter()
            .map(|c| c.identity)
            .collect();
        let mut sampler = Sampler::default();
        sampler.sample(&t.root(), &SensorConfig::default(), T0);
        sampler.retired_auto = usize::MAX - 1;
        for n in 0..34 {
            std::fs::remove_file(t.root().join(format!("hwmon{n}"))).unwrap();
        }
        let card = sampler.sample(&t.root(), &SensorConfig::default(), T1);
        assert_eq!(sampler.readings.len(), 32);
        assert!(ids[..2].iter().all(|id| !sampler.readings.contains_key(id)));
        assert!(ids[2..].iter().all(|id| sampler.readings.contains_key(id)));
        assert_eq!(sampler.retired_auto, usize::MAX);
        assert!(
            card.note
                .contains(&format!("Retired {} absent sensors", usize::MAX))
        );
    }

    #[test]
    fn simultaneous_connected_automatic_channels_are_not_capped() {
        let t = Tree::new();
        for n in 0..40 {
            let p = t.chip(n, &format!("cpu{n}"), "coretemp");
            put(&p, "temp1_input", "54200");
            put(&p, "temp1_label", "Package id 0");
        }
        let mut sampler = Sampler::default();
        let card = sampler.sample(&t.root(), &SensorConfig::default(), T0);
        assert_eq!(card.metrics.len(), 40);
        assert_eq!(sampler.readings.len(), 40);
        assert_eq!(card.state, CardState::Ready);
        assert!(!card.note.contains("Retired"));
    }

    #[test]
    fn configured_absence_and_present_faults_never_expire() {
        let t = Tree::new();
        let p = t.chip(0, "cpu", "coretemp");
        put(&p, "temp1_input", "54200");
        put(&p, "temp1_label", "Package id 0");
        let id = discover(&t.root()).unwrap()[0].identity.clone();
        let config = SensorConfig {
            channels: Some(vec![Selector {
                chip: id.chip.clone(),
                device: id.device.clone(),
                channel: id.channel.clone(),
                label: "Configured CPU".into(),
            }]),
        };
        let mut configured = Sampler::default();
        configured.sample(&t.root(), &config, T0);
        std::fs::remove_file(t.root().join("hwmon0")).unwrap();
        for _ in 0..240 {
            configured.sample(&t.root(), &config, T1);
        }
        assert_eq!(configured.readings.len(), 1);
        assert_eq!(configured.readings[&id].history.len(), 120);
        assert_eq!(configured.readings[&id].last.as_ref().unwrap().1, T0);
        symlink(&p, t.root().join("hwmon0")).unwrap();
        let mut automatic = Sampler::default();
        automatic.sample(&t.root(), &SensorConfig::default(), T0);
        put(&p, "temp1_fault", "1");
        for _ in 0..240 {
            let card = automatic.sample(&t.root(), &SensorConfig::default(), T1);
            assert_eq!(card.metrics.len(), 1);
            assert!(card.metrics[0].detail.contains("STALE · Sensor fault"));
            assert!(!card.note.contains("Retired"));
        }
        assert_eq!(automatic.readings[&id].history.len(), 120);
    }

    #[test]
    fn device_symlink_identity_matches_fallback_and_explicit_temperatures_do_not_use_defaults() {
        let t = Tree::new();
        let p = t.chip(0, "board", "board");
        put(&p, "temp4_input", "-2500");
        let fallback = discover(&t.root()).unwrap()[0].identity.clone();
        symlink(t.0.join("devices/board"), p.join("device")).unwrap();
        assert_eq!(discover(&t.root()).unwrap()[0].identity, fallback);
        let config = SensorConfig {
            channels: Some(vec![Selector {
                chip: fallback.chip,
                device: fallback.device,
                channel: "temp4".into(),
                label: "Selected probe".into(),
            }]),
        };
        let card = Sampler::default().sample(&t.root(), &config, T0);
        assert_eq!(card.metrics.len(), 1);
        assert_eq!(card.metrics[0].label, "Selected probe");
        assert_eq!(card.metrics[0].value, "-2.5 °C");
        assert!(card.metrics[0].detail.contains("millidegrees Celsius"));
        assert!(
            card.metrics[0]
                .detail
                .contains("1 / 120 raw 2s slots, 0 gaps")
        );
    }
    #[test]
    fn duplicate_chip_devices_remain_distinct_and_duplicate_identity_is_not_chosen() {
        let t = Tree::new();
        for (n, device) in [(0, "cpu0"), (1, "cpu1")] {
            let p = t.chip(n, device, "coretemp");
            put(&p, "temp1_input", "54000");
            put(&p, "temp1_label", "Package id 0");
        }
        let mut s = Sampler::default();
        assert_eq!(
            s.sample(&t.root(), &SensorConfig::default(), T0)
                .metrics
                .len(),
            2
        );
        symlink(
            t.0.join("devices/cpu0/hwmon/hwmon0"),
            t.root().join("hwmon9"),
        )
        .unwrap();
        let card = s.sample(&t.root(), &SensorConfig::default(), T1);
        assert_eq!(card.state, CardState::Stale);
        assert!(
            card.metrics
                .iter()
                .any(|m| m.detail.contains("Ambiguous duplicate identity"))
        );
    }
    #[test]
    fn explicit_selection_includes_unlabeled_zero_and_missing_sources() {
        let t = Tree::new();
        let p = t.chip(0, "board", "board");
        put(&p, "fan1_input", "0");
        let id = discover(&t.root()).unwrap()[0].identity.clone();
        let config = SensorConfig {
            channels: Some(vec![Selector {
                chip: id.chip,
                device: id.device,
                channel: id.channel,
                label: "Chosen fan".into(),
            }]),
        };
        let mut s = Sampler::default();
        assert_eq!(
            s.sample(&t.root(), &config, T0).metrics[0].value,
            "0 RPM · stopped"
        );
        put(&p, "fan1_input", "bad");
        let card = s.sample(&t.root(), &config, T1);
        assert_eq!(card.metrics[0].value, "0 RPM · stopped");
        assert!(card.metrics[0].detail.contains("STALE · Malformed reading"));
        let missing =
            Sampler::default().sample(Path::new("/definitely/missing/hwmon"), &config, T2);
        assert_eq!(missing.metrics[0].value, "—");
        assert!(matches!(missing.state, CardState::Unavailable(_)));
        assert!(
            missing.metrics[0]
                .detail
                .contains("UNAVAILABLE · Source missing")
        );
    }
    #[test]
    fn sampled_card_carries_temperature_graphs_and_no_fan_graphs() {
        let t = Tree::new();
        let cpu = t.chip(0, "cpu", "coretemp");
        put(&cpu, "temp1_input", "54200");
        put(&cpu, "temp1_label", "Package id 0");
        put(&cpu, "fan1_input", "820");
        put(&cpu, "fan1_label", "Case");
        let mut sampler = Sampler::default();
        let card = sampler.sample(&t.root(), &SensorConfig::default(), T0);
        assert_eq!(card.metrics.len(), 2);
        assert_eq!(card.graphs.len(), 1);
        let graph = &card.graphs[0];
        assert_eq!(graph.unit.as_deref(), Some("°C"));
        assert_eq!(graph.range, Some((50_000, 60_000)));
        assert_eq!(graph.signed_samples.as_ref().unwrap(), &vec![Some(54_200)]);
        let card = sampler.sample(&t.root(), &SensorConfig::default(), T1);
        assert_eq!(card.graphs.len(), 1);
        assert_eq!(
            card.graphs[0].signed_samples.as_ref().unwrap(),
            &vec![Some(54_200), Some(54_200)]
        );
        assert_eq!(card.graphs[0].range, Some((50_000, 60_000)));
        assert!(card.graphs.iter().all(|g| g.unit.as_deref() == Some("°C")));
    }
    #[test]
    fn held_temperature_range_survives_eviction_and_expands_outward() {
        let t = Tree::new();
        let cpu = t.chip(0, "cpu", "coretemp");
        put(&cpu, "temp1_input", "85000");
        put(&cpu, "temp1_label", "Package id 0");
        let mut sampler = Sampler::default();
        let card = sampler.sample(&t.root(), &SensorConfig::default(), T0);
        assert_eq!(card.graphs.len(), 1);
        assert_eq!(card.graphs[0].range, Some((85_000, 95_000)));
        put(&cpu, "temp1_input", "88000");
        for _ in 0..121 {
            sampler.sample(&t.root(), &SensorConfig::default(), T1);
        }
        let card = sampler.sample(&t.root(), &SensorConfig::default(), T1);
        assert_eq!(card.graphs.len(), 1);
        assert_eq!(
            card.graphs[0].range,
            Some((85_000, 95_000)),
            "evicting the 85 °C extreme must not shrink the held range"
        );
        assert!(
            !card.graphs[0]
                .signed_samples
                .as_ref()
                .unwrap()
                .contains(&Some(85_000)),
            "extreme sample should have evicted by now"
        );
        put(&cpu, "temp1_input", "96000");
        let card = sampler.sample(&t.root(), &SensorConfig::default(), T2);
        assert_eq!(card.graphs[0].range, Some((85_000, 100_000)));
        put(&cpu, "temp1_input", "42000");
        let card = sampler.sample(&t.root(), &SensorConfig::default(), T3);
        assert_eq!(card.graphs[0].range, Some((40_000, 100_000)));
    }
    #[test]
    fn demo_temperature_histories_are_deterministic_and_ranged() {
        let first = demo_card();
        let second = demo_card();
        assert_eq!(first, second);
        assert_eq!(first.metrics.len(), 4);
        assert_eq!(first.graphs.len(), 2);
        for graph in &first.graphs {
            let samples = graph.signed_samples.as_ref().unwrap();
            assert_eq!(samples.len(), 60);
            assert!(samples.iter().all(Option::is_some));
            let (floor, ceiling) = graph.range.unwrap();
            assert_eq!(floor % 5000, 0);
            assert_eq!(ceiling % 5000, 0);
            assert!(ceiling - floor >= 10_000);
            assert_eq!(graph.unit.as_deref(), Some("°C"));
        }
        assert_ne!(
            first.graphs[0].signed_samples,
            first.graphs[1].signed_samples
        );
    }
}
