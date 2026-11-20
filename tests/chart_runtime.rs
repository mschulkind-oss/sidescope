//! Process-boundary and scheduling tests for the chart runtime. Commands are
//! small synthetic `sh` scripts under `tests/fixtures/charts/runtime/`.

use sidescope::chart::config::{
    ChartSourceConfig, Normalizer, ResolvedRequest, resolve_request, validate_chart_config,
};
use sidescope::chart::dataset::{ChartDataset, parse_native_snapshot};
use sidescope::chart::error::{ChartError, ChartErrorKind};
use sidescope::chart::state::SourceSnapshot;
use sidescope::chart::time::{ChartZone, Timestamp, parse_instant};
use sidescope::chart_runtime::ChartRuntime;
use sidescope::chart_runtime::scheduler::{Action, Event, Scheduler};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MINIMAL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/charts/native/minimal.json"
);
const SECRET_ARGUMENT: &str = "--token=SECRET-ARGUMENT";

fn script(name: &str) -> String {
    format!(
        "{}/tests/fixtures/charts/runtime/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn native_only(
    _source: &ChartSourceConfig,
    stdout: &str,
    request: &ResolvedRequest,
) -> Result<ChartDataset, ChartError> {
    parse_native_snapshot(stdout, request)
}
const NORMALIZER: Normalizer = native_only;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("chart-runtime-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
    fn dir(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
    fn lines(&self, name: &str) -> usize {
        std::fs::read_to_string(self.0.join(name)).map_or(0, |text| text.lines().count())
    }
    fn pid(&self, name: &str) -> i32 {
        wait_until(Duration::from_secs(5), || {
            std::fs::read_to_string(self.0.join(name))
                .ok()
                .is_some_and(|text| text.trim().parse::<i32>().is_ok())
        });
        std::fs::read_to_string(self.0.join(name))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for name in ["child", "descendant", "pid"] {
            if let Ok(text) = std::fs::read_to_string(self.0.join(name))
                && let Ok(pid) = text.trim().parse::<i32>()
            {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source(id: &str, command: &[String], extra: &str) -> ChartSourceConfig {
    let text = format!(
        "id = \"{id}\"\ncommand = {}\nformat = \"sidescope-chart-v1\"\nwindow = \"3h\"\ntimezone = \"UTC\"\n{extra}",
        serde_json::to_string(command).unwrap()
    );
    let config: ChartSourceConfig = toml::from_str(&text).unwrap();
    validate_chart_config(std::slice::from_ref(&config), &[]).unwrap();
    config
}

fn command(name: &str, args: &[&str]) -> Vec<String> {
    let mut argv = vec!["sh".to_string(), script(name)];
    argv.extend(args.iter().map(ToString::to_string));
    argv
}

fn wait_until(limit: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    condition()
}

/// Wait for a snapshot of `id` satisfying `accept`.
fn await_snapshot(
    runtime: &ChartRuntime,
    id: &str,
    accept: impl Fn(&SourceSnapshot) -> bool,
) -> SourceSnapshot {
    let mut found = None;
    let satisfied = wait_until(Duration::from_secs(10), || {
        found = runtime
            .snapshots()
            .into_iter()
            .find(|snapshot| snapshot.source_id == id && accept(snapshot));
        found.is_some()
    });
    assert!(satisfied, "no matching snapshot for {id}");
    found.unwrap()
}

fn running(pid: i32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .is_some_and(|stat| {
            stat.rsplit_once(") ")
                .is_some_and(|(_, fields)| !fields.starts_with('Z'))
        })
}

fn assert_group_stopped_and_reaped(scratch: &Scratch) {
    let descendant = scratch.pid("descendant");
    let child = scratch.pid("child");
    assert!(
        wait_until(Duration::from_secs(2), || !running(descendant)),
        "descendant still running"
    );
    let mut status = 0;
    assert_eq!(
        unsafe { libc::waitpid(child, &mut status, libc::WNOHANG) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}

fn assert_message_is_clean(error: &ChartError) {
    let text = error.to_string();
    for secret in ["SECRET-STDOUT", "SECRET-STDERR", "SECRET-ARGUMENT"] {
        assert!(!text.contains(secret), "{text}");
    }
}

// ---- runtime through its public interface -------------------------------

#[test]
fn success_publishes_one_snapshot_with_dataset_and_last_success() {
    let scratch = Scratch::new("success");
    let config = source(
        "alpha",
        &command("emit.sh", &[&scratch.file("count"), MINIMAL]),
        "",
    );
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let snapshot = await_snapshot(&runtime, "alpha", |s| s.dataset.is_some());
    assert!(snapshot.last_error.is_none());
    assert!(snapshot.info.last_success.is_some());
    assert!(snapshot.info.last_attempt.is_some());
    assert!(snapshot.info.latency_ms.is_some());
    assert_eq!(snapshot.info.executable, "sh");
    assert_eq!(snapshot.info.timezone, "UTC");
    assert!(snapshot.info.request_window.is_some());
    assert!(snapshot.info.next_retry.is_some());
    // The first poll holds the loading and the published state at most once each.
    let polled = runtime.poll();
    assert_eq!(polled.len(), 1);
    assert!(polled[0].dataset.is_some());
    assert!(runtime.poll().is_empty(), "poll reports only changes");
}

#[test]
fn consumers_share_one_answer_per_refresh() {
    let scratch = Scratch::new("share");
    let config = source(
        "alpha",
        &command("emit.sh", &[&scratch.file("count"), MINIMAL]),
        "",
    );
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let first = await_snapshot(&runtime, "alpha", |s| s.dataset.is_some());
    let second = runtime
        .snapshots()
        .into_iter()
        .find(|s| s.source_id == "alpha")
        .unwrap();
    assert!(Arc::ptr_eq(
        first.dataset.as_ref().unwrap(),
        second.dataset.as_ref().unwrap()
    ));
    assert_eq!(scratch.lines("count"), 1, "one invocation per refresh");
}

#[test]
fn hung_command_times_out_and_its_group_is_stopped_and_reaped() {
    let scratch = Scratch::new("hang");
    let config = source(
        "hung",
        &command("hang.sh", &[&scratch.dir(), SECRET_ARGUMENT]),
        "timeout_seconds = 1\n",
    );
    let started = Instant::now();
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let snapshot = await_snapshot(&runtime, "hung", |s| s.last_error.is_some());
    let error = snapshot.last_error.unwrap();
    assert_eq!(error.kind, ChartErrorKind::Timeout);
    assert_message_is_clean(&error);
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(snapshot.dataset.is_none());
    assert_group_stopped_and_reaped(&scratch);
}

#[test]
fn stdout_over_one_mebibyte_is_an_output_bound_and_publishes_nothing() {
    let config = source("flood", &command("flood.sh", &[]), "");
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let snapshot = await_snapshot(&runtime, "flood", |s| s.last_error.is_some());
    assert_eq!(
        snapshot.last_error.unwrap().kind,
        ChartErrorKind::OutputBound
    );
    assert!(snapshot.dataset.is_none());
}

#[test]
fn a_mebibyte_of_stderr_does_not_block_or_leak_without_capture() {
    let config = source("noisy", &command("noisy-stderr.sh", &[MINIMAL]), "");
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let snapshot = await_snapshot(&runtime, "noisy", |s| s.dataset.is_some());
    assert!(snapshot.stderr_excerpt.is_none());
}

#[test]
fn captured_stderr_keeps_only_the_first_eight_kibibytes_sanitized() {
    let config = source(
        "noisy",
        &command("noisy-stderr.sh", &[MINIMAL]),
        "capture_stderr = true\n",
    );
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let snapshot = await_snapshot(&runtime, "noisy", |s| s.dataset.is_some());
    let excerpt = snapshot.stderr_excerpt.expect("opted in");
    assert!(excerpt.starts_with("[31mwarn  eee"), "{excerpt:.30}");
    assert!(!excerpt.chars().any(char::is_control));
    assert!(excerpt.chars().count() <= 8192);
    assert!(excerpt.chars().count() > 8000);
}

#[test]
fn nonzero_exit_keeps_the_prior_dataset_and_leaks_nothing() {
    let scratch = Scratch::new("nonzero");
    let config = source(
        "alpha",
        &command(
            "flaky.sh",
            &[&scratch.file("count"), MINIMAL, SECRET_ARGUMENT],
        ),
        "capture_stderr = true\n",
    );
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let good = await_snapshot(&runtime, "alpha", |s| s.dataset.is_some());
    runtime.refresh_now("alpha");
    let failed = await_snapshot(&runtime, "alpha", |s| s.last_error.is_some());
    let error = failed.last_error.as_ref().unwrap();
    assert_eq!(error.kind, ChartErrorKind::NonzeroExit);
    assert_message_is_clean(error);
    assert!(Arc::ptr_eq(
        good.dataset.as_ref().unwrap(),
        failed.dataset.as_ref().unwrap()
    ));
    assert_eq!(failed.info.last_success, good.info.last_success);
    assert_eq!(
        failed.stderr_excerpt.as_deref(),
        Some("SECRET-STDERR"),
        "opt-in capture keeps the bounded stderr of a failing run"
    );
    assert!(!failed.info.retry_suspended);
}

#[test]
fn failed_runs_discard_stderr_unless_capture_is_opted_in() {
    let scratch = Scratch::new("nonzero-default");
    let config = source(
        "alpha",
        &command("flaky.sh", &[&scratch.file("count"), MINIMAL]),
        "",
    );
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    await_snapshot(&runtime, "alpha", |s| s.dataset.is_some());
    runtime.refresh_now("alpha");
    let failed = await_snapshot(&runtime, "alpha", |s| s.last_error.is_some());
    assert!(failed.stderr_excerpt.is_none());
}

#[test]
fn capture_on_a_failing_command_that_floods_stderr_stays_bounded_and_does_not_deadlock() {
    let scratch = Scratch::new("flood-fail");
    let config = source(
        "noisy",
        &command("noisy-stderr.sh", &["/definitely/missing.json"]),
        "capture_stderr = true\ntimeout_seconds = 20\n",
    );
    let _ = &scratch;
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let failed = await_snapshot(&runtime, "noisy", |s| s.last_error.is_some());
    assert!(
        failed
            .stderr_excerpt
            .as_ref()
            .is_none_or(|text| text.chars().count() <= 8192)
    );
    assert!(
        failed
            .stderr_excerpt
            .iter()
            .all(|text| !text.contains('\u{1b}'))
    );
}

#[test]
fn a_deferred_window_is_not_a_failure_and_keeps_the_source_schedulable() {
    let mut scheduler = Scheduler::new(&[sched_source("alpha", "")], 4);
    let actions = scheduler.step(secs(0), Event::Tick);
    let revision = starts(&actions)[0].1;
    let actions = scheduler.step(
        secs(0),
        Event::Deferred {
            source: "alpha".into(),
            revision,
            retry_after: secs(1),
        },
    );
    assert!(starts(&actions).is_empty());
    assert!(!scheduler.is_suspended("alpha"));
    assert_eq!(scheduler.next_due("alpha"), Some(secs(1)));
    let actions = scheduler.step(secs(1), Event::Tick);
    assert_eq!(starts(&actions).len(), 1, "retried one second later");
    // A later real failure still starts the backoff ladder at its first step.
    let revision = starts(&actions)[0].1;
    finish(
        &mut scheduler,
        secs(1),
        "alpha",
        revision,
        failure(ChartErrorKind::Timeout),
    );
    assert_eq!(scheduler.next_due("alpha"), Some(secs(1) + secs(30)));
}

#[test]
fn one_day_calendar_window_at_local_midnight_is_deferred_not_invalid() {
    use sidescope::chart::config::{RequestError, resolve_request_at};
    let config = calendar_source("UTC", 1);
    for instant in ["2026-11-20T00:00:00Z", "2026-11-20T00:00:00.900Z"] {
        match resolve_request_at(&config, at(instant), 1) {
            Err(RequestError::Deferred { retry_after }) => assert_eq!(retry_after, secs(1)),
            other => panic!("{instant}: {other:?}"),
        }
    }
    let request = resolve_request_at(&config, at("2026-11-20T00:00:01Z"), 1).unwrap();
    assert_eq!(request.window.seconds(), 1);
    // A genuinely invalid configuration stays invalid.
    let mut bad = calendar_source("UTC", 1);
    bad.timezone = "Not/AZone".into();
    assert!(matches!(
        resolve_request_at(&bad, at("2026-11-20T00:00:00Z"), 1),
        Err(RequestError::Invalid(_))
    ));
}

#[test]
fn exit_zero_with_malformed_json_is_a_parse_failure_not_an_empty_dataset() {
    let scratch = Scratch::new("garbage");
    let config = source(
        "alpha",
        &command("garbage.sh", &[&scratch.file("count"), MINIMAL]),
        "",
    );
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let good = await_snapshot(&runtime, "alpha", |s| s.dataset.is_some());
    runtime.refresh_now("alpha");
    let bad = await_snapshot(&runtime, "alpha", |s| s.last_error.is_some());
    let error = bad.last_error.as_ref().unwrap();
    assert_eq!(error.kind, ChartErrorKind::ParseFailure);
    assert_message_is_clean(error);
    assert!(Arc::ptr_eq(
        good.dataset.as_ref().unwrap(),
        bad.dataset.as_ref().unwrap()
    ));
}

#[test]
fn non_utf8_output_is_a_parse_failure() {
    let config = source(
        "binary",
        &["printf".to_string(), "\\377\\376".to_string()],
        "",
    );
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    let snapshot = await_snapshot(&runtime, "binary", |s| s.last_error.is_some());
    assert_eq!(
        snapshot.last_error.unwrap().kind,
        ChartErrorKind::ParseFailure
    );
    assert!(snapshot.dataset.is_none());
}

#[test]
fn missing_executable_suspends_retry_until_retry_or_reconfigure() {
    let missing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-such-chart-command");
    let config = source(
        "gone",
        &[
            missing.to_string_lossy().into_owned(),
            SECRET_ARGUMENT.into(),
        ],
        "",
    );
    let runtime = ChartRuntime::start(std::slice::from_ref(&config), NORMALIZER);
    let snapshot = await_snapshot(&runtime, "gone", |s| s.last_error.is_some());
    let error = snapshot.last_error.as_ref().unwrap();
    assert_eq!(error.kind, ChartErrorKind::MissingExecutable);
    assert_message_is_clean(error);
    assert!(snapshot.info.retry_suspended);
    assert!(snapshot.info.next_retry.is_none());
    let first_attempt = snapshot.info.last_attempt;
    runtime.poll();
    std::thread::sleep(Duration::from_millis(300));
    assert!(runtime.poll().is_empty(), "no automatic retry");
    runtime.retry("gone");
    let again = await_snapshot(&runtime, "gone", |s| s.info.last_attempt != first_attempt);
    assert!(again.info.retry_suspended);
    // Settings change resumes too, with a new revision and an empty cache.
    let mut changed = config.clone();
    changed.timeout_seconds = Some(5);
    runtime.reconfigure(&[changed]);
    let after = await_snapshot(&runtime, "gone", |s| {
        s.info.revision != again.info.revision && s.last_error.is_some()
    });
    assert!(after.info.retry_suspended);
}

#[test]
fn unchanged_reconfigure_does_not_refetch_and_changed_one_clears_the_cache() {
    let scratch = Scratch::new("reconfigure");
    let config = source(
        "alpha",
        &command("emit.sh", &[&scratch.file("count"), MINIMAL]),
        "",
    );
    let runtime = ChartRuntime::start(std::slice::from_ref(&config), NORMALIZER);
    let first = await_snapshot(&runtime, "alpha", |s| s.dataset.is_some());
    runtime.poll();
    runtime.reconfigure(std::slice::from_ref(&config));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        scratch.lines("count"),
        1,
        "unchanged entry is not refetched"
    );
    assert!(runtime.poll().is_empty());
    let mut changed = config.clone();
    changed.window = Some("1h".into());
    runtime.reconfigure(&[changed]);
    let second = await_snapshot(&runtime, "alpha", |s| {
        s.info.revision != first.info.revision && s.dataset.is_some()
    });
    assert_eq!(scratch.lines("count"), 2);
    assert!(!Arc::ptr_eq(
        first.dataset.as_ref().unwrap(),
        second.dataset.as_ref().unwrap()
    ));
    let window = second.info.request_window.unwrap();
    assert_eq!(window.seconds(), 3600);
}

#[test]
fn reconfigure_cancels_the_in_flight_command_and_publishes_only_the_new_revision() {
    let scratch = Scratch::new("obsolete");
    let gate = scratch.file("gate");
    let old = source(
        "alpha",
        &command("gate.sh", &[&gate, MINIMAL, &scratch.file("pid")]),
        "",
    );
    let runtime = ChartRuntime::start(std::slice::from_ref(&old), NORMALIZER);
    let old_pid = scratch.pid("pid");
    let old_revision = runtime.snapshots()[0].info.revision;
    let mut changed = old.clone();
    changed.window = Some("2h".into());
    runtime.reconfigure(&[changed]);
    assert!(
        wait_until(Duration::from_secs(3), || !running(old_pid)),
        "cancelled command still running"
    );
    std::fs::write(&gate, "open").unwrap();
    let published = await_snapshot(&runtime, "alpha", |s| s.dataset.is_some());
    assert_ne!(published.info.revision, old_revision);
    assert_eq!(published.info.request_window.unwrap().seconds(), 7200);
}

#[test]
fn six_sources_never_run_more_than_four_commands_at_once() {
    let scratch = Scratch::new("concurrency");
    let sources: Vec<ChartSourceConfig> = (0..6)
        .map(|index| {
            source(
                &format!("source-{index}"),
                &command("concurrency.sh", &[&scratch.dir(), MINIMAL]),
                "",
            )
        })
        .collect();
    let runtime = ChartRuntime::start(&sources, NORMALIZER);
    for index in 0..6 {
        await_snapshot(&runtime, &format!("source-{index}"), |s| {
            s.dataset.is_some()
        });
    }
    let seen: Vec<usize> = std::fs::read_to_string(scratch.0.join("high-water"))
        .unwrap()
        .lines()
        .map(|line| line.trim().parse().unwrap())
        .collect();
    assert_eq!(seen.len(), 6);
    assert_eq!(*seen.iter().max().unwrap(), 4, "high water {seen:?}");
}

#[test]
fn manual_refresh_while_running_runs_exactly_one_more_time() {
    let scratch = Scratch::new("coalesce");
    let gate = scratch.file("gate");
    let config = source(
        "alpha",
        &command("gate.sh", &[&gate, MINIMAL, &scratch.file("pid")]),
        "",
    );
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    scratch.pid("pid");
    for _ in 0..10 {
        runtime.refresh_now("alpha");
    }
    std::fs::write(&gate, "open").unwrap();
    await_snapshot(&runtime, "alpha", |s| s.dataset.is_some());
    // The queued refresh restarts the command exactly once.
    assert!(wait_until(Duration::from_secs(5), || scratch
        .lines("pid.runs")
        == 2));
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(scratch.lines("pid.runs"), 2);
}

#[test]
fn shutdown_and_drop_stop_and_reap_running_commands() {
    for explicit in [true, false] {
        let scratch = Scratch::new(if explicit { "shutdown" } else { "drop" });
        let config = source("hung", &command("hang.sh", &[&scratch.dir()]), "");
        let runtime = ChartRuntime::start(&[config], NORMALIZER);
        scratch.pid("descendant");
        if explicit {
            runtime.shutdown();
        } else {
            drop(runtime);
        }
        assert_group_stopped_and_reaped(&scratch);
    }
}

const HELPER_ENV: &str = "SIDESCOPE_CHART_RUNTIME_HELPER_DIR";

/// Re-executed by `owner_death_takes_the_command_with_it`; does nothing otherwise.
#[test]
fn helper_owner_process() {
    let Ok(dir) = std::env::var(HELPER_ENV) else {
        return;
    };
    let config = source(
        "held",
        &command("hold.sh", &[&format!("{dir}/pid")]),
        "timeout_seconds = 60\n",
    );
    let _runtime = ChartRuntime::start(&[config], NORMALIZER);
    std::thread::sleep(Duration::from_secs(120));
}

#[test]
fn owner_death_takes_the_command_with_it() {
    let scratch = Scratch::new("owner");
    let mut owner = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "helper_owner_process",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HELPER_ENV, scratch.dir())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let held = scratch.pid("pid");
    assert!(running(held));
    unsafe {
        libc::kill(owner.id() as libc::pid_t, libc::SIGKILL);
    }
    owner.wait().unwrap();
    assert!(
        wait_until(Duration::from_secs(3), || !running(held)),
        "command outlived its killed owner"
    );
}

#[test]
fn runtime_source_is_toolkit_free() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![root.join("chart_runtime.rs")];
    for entry in std::fs::read_dir(root.join("chart_runtime")).unwrap() {
        files.push(entry.unwrap().path());
    }
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for toolkit in ["gtk", "glib", "adw"] {
            assert!(
                !text.contains(toolkit),
                "{} mentions {toolkit}",
                file.display()
            );
        }
    }
}

// ---- request construction, calendar windows and zones ---------------------

fn calendar_source(zone: &str, days: u16) -> ChartSourceConfig {
    let text = format!(
        "id = \"cal\"\ncommand = [\"example-chart-data\", \"--from\", \"{{from}}\", \"--to\", \"{{to}}\", \"--timezone\", \"{{timezone}}\", \"--fixed\"]\nformat = \"sidescope-chart-v1\"\ncalendar_days = {days}\ntimezone = \"{zone}\"\n"
    );
    toml::from_str(&text).unwrap()
}

fn at(text: &str) -> Timestamp {
    parse_instant(text).unwrap()
}

#[test]
fn request_substitutes_one_argument_per_placeholder_and_truncates_to_seconds() {
    let config = calendar_source("UTC", 1);
    let request = resolve_request(&config, at("2026-06-01T10:20:30.987Z"), 7).unwrap();
    assert_eq!(
        request.argv,
        [
            "example-chart-data",
            "--from",
            "2026-06-01T00:00:00Z",
            "--to",
            "2026-06-01T10:20:30Z",
            "--timezone",
            "UTC",
            "--fixed"
        ]
    );
    assert_eq!(request.revision, 7);
}

#[test]
fn calendar_windows_follow_local_midnight_across_dst() {
    // Fall back: 2026-11-01 lasts 25 hours in New York.
    let fall = resolve_request(
        &calendar_source("America/New_York", 2),
        at("2026-11-02T15:00:00Z"),
        1,
    )
    .unwrap();
    assert_eq!(fall.argv[2], "2026-11-01T04:00:00Z");
    assert_eq!(fall.window.seconds(), (25 + 10) * 3600);
    // Spring forward: 2026-03-08 lasts 23 hours.
    let spring = resolve_request(
        &calendar_source("America/New_York", 2),
        at("2026-03-09T15:00:00Z"),
        1,
    )
    .unwrap();
    assert_eq!(spring.argv[2], "2026-03-08T05:00:00Z");
    assert_eq!(spring.window.seconds(), (23 + 11) * 3600);
    // A zone east of UTC: local today starts on the previous UTC day.
    let tokyo = resolve_request(
        &calendar_source("Asia/Tokyo", 1),
        at("2026-06-01T20:00:00Z"),
        1,
    )
    .unwrap();
    assert_eq!(tokyo.argv[2], "2026-06-01T15:00:00Z");
    assert_eq!(tokyo.argv[6], "Asia/Tokyo");
}

#[test]
fn fixed_windows_are_exact_durations_even_across_a_dst_gap() {
    let mut config = calendar_source("America/New_York", 1);
    config.calendar_days = None;
    config.window = Some("3h".into());
    let request = resolve_request(&config, at("2026-03-08T07:30:00Z"), 1).unwrap();
    assert_eq!(request.argv[2], "2026-03-08T04:30:00Z");
    assert_eq!(request.window.seconds(), 3 * 3600);
}

#[test]
fn invalid_zone_or_window_is_a_configuration_error_without_running_anything() {
    let mut config = calendar_source("UTC", 1);
    config.timezone = "Not/AZone".into();
    let error = resolve_request(&config, at("2026-06-01T00:00:00Z"), 1).unwrap_err();
    assert_eq!(error.kind, ChartErrorKind::ConfigInvalid);
    assert!(ChartErrorKind::ConfigInvalid.suspends_retry());
}

#[test]
fn runtime_passes_the_resolved_window_to_the_command() {
    let scratch = Scratch::new("args");
    let mut config = source(
        "cal",
        &command(
            "record-args.sh",
            &[
                &scratch.file("args"),
                MINIMAL,
                "{from}",
                "{to}",
                "{timezone}",
            ],
        ),
        "",
    );
    config.window = None;
    config.calendar_days = Some(2);
    config.timezone = "America/New_York".into();
    let runtime = ChartRuntime::start(&[config], NORMALIZER);
    await_snapshot(&runtime, "cal", |s| s.dataset.is_some());
    let text = std::fs::read_to_string(scratch.0.join("args")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[2], "America/New_York");
    let (from, to) = (at(lines[0]), at(lines[1]));
    let zone = ChartZone::parse("America/New_York").unwrap();
    let local = zone.local(from);
    assert_eq!((local.hour, local.minute), (0, 0), "from is local midnight");
    assert!(Timestamp::now().as_second() - to.as_second() < 30);
}

// ---- pure scheduler with injected monotonic time ------------------------

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn sched_source(id: &str, extra: &str) -> ChartSourceConfig {
    source(id, &["true".to_string()], extra)
}

fn starts(actions: &[Action]) -> Vec<(String, u64)> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Start { source, revision } => Some((source.clone(), *revision)),
            _ => None,
        })
        .collect()
}

fn failure(kind: ChartErrorKind) -> Result<(), ChartError> {
    Err(ChartError::new(kind, "synthetic"))
}

fn finish(
    scheduler: &mut Scheduler,
    now: Duration,
    source: &str,
    revision: u64,
    outcome: Result<(), ChartError>,
) -> Vec<Action> {
    scheduler.step(
        now,
        Event::Finished {
            source: source.into(),
            revision,
            outcome,
        },
    )
}

/// Runs the first Start, then fails every attempt exactly when it is due.
fn failure_delays(refresh: &str, failures: usize) -> Vec<u64> {
    let mut scheduler = Scheduler::new(&[sched_source("a", refresh)], 4);
    let mut now = secs(0);
    let mut started = starts(&scheduler.step(now, Event::Tick));
    let mut delays = Vec::new();
    for _ in 0..failures {
        let (id, revision) = started.pop().expect("a run is in flight");
        finish(
            &mut scheduler,
            now,
            &id,
            revision,
            failure(ChartErrorKind::NonzeroExit),
        );
        let due = scheduler.next_due("a").unwrap();
        delays.push((due - now).as_secs());
        assert_eq!(scheduler.next_wake(), Some(due));
        now = due;
        started = starts(&scheduler.step(now, Event::Tick));
        assert_eq!(started.len(), 1);
    }
    delays
}

#[test]
fn backoff_is_30_60_120_then_300_and_never_below_the_interval() {
    assert_eq!(failure_delays("", 6), [30, 60, 120, 300, 300, 300]);
    assert_eq!(failure_delays("refresh_seconds = 45\n", 3), [45, 60, 120]);
    assert_eq!(
        failure_delays("refresh_seconds = 600\n", 3),
        [600, 600, 600]
    );
}

#[test]
fn success_resets_the_backoff() {
    let mut scheduler = Scheduler::new(&[sched_source("a", "")], 4);
    let mut started = starts(&scheduler.step(secs(0), Event::Tick));
    let (_, revision) = started.pop().unwrap();
    finish(
        &mut scheduler,
        secs(0),
        "a",
        revision,
        failure(ChartErrorKind::Timeout),
    );
    finish_cycle(
        &mut scheduler,
        secs(30),
        revision,
        failure(ChartErrorKind::Timeout),
    );
    assert_eq!(scheduler.next_due("a"), Some(secs(30 + 60)));
    finish_cycle(&mut scheduler, secs(90), revision, Ok(()));
    assert_eq!(
        scheduler.next_due("a"),
        Some(secs(90 + 30)),
        "interval after success"
    );
    finish_cycle(
        &mut scheduler,
        secs(120),
        revision,
        failure(ChartErrorKind::Timeout),
    );
    assert_eq!(
        scheduler.next_due("a"),
        Some(secs(120 + 30)),
        "backoff restarted"
    );
}

/// Starts the due run at `now` and finishes it at `now`.
fn finish_cycle(
    scheduler: &mut Scheduler,
    now: Duration,
    revision: u64,
    outcome: Result<(), ChartError>,
) {
    assert_eq!(starts(&scheduler.step(now, Event::Tick)).len(), 1);
    finish(scheduler, now, "a", revision, outcome);
}

#[test]
fn configuration_errors_and_missing_executables_suspend_scheduled_retry() {
    for kind in [
        ChartErrorKind::ConfigInvalid,
        ChartErrorKind::MissingExecutable,
    ] {
        let mut scheduler = Scheduler::new(&[sched_source("a", "")], 4);
        let (_, revision) = starts(&scheduler.step(secs(0), Event::Tick))[0].clone();
        let actions = finish(&mut scheduler, secs(0), "a", revision, failure(kind));
        assert_eq!(actions, [Action::Publish { source: "a".into() }]);
        assert!(scheduler.is_suspended("a"));
        assert_eq!(scheduler.next_wake(), None);
        assert!(starts(&scheduler.step(secs(100_000), Event::Tick)).is_empty());
        // Owner request resumes.
        let resumed = scheduler.step(secs(100_001), Event::RetryRequested { source: "a".into() });
        assert_eq!(starts(&resumed).len(), 1);
        assert!(!scheduler.is_suspended("a"));
    }
    // So does a settings change.
    let mut scheduler = Scheduler::new(&[sched_source("a", "")], 4);
    let (_, revision) = starts(&scheduler.step(secs(0), Event::Tick))[0].clone();
    finish(
        &mut scheduler,
        secs(0),
        "a",
        revision,
        failure(ChartErrorKind::MissingExecutable),
    );
    let changed = sched_source("a", "timeout_seconds = 3\n");
    let actions = scheduler.step(
        secs(5),
        Event::Reconfigured {
            sources: vec![changed],
        },
    );
    assert_eq!(starts(&actions).len(), 1);
    assert!(!scheduler.is_suspended("a"));
}

#[test]
fn runtime_failures_never_suspend() {
    for kind in [
        ChartErrorKind::SpawnFailed,
        ChartErrorKind::NonzeroExit,
        ChartErrorKind::Timeout,
        ChartErrorKind::OutputBound,
        ChartErrorKind::ParseFailure,
        ChartErrorKind::InvalidDataset,
        ChartErrorKind::UnsupportedVersion,
        ChartErrorKind::InvalidMapping,
    ] {
        let mut scheduler = Scheduler::new(&[sched_source("a", "")], 4);
        let (_, revision) = starts(&scheduler.step(secs(0), Event::Tick))[0].clone();
        finish(&mut scheduler, secs(0), "a", revision, failure(kind));
        assert!(!scheduler.is_suspended("a"), "{kind:?}");
        assert_eq!(scheduler.next_due("a"), Some(secs(30)));
    }
}

#[test]
fn repeated_refresh_requests_while_in_flight_queue_exactly_one_run() {
    let mut scheduler = Scheduler::new(&[sched_source("a", "")], 4);
    let (_, revision) = starts(&scheduler.step(secs(0), Event::Tick))[0].clone();
    for at in 1..=5 {
        let actions = scheduler.step(secs(at), Event::RefreshRequested { source: "a".into() });
        assert!(actions.is_empty(), "never a second concurrent run");
    }
    let actions = finish(&mut scheduler, secs(6), "a", revision, Ok(()));
    assert_eq!(actions[0], Action::Publish { source: "a".into() });
    assert_eq!(starts(&actions), [("a".to_string(), revision)]);
    // That run finishes: nothing further is queued.
    let after = finish(&mut scheduler, secs(7), "a", revision, Ok(()));
    assert!(starts(&after).is_empty());
    assert_eq!(scheduler.next_due("a"), Some(secs(37)));
}

#[test]
fn refresh_while_idle_runs_at_once_and_overrides_backoff() {
    let mut scheduler = Scheduler::new(&[sched_source("a", "")], 4);
    let (_, revision) = starts(&scheduler.step(secs(0), Event::Tick))[0].clone();
    finish(
        &mut scheduler,
        secs(0),
        "a",
        revision,
        failure(ChartErrorKind::Timeout),
    );
    let actions = scheduler.step(secs(2), Event::RefreshRequested { source: "a".into() });
    assert_eq!(starts(&actions).len(), 1);
}

#[test]
fn reconfigure_cancels_bumps_the_revision_and_drops_the_obsolete_answer() {
    let old = sched_source("a", "");
    let mut scheduler = Scheduler::new(std::slice::from_ref(&old), 4);
    let (_, old_revision) = starts(&scheduler.step(secs(0), Event::Tick))[0].clone();
    let mut changed = old.clone();
    changed.window = Some("1h".into());
    let actions = scheduler.step(
        secs(1),
        Event::Reconfigured {
            sources: vec![changed],
        },
    );
    assert_eq!(
        actions,
        [Action::Cancel {
            source: "a".into(),
            revision: old_revision
        }],
        "the new run waits for the old worker"
    );
    let new_revision = scheduler.revision("a").unwrap();
    assert!(new_revision > old_revision);
    // The old worker finishes after the revision moved on: no publish, new run starts.
    let actions = finish(&mut scheduler, secs(2), "a", old_revision, Ok(()));
    assert!(!actions.iter().any(|a| matches!(a, Action::Publish { .. })));
    assert_eq!(starts(&actions), [("a".to_string(), new_revision)]);
    // A duplicate or unknown answer is ignored too.
    assert!(finish(&mut scheduler, secs(3), "a", old_revision, Ok(())).is_empty());
    assert!(finish(&mut scheduler, secs(3), "nobody", 99, Ok(())).is_empty());
}

#[test]
fn completion_before_reconfigure_is_published_then_the_new_revision_starts_fresh() {
    let old = sched_source("a", "");
    let mut scheduler = Scheduler::new(std::slice::from_ref(&old), 4);
    let (_, old_revision) = starts(&scheduler.step(secs(0), Event::Tick))[0].clone();
    let actions = finish(&mut scheduler, secs(1), "a", old_revision, Ok(()));
    assert_eq!(actions, [Action::Publish { source: "a".into() }]);
    let mut changed = old.clone();
    changed.window = Some("1h".into());
    let actions = scheduler.step(
        secs(2),
        Event::Reconfigured {
            sources: vec![changed],
        },
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Cancel { .. })));
    assert_eq!(
        starts(&actions),
        [("a".to_string(), scheduler.revision("a").unwrap())]
    );
}

