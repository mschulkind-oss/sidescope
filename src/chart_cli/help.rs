//! `sidescope charts --help` and each verb's help. The key tables come from
//! the same descriptors `charts schema` prints.

use super::args::Verb;
use crate::chart::schema::{DETAILS_DEFAULT_LIMIT, DETAILS_MAX_LIMIT, help_tables};

const COOKBOOK: &str = "docs/chart-cookbook.md";

const COMMANDS: &str = "\
COMMANDS:
  schema   Print every chart key, default, range, enum, pointer scope, payload version,
           limit, error kind and the report schema. Reads no configuration.
  check    Validate the configuration and resolve each source's request and each card's
           intent. Runs nothing, reads no data.
  replay   Normalize saved output as a source's answer and build its cards. Runs nothing.
  run      Execute exactly the named sources once and build their cards.";

fn options(verb: Option<Verb>) -> String {
    let all = [
        (
            "--config FILE",
            "Configuration to read (check, replay, run); default is the application's own file,\n                      which may be absent. An explicit file that is missing is an error.",
            &[Verb::Check, Verb::Replay, Verb::Run][..],
        ),
        (
            "--at INSTANT",
            "RFC 3339 clock, floored to whole seconds (check, replay with --data); replay has\n                      no default clock and run refuses it.",
            &[Verb::Check, Verb::Replay][..],
        ),
        (
            "--card ID",
            "Restrict card output; repeatable. Default is every card of the selected sources.",
            &[Verb::Check, Verb::Replay, Verb::Run][..],
        ),
        (
            "--data SOURCE=FILE",
            "Saved stdout of one source (replay); repeatable, at most 1 MiB each.",
            &[Verb::Replay][..],
        ),
        (
            "--from-capture DIR",
            "Replay a directory written by run --capture, with its recorded clocks (replay).",
            &[Verb::Replay][..],
        ),
        (
            "--source ID",
            "Run this source (run); repeatable. Nothing runs unless named.",
            &[Verb::Run][..],
        ),
        (
            "--all-sources",
            "Run every configured source once (run); not with --source.",
            &[Verb::Run][..],
        ),
        (
            "--capture DIR",
            "Write each exit-0 stdout byte-exact plus manifest.json, owner-only, never overwriting (run).",
            &[Verb::Run][..],
        ),
        (
            "--details CARD",
            "Include that card's Details rows, paged (replay, run).",
            &[Verb::Replay, Verb::Run][..],
        ),
        (
            "--offset N, --limit N",
            "Details page start (default 0) and size (1 to 5000, default 500); need --details.",
            &[Verb::Replay, Verb::Run][..],
        ),
        (
            "--section NAME",
            "Print one schema section only (schema).",
            &[Verb::Schema][..],
        ),
        (
            "--show-argv",
            "Include the resolved argument vector; off by default (check, replay, run).",
            &[Verb::Check, Verb::Replay, Verb::Run][..],
        ),
        (
            "--format json|text",
            "json is the contract and the default; text is a short summary of the same report.",
            &[Verb::Schema, Verb::Check, Verb::Replay, Verb::Run][..],
        ),
    ];
    let mut out = String::from("OPTIONS:\n");
    for (flag, text, verbs) in all {
        if verb.is_none_or(|verb| verbs.contains(&verb)) {
            out.push_str(&format!("  {flag:<20}{text}\n"));
        }
    }
    out
}

const EXIT_CODES: &str = "\
EXIT CODES:
  0  ok             every required check passed for a non-empty scope
  1  findings       a source failed, a dataset was refused, or a card was refused or unavailable
  2  invalid        usage, unreadable or invalid configuration, bad selection, empty scope,
                    unreadable data, capture path, or an oversized report
  3  undetermined   a required check ended unknown (for example a request still deferred)
Every verb but schema prints exactly one JSON report on stdout; exit 0 never means the chart is
what you intended, so assert the outcome you expect with jq -e.";

const AUTHORITY: &str = "\
AUTHORITY:
  `run` executes the configured command with this application's authority and inherited
  environment, unsandboxed, exactly once per named source, with the existing timeout,
  output bound, process-group kill and reap. It never syncs, installs, starts or
  authenticates anything. Raw output is never printed; it reaches disk only under --capture,
  and a capture is real data kept at your request: keep it out of version control.";

pub fn text(verb: Option<Verb>) -> String {
    let mut out = String::new();
    match verb {
        None => {
            out.push_str("sidescope charts — author and verify chart sources and cards without a display\n\n");
            out.push_str("USAGE:\n");
            for verb in [Verb::Schema, Verb::Check, Verb::Replay, Verb::Run] {
                out.push_str(&usage(verb));
            }
            out.push('\n');
            out.push_str(COMMANDS);
            out.push_str("\n\n");
        }
        Some(verb) => {
            out.push_str(&format!("sidescope charts {}\n\nUSAGE:\n", verb.name()));
            out.push_str(&usage(verb));
            out.push('\n');
        }
    }
    out.push_str(&options(verb));
    out.push('\n');
    if verb.is_none_or(|verb| verb == Verb::Run) {
        out.push_str(AUTHORITY);
        out.push_str("\n\n");
    }
    if verb != Some(Verb::Schema) {
        out.push_str(EXIT_CODES);
        out.push_str("\n\n");
    }
    if verb.is_none_or(|verb| matches!(verb, Verb::Check | Verb::Schema)) {
        out.push_str("CONFIGURATION KEYS (the same tables `charts schema` prints):\n");
        out.push_str(&help_tables());
        out.push('\n');
    }
    out.push_str(&format!(
        "Details pages hold {DETAILS_DEFAULT_LIMIT} rows by default and at most {DETAILS_MAX_LIMIT}.\n"
    ));
    out.push_str(&format!("Cookbook and troubleshooting flow: {COOKBOOK}\n"));
    out
}

fn usage(verb: Verb) -> String {
    let line = match verb {
        Verb::Schema => "sidescope charts schema [--section NAME] [--format json|text]",
        Verb::Check => {
            "sidescope charts check [--config FILE] [--at INSTANT] [--card ID]... [--show-argv] [--format json|text]"
        }
        Verb::Replay => {
            "sidescope charts replay [--config FILE] (--at INSTANT --data SOURCE=FILE... | --from-capture DIR)\n      [--card ID]... [--details CARD [--offset N] [--limit N]] [--show-argv] [--format json|text]"
        }
        Verb::Run => {
            "sidescope charts run [--config FILE] (--source ID... | --all-sources) [--capture DIR]\n      [--card ID]... [--details CARD [--offset N] [--limit N]] [--show-argv] [--format json|text]"
        }
    };
    format!("  {line}\n")
}
