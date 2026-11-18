//! Linux local telemetry. Histories are session-only, never spend estimates.
use crate::{Card, CardMetric, CardState, Graph};
use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

pub const SAMPLE_SECS: u64 = 2;
pub const HISTORY_LIMIT: usize = 120;
/// Visible span of every local history graph: HISTORY_LIMIT slots at SAMPLE_SECS.
pub const HISTORY_WINDOW_SECS: u64 = HISTORY_LIMIT as u64 * SAMPLE_SECS;

/// Short label for the graph window, e.g. "4 min", so cards can name the span
/// their trend lines cover without repeating the slot arithmetic.
pub fn history_window() -> String {
    let secs = HISTORY_WINDOW_SECS;
    if secs.is_multiple_of(60) {
        format!("{} min", secs / 60)
    } else {
        format!("{secs} s")
    }
}
type NetworkCounters = BTreeMap<String, (u64, u64)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTicks {
    pub busy: u64,
    pub total: u64,
}

pub fn parse_cpu(source: &str) -> Result<BTreeMap<String, CpuTicks>, String> {
    let mut rows = BTreeMap::new();
    for line in source.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else { continue };
        if name != "cpu"
            && !name
                .strip_prefix("cpu")
                .is_some_and(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_digit()))
        {
            continue;
        }
        let values: Vec<u64> = fields
            .take(8)
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| "Invalid CPU counters")?;
        if values.len() < 4 {
            return Err("Incomplete CPU counters".into());
        }
        let total = values
            .iter()
            .try_fold(0u64, |sum, n| sum.checked_add(*n))
            .ok_or("CPU counter overflow")?;
        let idle = values[3]
            .checked_add(*values.get(4).unwrap_or(&0))
            .ok_or("CPU idle overflow")?;
        rows.insert(
            name.into(),
            CpuTicks {
                total,
                busy: total - idle,
            },
        );
    }
    if !rows.contains_key("cpu") {
        return Err("CPU counters unavailable".into());
    }
    Ok(rows)
}