#[test]
fn unchanged_reconfigure_starts_nothing_and_keeps_the_revision() {
    let source = sched_source("a", "");
    let mut scheduler = Scheduler::new(std::slice::from_ref(&source), 4);
    let (_, revision) = starts(&scheduler.step(secs(0), Event::Tick))[0].clone();
    finish(&mut scheduler, secs(0), "a", revision, Ok(()));
    let actions = scheduler.step(
        secs(1),
        Event::Reconfigured {
            sources: vec![source],
        },
    );
    assert!(actions.is_empty());
    assert_eq!(scheduler.revision("a"), Some(revision));
    assert_eq!(scheduler.next_due("a"), Some(secs(30)));
}

#[test]
fn removed_sources_are_cancelled_and_added_ones_start() {
    let a = sched_source("a", "");
    let mut scheduler = Scheduler::new(std::slice::from_ref(&a), 4);
    let (_, revision) = starts(&scheduler.step(secs(0), Event::Tick))[0].clone();
    let b = sched_source("b", "");
    let actions = scheduler.step(secs(1), Event::Reconfigured { sources: vec![b] });
    assert!(actions.contains(&Action::Cancel {
        source: "a".into(),
        revision
    }));
    assert_eq!(starts(&actions).len(), 1);
    assert_eq!(scheduler.revision("a"), None);
    assert!(finish(&mut scheduler, secs(2), "a", revision, Ok(())).is_empty());
}

