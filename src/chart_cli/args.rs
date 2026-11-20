//! Argument parsing for `sidescope charts`. Pure: nothing is read or run here.

use crate::chart::schema::{DETAILS_DEFAULT_LIMIT, DETAILS_MAX_LIMIT, section_names};
use crate::chart::time::{Timestamp, parse_instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Schema,
    Check,
    Replay,
    Run,
}

impl Verb {
    pub fn name(self) -> &'static str {
        match self {
            Verb::Schema => "schema",
            Verb::Check => "check",
            Verb::Replay => "replay",
            Verb::Run => "run",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        [Verb::Schema, Verb::Check, Verb::Replay, Verb::Run]
            .into_iter()
            .find(|verb| verb.name() == name)
    }

    /// Flags this verb takes, besides `--format` and `--help`.
    fn flags(self) -> &'static [&'static str] {
        match self {
            Verb::Schema => &["--section"],
            Verb::Check => &["--config", "--at", "--card", "--show-argv"],
            Verb::Replay => &[
                "--config",
                "--at",
                "--data",
                "--from-capture",
                "--card",
                "--details",
                "--offset",
                "--limit",
                "--show-argv",
            ],
            Verb::Run => &[
                "--config",
                "--source",
                "--all-sources",
                "--capture",
                "--card",
                "--details",
                "--offset",
                "--limit",
                "--show-argv",
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsRequest {
    pub card: String,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct Invocation {
    pub verb: Verb,
    pub format: Format,
    pub config: Option<String>,
    pub at: Option<Timestamp>,
    pub cards: Vec<String>,
    pub sources: Vec<String>,
    pub all_sources: bool,
    /// `(source id, file)` in the order given.
    pub data: Vec<(String, String)>,
    pub from_capture: Option<String>,
    pub details: Option<DetailsRequest>,
    pub capture: Option<String>,
    pub show_argv: bool,
    pub section: Option<String>,
}

pub enum Parsed {
    /// Print help text: for the family (`None`) or one verb.
    Help(Option<Verb>),
    Invocation(Box<Invocation>),
}

/// A usage failure, still printed as one report.
#[derive(Debug)]
pub struct UsageError {
    pub verb: Option<Verb>,
    pub format: Format,
    pub text: String,
}

fn at_second(text: &str) -> Result<Timestamp, String> {
    let instant = parse_instant(text).map_err(|error| format!("--at {text:?}: {error}"))?;
    Timestamp::from_second(instant.as_second())
        .map_err(|_| format!("--at {text:?}: instant is out of range"))
}

pub fn parse(args: &[String]) -> Result<Parsed, UsageError> {
    let format = if args
        .windows(2)
        .any(|pair| pair[0] == "--format" && pair[1] == "text")
    {
        Format::Text
    } else {
        Format::Json
    };
    let fail = |verb: Option<Verb>, text: String| UsageError { verb, format, text };
    let Some(first) = args.first() else {
        return Err(fail(
            None,
            "charts needs a verb: schema, check, replay or run".into(),
        ));
    };
    if first == "--help" || first == "-h" {
        return Ok(Parsed::Help(None));
    }
    let Some(verb) = Verb::from_name(first) else {
        return Err(fail(
            None,
            format!("unknown charts verb {first:?}: use schema, check, replay or run"),
        ));
    };
    let fail = |text: String| fail(Some(verb), text);
    let mut invocation = Invocation {
        verb,
        format,
        config: None,
        at: None,
        cards: Vec::new(),
        sources: Vec::new(),
        all_sources: false,
        data: Vec::new(),
        from_capture: None,
        details: None,
        capture: None,
        show_argv: false,
        section: None,
    };
    let mut details_card: Option<String> = None;
    let mut offset: Option<usize> = None;
    let mut limit: Option<usize> = None;
    let mut seen_format = false;
    let mut iter = args[1..].iter();
    while let Some(flag) = iter.next() {
        if flag == "--help" || flag == "-h" {
            return Ok(Parsed::Help(Some(verb)));
        }
        if flag != "--format" && !verb.flags().contains(&flag.as_str()) {
            return Err(fail(format!(
                "sidescope charts {} does not take {flag}",
                verb.name()
            )));
        }
        let mut value = |what: &str| -> Result<String, UsageError> {
            iter.next()
                .cloned()
                .ok_or_else(|| fail(format!("{flag} needs {what}")))
        };
        let repeated = |name: &str| fail(format!("{name} given more than once"));
        match flag.as_str() {
            "--format" => {
                let text = value("json or text")?;
                if std::mem::replace(&mut seen_format, true) {
                    return Err(repeated("--format"));
                }
                match text.as_str() {
                    "json" | "text" => {}
                    other => return Err(fail(format!("--format {other:?}: use json or text"))),
                }
            }
            "--config" => {
                let path = value("a file path")?;
                if invocation.config.replace(path).is_some() {
                    return Err(repeated("--config"));
                }
            }
            "--at" => {
                let text = value("an instant")?;
                let at = at_second(&text).map_err(&fail)?;
                if invocation.at.replace(at).is_some() {
                    return Err(repeated("--at"));
                }
            }
            "--card" => {
                let id = value("a card id")?;
                if invocation.cards.contains(&id) {
                    return Err(fail(format!("--card {id} given more than once")));
                }
                invocation.cards.push(id);
            }
            "--source" => {
                let id = value("a source id")?;
                if invocation.sources.contains(&id) {
                    return Err(fail(format!("--source {id} given more than once")));
                }
                invocation.sources.push(id);
            }
            "--all-sources" => {
                if std::mem::replace(&mut invocation.all_sources, true) {
                    return Err(repeated("--all-sources"));
                }
            }
            "--data" => {
                let pair = value("SOURCE=FILE")?;
                let Some((id, file)) = pair
                    .split_once('=')
                    .filter(|(a, b)| !a.is_empty() && !b.is_empty())
                else {
                    return Err(fail(format!("--data {pair:?}: use SOURCE=FILE")));
                };
                if invocation.data.iter().any(|(seen, _)| seen == id) {
                    return Err(fail(format!("--data {id} given more than once")));
                }
                invocation.data.push((id.to_string(), file.to_string()));
            }
            "--from-capture" => {
                let dir = value("a directory")?;
                if invocation.from_capture.replace(dir).is_some() {
                    return Err(repeated("--from-capture"));
                }
            }
            "--capture" => {
                let dir = value("a directory")?;
                if invocation.capture.replace(dir).is_some() {
                    return Err(repeated("--capture"));
                }
            }
            "--details" => {
                let id = value("a card id")?;
                if details_card.replace(id).is_some() {
                    return Err(repeated("--details"));
                }
            }
            "--offset" => {
                let text = value("a row number")?;
                let number: usize = text
                    .parse()
                    .map_err(|_| fail(format!("--offset {text:?}: use a number from 0")))?;
                if offset.replace(number).is_some() {
                    return Err(repeated("--offset"));
                }
            }
            "--limit" => {
                let text = value("a row count")?;
                let number: usize = text
                    .parse()
                    .ok()
                    .filter(|n| (1..=DETAILS_MAX_LIMIT).contains(n))
                    .ok_or_else(|| {
                        fail(format!("--limit {text:?}: use 1 to {DETAILS_MAX_LIMIT}"))
                    })?;
                if limit.replace(number).is_some() {
                    return Err(repeated("--limit"));
                }
            }
            "--show-argv" => invocation.show_argv = true,
            "--section" => {
                let name = value("a section name")?;
                if !section_names().contains(&name.as_str()) {
                    return Err(fail(format!(
                        "--section {name:?}: use one of {}",
                        section_names().join(", ")
                    )));
                }
                if invocation.section.replace(name).is_some() {
                    return Err(repeated("--section"));
                }
            }
            other => return Err(fail(format!("unexpected argument {other:?}"))),
        }
    }
    match details_card {
        Some(card) => {
            invocation.details = Some(DetailsRequest {
                card,
                offset: offset.unwrap_or(0),
                limit: limit.unwrap_or(DETAILS_DEFAULT_LIMIT),
            });
        }
        None if offset.is_some() || limit.is_some() => {
            return Err(fail("--offset and --limit need --details".into()));
        }
        None => {}
    }
    match verb {
        Verb::Run if invocation.sources.is_empty() && !invocation.all_sources => {
            return Err(fail(
                "run executes only what is named: give --source ID or --all-sources".into(),
            ));
        }
        Verb::Replay if invocation.data.is_empty() && invocation.from_capture.is_none() => {
            return Err(fail(
                "replay needs saved output: give --data SOURCE=FILE or --from-capture DIR".into(),
            ));
        }
        Verb::Replay
            if !invocation.data.is_empty()
                && invocation.at.is_none()
                && invocation.from_capture.is_none() =>
        {
            return Err(fail(
                "replay has no default clock: give --at INSTANT with --data".into(),
            ));
        }
        _ => {}
    }
    Ok(Parsed::Invocation(Box::new(invocation)))
}