pub fn cpu_percent(previous: CpuTicks, current: CpuTicks) -> Option<u64> {
    let total = current.total.checked_sub(previous.total)?;
    let busy = current.busy.checked_sub(previous.busy)?;
    if total == 0 || busy > total {
        return None;
    }
    Some((u128::from(busy) * 100 / u128::from(total)) as u64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

pub fn parse_memory(source: &str) -> Result<Memory, String> {
    let mut fields = BTreeMap::new();
    for line in source.lines() {
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() == 3 && parts[2] == "kB" {
            let n = parts[1]
                .parse::<u64>()
                .map_err(|_| "Invalid memory counter")?
                .checked_mul(1024)
                .ok_or("Memory overflow")?;
            fields.insert(parts[0], n);
        }
    }
    let get = |key| {
        fields
            .get(key)
            .copied()
            .ok_or_else(|| format!("Missing {key}"))
    };
    let total = get("MemTotal:")?;
    if total == 0 {
        return Err("Zero RAM total".into());
    }
    let used = total
        .checked_sub(get("MemAvailable:")?)
        .ok_or("Invalid available RAM")?;
    let swap_total = get("SwapTotal:")?;
    let swap_used = swap_total
        .checked_sub(get("SwapFree:")?)
        .ok_or("Invalid free swap")?;
    Ok(Memory {
        total,
        used,
        swap_total,
        swap_used,
    })
}

pub fn parse_network(source: &str) -> Result<BTreeMap<String, (u64, u64)>, String> {
    let mut interfaces = BTreeMap::new();
    for line in source.lines() {
        let Some((name, values)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name == "lo" {
            continue;
        }
        let values: Vec<_> = values.split_whitespace().collect();
        if values.len() < 16 {
            return Err("Incomplete network counters".into());
        }
        let rx = values[0]
            .parse()
            .map_err(|_| "Invalid network receive counter")?;
        let tx = values[8]
            .parse()
            .map_err(|_| "Invalid network transmit counter")?;
        interfaces.insert(name.into(), (rx, tx));
    }
    Ok(interfaces)
}

/// Match interfaces individually: resets/new devices cannot create aggregate spikes.
pub fn network_rate(
    previous: &BTreeMap<String, (u64, u64)>,
    current: &BTreeMap<String, (u64, u64)>,
    elapsed: f64,
) -> Option<(u64, u64)> {
    if !elapsed.is_finite() || elapsed <= 0.0 {
        return None;
    }
    let mut totals = (0u64, 0u64);
    let mut matched = false;
    for (name, &(rx, tx)) in current {
        let Some(&(old_rx, old_tx)) = previous.get(name) else {
            continue;
        };
        let (Some(rx), Some(tx)) = (rx.checked_sub(old_rx), tx.checked_sub(old_tx)) else {
            continue;
        };
        totals.0 = totals.0.checked_add(rx)?;
        totals.1 = totals.1.checked_add(tx)?;
        matched = true;
    }
    matched.then(|| {
        (
            (totals.0 as f64 / elapsed) as u64,
            (totals.1 as f64 / elapsed) as u64,
        )
    })
}

/// Used matches df; available excludes blocks reserved from ordinary users.
pub fn disk_usage(
    blocks: u64,
    free: u64,
    available: u64,
    block_size: u64,
) -> Result<(u64, u64), String> {
    if available > free || block_size == 0 {
        return Err("Invalid available disk counters".into());
    }
    let used = blocks
        .checked_sub(free)
        .and_then(|n| n.checked_mul(block_size))
        .ok_or("Invalid disk counters")?;
    let available = available
        .checked_mul(block_size)
        .ok_or("Disk counter overflow")?;
    Ok((used, available))
}

fn root_disk() -> Result<(u64, u64), String> {
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // The constant is NUL-terminated, statvfs initializes the struct on success.
    let result = unsafe { libc::statvfs(c"/".as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return Err("Root filesystem statistics unavailable".into());
    }
    let stat = unsafe { stat.assume_init() };
    disk_usage(stat.f_blocks, stat.f_bfree, stat.f_bavail, stat.f_frsize)
}

pub fn bytes(value: u64) -> String {
    format!("{:.1} GiB", value as f64 / 1_073_741_824.0)
}
fn rate(value: u64) -> String {
    format!("{:.1} KiB/s", value as f64 / 1024.0)
}
fn pct(used: u64, total: u64) -> u64 {
    if total == 0 {
        0
    } else {
        (u128::from(used) * 100 / u128::from(total)).min(100) as u64
    }
}

pub fn metric(label: &str, value: String, detail: &str) -> CardMetric {
    CardMetric {
        label: label.into(),
        value,
        detail: detail.into(),
        fraction: None,
        elapsed_pct: None,
        pace: None,
    }
}
pub fn card(id: &str, title: &str, subtitle: &str, metrics: Vec<CardMetric>) -> Card {
    Card {
        id: id.into(),
        title: title.into(),
        eyebrow: title.into(),
        subtitle: subtitle.into(),
        state: CardState::Ready,
        metrics,
        graphs: vec![],
        weather: None,
        note: String::new(),
    }
}
pub fn unavailable(id: &str, title: &str, reason: &str) -> Card {
    let mut c = card(id, title, "", vec![]);
    c.state = CardState::Unavailable(reason.into());
    c
}

#[derive(Default)]
pub struct History(VecDeque<Option<u64>>);
impl History {
    pub fn push(&mut self, value: u64) {
        self.push_slot(Some(value));
    }
    pub fn push_slot(&mut self, value: Option<u64>) {
        if self.0.len() == HISTORY_LIMIT {
            self.0.pop_front();
        }
        self.0.push_back(value);
    }
    pub fn graph(&self, label: String, ceiling: u64) -> Graph {
        Graph::unsigned(label, self.0.iter().copied().collect(), ceiling.max(1))
    }
}

#[derive(Default)]
pub struct Sampler {
    cpu: Option<BTreeMap<String, CpuTicks>>,
    network: Option<(Instant, NetworkCounters)>,
    histories: BTreeMap<String, History>,
    cached: BTreeMap<String, Card>,
}
impl Sampler {
    fn history(&mut self, key: &str, value: u64, label: String, ceiling: u64) -> Graph {
        let history = self.histories.entry(key.into()).or_default();
        history.push(value);
        history.graph(label, ceiling)
    }
    fn retain(&mut self, id: &str, title: &str, result: Result<Card, String>) -> Card {
        match result {
            Ok(mut c) => {
                c.note = format!("Linux · {SAMPLE_SECS}s · {HISTORY_LIMIT} slots");
                // Sampled atomically with this tick, so it is always fresh here.
                c.note.push_str("\nSampled just now");
                self.cached.insert(id.into(), c.clone());
                c
            }
            Err(reason) => {
                for (key, history) in &mut self.histories {
                    if (id == "cpu" && key.starts_with("cpu"))
                        || (id == "ram" && matches!(key.as_str(), "ram" | "swap"))
                        || (id == "network" && matches!(key.as_str(), "rx" | "tx"))
                    {
                        history.push_slot(None);
                    }
                }
                match self.cached.get(id) {
                    Some(c) => {
                        let mut c = c.clone();
                        c.state = CardState::Stale;
                        c.note = format!(
                            "STALE · {reason} · last successful sample retained\n{}",
                            c.note
                        );
                        c
                    }
                    None => unavailable(id, title, &reason),
                }
            }
        }
    }
    pub fn sample(&mut self) -> Vec<Card> {
        let cpu = std::fs::read_to_string("/proc/stat")
            .map_err(|_| "Cannot read /proc/stat".into())
            .and_then(|s| parse_cpu(&s))
            .inspect_err(|_| self.cpu = None)
            .and_then(|current| self.cpu_card(current));
        let ram = std::fs::read_to_string("/proc/meminfo")
            .map_err(|_| "Cannot read /proc/meminfo".into())
            .and_then(|s| parse_memory(&s))
            .map(|m| self.memory_card(m));
        let network = std::fs::read_to_string("/proc/net/dev")
            .map_err(|_| "Cannot read /proc/net/dev".into())
            .and_then(|s| parse_network(&s))
            .inspect_err(|_| self.network = None)
            .and_then(|n| self.network_card(n, Instant::now()));
        let disk = root_disk().map(|(used, free)| {
            card(
                "disk",
                "Disk",
                "Root filesystem /",
                vec![
                    metric("Used", bytes(used), ""),
                    metric(
                        "Available",
                        bytes(free),
                        "Normal-user space (df Available); excludes reserved blocks",
                    ),
                ],
            )
        });
        vec![
            self.retain("cpu", "CPU", cpu),
            self.retain("ram", "Memory", ram),
            self.retain("network", "Network", network),
            self.retain("disk", "Disk", disk),
        ]
    }
    fn cpu_card(&mut self, current: BTreeMap<String, CpuTicks>) -> Result<Card, String> {
        let previous = self
            .cpu
            .replace(current.clone())
            .ok_or("CPU history warming up: waiting for second sample")?;
        self.histories
            .retain(|key, _| !key.starts_with("cpu") || current.contains_key(key));
        let mut c = card(
            "cpu",
            "CPU",
            &format!(
                "{} logical cores · utilization",
                current.len().saturating_sub(1)
            ),
            vec![],
        );
        for (name, ticks) in current {
            if let Some(percent) = previous.get(&name).and_then(|old| cpu_percent(*old, ticks)) {
                let label = if name == "cpu" {
                    format!("Total · {percent}%")
                } else {
                    format!("Core {} · {percent}%", &name[3..])
                };
                c.graphs.push(self.history(&name, percent, label, 100));
            } else {
                let history = self.histories.entry(name.clone()).or_default();
                history.push_slot(None);
                let label = if name == "cpu" {
                    "Total · —".into()
                } else {
                    format!("Core {} · —", &name[3..])
                };
                c.graphs.push(history.graph(label, 100));
            }
        }
        c.graphs.sort_by_key(|graph| {
            graph
                .label
                .strip_prefix("Core ")
                .and_then(|s| s.split(' ').next())
                .and_then(|n| n.parse::<usize>().ok())
                .map(|n| n + 1)
                .unwrap_or(0)
        });
        if c.graphs.is_empty() {
            return Err("CPU counters reset or unchanged; resampling".into());
        }
        Ok(c)
    }
    fn memory_card(&mut self, m: Memory) -> Card {
        let mut c = card(
            "ram",
            "Memory",
            "RAM and swap are separate",
            vec![
                metric(
                    "RAM used / total",
                    format!("{} / {}", bytes(m.used), bytes(m.total)),
                    "Used = total − MemAvailable",
                ),
                metric(
                    "Swap used / total",
                    format!("{} / {}", bytes(m.swap_used), bytes(m.swap_total)),
                    if m.swap_total == 0 {
                        "No swap configured"
                    } else {
                        ""
                    },
                ),
            ],
        );
        c.graphs.push(self.history(
            "ram",
            pct(m.used, m.total),
            format!("RAM · {}%", pct(m.used, m.total)),
            100,
        ));
        if m.swap_total == 0 {
            self.histories.remove("swap");
        } else {
            c.graphs.push(self.history(
                "swap",
                pct(m.swap_used, m.swap_total),
                format!("Swap · {}%", pct(m.swap_used, m.swap_total)),
                100,
            ));
        }
        c
    }
    fn network_card(
        &mut self,
        current: BTreeMap<String, (u64, u64)>,
        now: Instant,
    ) -> Result<Card, String> {
        let old = self
            .network
            .replace((now, current.clone()))
            .ok_or("Throughput warming up: waiting for second sample")?;
        let (rx, tx) = network_rate(&old.1, &current, now.duration_since(old.0).as_secs_f64())
            .ok_or("No comparable non-loopback counters; resampling")?;
        let mut c = card(
            "network",
            "Network",
            "All non-loopback interfaces",
            vec![
                metric("Receive", rate(rx), ""),
                metric(
                    "Transmit",
                    rate(tx),
                    "Virtual interfaces may double-count traffic",
                ),
            ],
        );
        for (key, value, name) in [("rx", rx, "Receive"), ("tx", tx, "Transmit")] {
            let history = self.histories.entry(key.into()).or_default();
            history.push(value);
            let max = history
                .0
                .iter()
                .flatten()
                .copied()
                .max()
                .unwrap_or(1)
                .max(1024);
            c.graphs
                .push(history.graph(format!("{name} · auto Y 0–{}", rate(max)), max));
        }
        Ok(c)
    }
}

/// Busy four-minute CPU fixture, generated from a seeded synthetic workload, not /proc.
pub fn busy_cpu_demo() -> Card {
    let cores: Vec<Vec<Option<u64>>> =
        serde_json::from_str(include_str!("../tests/fixtures/cpu-busy.json")).unwrap();
    let total: Vec<_> = (0..HISTORY_LIMIT)
        .map(|slot| {
            cores
                .iter()
                .map(|core| core[slot])
                .sum::<Option<u64>>()
                .map(|v| v / cores.len() as u64)
        })
        .collect();
    let mut cpu = card(
        "cpu",
        "CPU",
        "12 logical cores · synthetic utilization",
        vec![],
    );
    for (index, samples) in std::iter::once(total).chain(cores).enumerate() {
        let name = if index == 0 {
            "Total".into()
        } else {
            format!("Core {}", index - 1)
        };
        let current = samples.last().copied().flatten().unwrap();
        cpu.graphs.push(Graph::unsigned(
            format!("{name} · {current}%"),
            samples,
            100,
        ));
    }
    cpu
}

pub fn demo_cards() -> Vec<Card> {
    let mut s = Sampler::default();
    let mut cpu = card(
        "cpu",
        "CPU",
        "8 logical cores · synthetic utilization",
        vec![],
    );
    let core_samples: Vec<Vec<u64>> = (0..8)
        .map(|core| (0..60).map(|i| (i * 7 + core * 13) % 72 + 8).collect())
        .collect();
    let total: Vec<u64> = (0..60)
        .map(|i| core_samples.iter().map(|core| core[i]).sum::<u64>() / 8)
        .collect();
    cpu.graphs.push(Graph::unsigned(
        format!("Total · {}%", total[59]),
        total.into_iter().map(Some).collect(),
        100,
    ));
    for (core, samples) in core_samples.into_iter().enumerate() {
        cpu.graphs.push(Graph::unsigned(
            format!("Core {core} · {}%", samples[59]),
            samples.into_iter().map(Some).collect(),
            100,
        ));
    }
    let mut memory = s.memory_card(Memory {
        total: 16 << 30,
        used: 7 << 30,
        swap_total: 4 << 30,
        swap_used: 1 << 29,
    });
    memory.graphs[0].samples = (0..60).map(|i| Some(40 + (i + 7) % 9)).collect();
    memory.graphs[1].samples = vec![Some(12); 60];
    let mut net = card(
        "network",
        "Network",
        "Synthetic non-loopback traffic",
        vec![
            metric("Receive", "128.0 KiB/s".into(), ""),
            metric("Transmit", "32.0 KiB/s".into(), ""),
        ],
    );
    net.graphs = ["Receive", "Transmit"]
        .into_iter()
        .enumerate()
        .map(|(j, label)| {
            Graph::unsigned(
                format!("{label} · auto Y 0–256 KiB/s"),
                (0..60)
                    .map(|i| Some(((i * 17 + j as u64 * 9) % 200) * 1024))
                    .collect(),
                256 * 1024,
            )
        })
        .collect();
    net.graphs[0].samples[59] = Some(128 * 1024);
    net.graphs[1].samples[59] = Some(32 * 1024);
    vec![
        cpu,
        memory,
        net,
        card(
            "disk",
            "Disk",
            "Synthetic root filesystem /",
            vec![
                metric("Used", "186.0 GiB".into(), ""),
                metric(
                    "Available",
                    "280.0 GiB".into(),
                    "Normal-user space (df Available); excludes reserved blocks",
                ),
            ],
        ),
        crate::sensors::demo_card(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn busy_cpu_fixture_has_full_history_twelve_distinct_cores_and_raw_excursions() {
        let cpu = busy_cpu_demo();
        assert_eq!(cpu.graphs.len(), 13);
        for (index, graph) in cpu.graphs.iter().enumerate() {
            assert_eq!(graph.samples.len(), HISTORY_LIMIT);
            assert_eq!(graph.ceiling, 100);
            let last = graph.samples[119].unwrap();
            assert!(graph.label.ends_with(&format!("{last}%")));
            assert!(graph.samples.iter().flatten().all(|v| *v <= 100));
            if index > 0 {
                assert!(graph.samples.contains(&Some(100)));
                assert!(graph.samples.iter().flatten().any(|v| *v < 5));
                if index > 1 {
                    assert_ne!(graph.samples, cpu.graphs[index - 1].samples);
                }
            }
        }
        for slot in 0..120 {
            let sum: Option<u64> = cpu.graphs[1..].iter().map(|g| g.samples[slot]).sum();
            assert_eq!(cpu.graphs[0].samples[slot], sum.map(|v| v / 12));
        }
        assert_eq!(cpu.graphs[3].samples[52], None);
        assert_eq!(cpu.graphs[10].samples[119], Some(69));
        let points = crate::plot::history_points(&cpu.graphs[3].samples, 100, 119.0, 28.0);
        assert_eq!(points[52], None);
        assert_eq!(points[23], Some((23.0, 0.0)));
        assert_eq!(points.len(), 120);
    }

    #[test]
    fn cpu_excludes_guest_ticks_and_counts_iowait_idle() {
        let a = parse_cpu("cpu 10 0 10 70 10 0 0 0 9 0\ncpu0 10 0 10 70 10\n").unwrap();
        assert_eq!(
            a["cpu"],
            CpuTicks {
                busy: 20,
                total: 100
            }
        );
        assert_eq!(
            cpu_percent(
                a["cpu"],
                CpuTicks {
                    busy: 45,
                    total: 200
                }
            ),
            Some(25)
        );
        assert_eq!(cpu_percent(a["cpu"], a["cpu"]), None);
        assert_eq!(cpu_percent(a["cpu"], CpuTicks { busy: 1, total: 10 }), None);
        assert!(parse_cpu("cpu 1 x 3 4").is_err());
        assert!(parse_cpu("intr 123").is_err());
    }
    #[test]
    fn memory_uses_available_and_separates_swap() {
        let m = parse_memory(
            "MemTotal: 100 kB\nMemAvailable: 40 kB\nSwapTotal: 20 kB\nSwapFree: 15 kB",
        )
        .unwrap();
        assert_eq!(
            m,
            Memory {
                total: 102400,
                used: 61440,
                swap_total: 20480,
                swap_used: 5120
            }
        );
        assert!(parse_memory("MemTotal: 100 kB").is_err());
        assert!(parse_memory("MemTotal: 0 kB\nMemAvailable: 0 kB").is_err());
    }
    #[test]
    fn network_excludes_loopback_and_handles_hotplug_reset_and_elapsed() {
        let text =
            "lo: 90 0 0 0 0 0 0 0 80 0 0 0 0 0 0 0\neth0: 100 0 0 0 0 0 0 0 200 0 0 0 0 0 0 0";
        let previous = parse_network(text).unwrap();
        assert_eq!(previous.len(), 1);
        let mut current = previous.clone();
        current.insert("eth0".into(), (300, 300));
        current.insert("new".into(), (999999, 999999));
        assert_eq!(network_rate(&previous, &current, 2.0), Some((100, 50)));
        current.insert("eth0".into(), (1, 1));
        assert_eq!(network_rate(&previous, &current, 2.0), None);
        assert_eq!(network_rate(&previous, &previous, 0.0), None);
    }
    #[test]
    fn history_is_bounded_and_disk_arithmetic_checked() {
        let (used, available) = disk_usage(100, 30, 20, 1).unwrap();
        assert_eq!((used, available), (70, 20));
        assert_eq!(100 - used - available, 10); // reserved, not counted as used
        assert!(disk_usage(100, 30, 31, 1).is_err());
        let mut h = History::default();
        for n in 0..500 {
            h.push(n);
        }
        assert_eq!(h.0.len(), HISTORY_LIMIT);
        assert_eq!(h.0.front(), Some(&Some(380)));
        assert_eq!(disk_usage(100, 30, 30, 1024), Ok((71680, 30720)));
        assert!(disk_usage(1, 2, 1, 1024).is_err());
        assert!(disk_usage(u64::MAX, 0, 0, 2).is_err());
        assert!(disk_usage(u64::MAX, u64::MAX, u64::MAX, 2).is_err());
        assert!(disk_usage(100, 30, 20, 0).is_err());
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_proc_sampling_smoke_uses_real_local_sources_without_network_requests() {
        let mut s = Sampler::default();
        let first = s.sample();
        assert_eq!(first.len(), 4);
        assert_eq!(first[1].state, CardState::Ready);
        assert_eq!(first[3].state, CardState::Ready);
        std::thread::sleep(std::time::Duration::from_millis(100));
        let second = s.sample();
        assert_eq!(second[0].state, CardState::Ready);
        assert!(!second[0].graphs.is_empty());
        assert!(
            second[0]
                .graphs
                .iter()
                .all(|g| g.samples.len() <= HISTORY_LIMIT)
        );
    }

    #[test]
    fn cpu_hotplug_history_and_no_swap_are_explicit() {
        let mut sampler = Sampler::default();
        let initial = parse_cpu(
            "cpu 10 0 0 90
cpu0 10 0 0 90",
        )
        .unwrap();
        assert!(sampler.cpu_card(initial).is_err());
        let next = parse_cpu(
            "cpu 20 0 0 180
cpu0 20 0 0 180
cpu1 20 0 0 180",
        )
        .unwrap();
        let warming = sampler.cpu_card(next).unwrap();
        assert_eq!(warming.graphs.len(), 3);
        assert_eq!(warming.graphs[2].samples, vec![None]); // new core has no invented value
        let third = parse_cpu(
            "cpu 30 0 0 270
cpu0 30 0 0 270
cpu1 30 0 0 270",
        )
        .unwrap();
        assert_eq!(sampler.cpu_card(third).unwrap().graphs.len(), 3);
        let removed = parse_cpu(
            "cpu 40 0 0 360
cpu0 40 0 0 360",
        )
        .unwrap();
        sampler.cpu_card(removed).unwrap();
        assert!(!sampler.histories.contains_key("cpu1"));
        let c = sampler.memory_card(Memory {
            total: 1024,
            used: 512,
            swap_total: 0,
            swap_used: 0,
        });
        assert_eq!(c.graphs.len(), 1);
        assert_eq!(c.metrics[1].detail, "No swap configured");
    }

    #[test]
    fn history_window_matches_retained_slots_and_sample_interval() {
        assert_eq!(HISTORY_WINDOW_SECS, 240);
        assert_eq!(history_window(), "4 min");
        let mut s = Sampler::default();
        let ram = s.memory_card(Memory {
            total: 1024,
            used: 512,
            swap_total: 4096,
            swap_used: 1024,
        });
        let retained = s.retain("ram", "Memory", Ok(ram));
        assert!(retained.note.contains("120 slots"), "{}", retained.note);
        assert!(!retained.note.contains("session history"));
    }

    #[test]
    fn failures_record_missing_slots_without_inventing_values() {
        let mut s = Sampler::default();
        let c = s.memory_card(Memory {
            total: 1024,
            used: 512,
            swap_total: 0,
            swap_used: 0,
        });
        s.retain("ram", "Memory", Ok(c));
        let stale = s.retain("ram", "Memory", Err("denied".into()));
        assert_eq!(stale.graphs[0].samples, vec![Some(50)]); // retained success is frozen/stale
        let recovered = s.memory_card(Memory {
            total: 1024,
            used: 512,
            swap_total: 0,
            swap_used: 0,
        });
        assert_eq!(recovered.graphs[0].samples, vec![Some(50), None, Some(50)]);
        let mut h = History::default();
        for _ in 0..500 {
            h.push_slot(None);
        }
        assert_eq!(h.0.len(), HISTORY_LIMIT);
        assert!(h.0.iter().all(Option::is_none));
    }

    #[test]
    fn cpu_core_order_is_numeric_and_reset_slots_are_missing() {
        let source = |ticks| {
            format!(
                "cpu {ticks} 0 0 100\ncpu0 {ticks} 0 0 100\ncpu2 {ticks} 0 0 100\ncpu10 {ticks} 0 0 100"
            )
        };
        let mut s = Sampler::default();
        assert!(s.cpu_card(parse_cpu(&source(10)).unwrap()).is_err());
        let c = s.cpu_card(parse_cpu(&source(20)).unwrap()).unwrap();
        assert!(c.graphs[1].label.starts_with("Core 0"));
        assert!(c.graphs[2].label.starts_with("Core 2"));
        assert!(c.graphs[3].label.starts_with("Core 10"));
        let reset = s.cpu_card(parse_cpu(&source(1)).unwrap()).unwrap();
        assert!(reset.graphs.iter().all(|g| g.samples.last() == Some(&None)));
        assert!(reset.graphs.iter().all(|g| g.ceiling == 100));
    }

    #[test]
    fn demo_is_deterministic_and_failures_retain_stale_only() {
        assert_eq!(demo_cards(), demo_cards());
        assert_eq!(demo_cards()[0].graphs.len(), 9);
        let mut s = Sampler::default();
        let c = card("ram", "Memory", "", vec![]);
        assert_eq!(s.retain("ram", "Memory", Ok(c)).state, CardState::Ready);
        assert_eq!(
            s.retain("ram", "Memory", Err("denied".into())).state,
            CardState::Stale
        );
        assert!(matches!(
            s.retain("cpu", "CPU", Err("denied".into())).state,
            CardState::Unavailable(_)
        ));
    }
}