#[test]
fn six_sources_never_exceed_four_concurrent_starts() {
    let sources: Vec<_> = (0..6).map(|i| sched_source(&format!("s{i}"), "")).collect();
    let mut scheduler = Scheduler::new(&sources, 4);
    let mut running: Vec<(String, u64)> = starts(&scheduler.step(secs(0), Event::Tick));
    assert_eq!(running.len(), 4);
    assert_eq!(
        scheduler.next_wake(),
        None,
        "busy slots wake only on completion"
    );
    let mut high_water = running.len();
    let mut finished = 0;
    while let Some((id, revision)) = running.pop() {
        finished += 1;
        let actions = finish(&mut scheduler, secs(1), &id, revision, Ok(()));
        running.extend(starts(&actions));
        high_water = high_water.max(running.len());
        assert!(running.len() <= 4);
        if finished > 20 {
            break;
        }
    }
    assert_eq!(high_water, 4);
    assert_eq!(
        finished, 6,
        "every source ran once before the next interval"
    );
}

#[test]
fn next_wake_reports_the_earliest_due_source() {
    let sources = [
        sched_source("a", ""),
        sched_source("b", "refresh_seconds = 60\n"),
    ];
    let mut scheduler = Scheduler::new(&sources, 4);
    assert_eq!(scheduler.next_wake(), Some(secs(0)));
    let started = starts(&scheduler.step(secs(0), Event::Tick));
    assert_eq!(scheduler.next_wake(), None);
    for (id, revision) in started {
        finish(&mut scheduler, secs(10), &id, revision, Ok(()));
    }
    assert_eq!(scheduler.next_wake(), Some(secs(40)));
}
