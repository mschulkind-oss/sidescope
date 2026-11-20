//! Builds the report from per-source evaluations and card facts. Cards are
//! reduced by `reduce_card`; checks are combined here and nowhere else.

use super::args::{DetailsRequest, Verb};
use super::eval::{SourceEval, Step, blank_snapshot};
use super::report::{
    AttemptBlock, CardBlock, CheckBlock, ClockBlock, ClocksBlock, ConfigBlock, DatasetBlock,
    ErrorBlock, Report, ReportError, RequestBlock, SourceBlock, Status, card_block, check,
    not_evaluated_block, outcome_fails,
};
use super::select::Scope;
use crate::chart::facts::{DataOrigin, reduce_card};
use crate::chart::schema::{CHECKS, REPORT_SCHEMA_ID};
use std::collections::HashMap;

pub struct Assembly<'a> {
    pub verb: Verb,
    pub clock: ClockBlock,
    pub config: ConfigBlock,
    pub scope: &'a Scope<'a>,
    pub evals: Vec<SourceEval<'a>>,
    pub show_argv: bool,
    pub details: Option<&'a DetailsRequest>,
    /// Errors found after execution (`capture-write`).
    pub errors: Vec<ReportError>,
}

fn step_block(id: &'static str, step: &Step) -> CheckBlock {
    check(id, step.status, step.why.clone())
}

fn request_block(eval: &SourceEval<'_>, show_argv: bool) -> RequestBlock {
    let config = eval.config;
    RequestBlock {
        window: config.window.clone(),
        calendar_days: config.calendar_days,
        from: eval.request.as_ref().map(|r| r.window.start.to_string()),
        to: eval.request.as_ref().map(|r| r.window.end.to_string()),
        timezone: config.timezone.clone(),
        executable: super::eval::executable_name(config),
        argc: config.command.len(),
        argv: show_argv.then(|| {
            eval.request
                .as_ref()
                .map_or_else(|| config.command.clone(), |r| r.argv.clone())
        }),
    }
}

fn aggregate(id: &'static str, per_source: &[CheckBlock]) -> CheckBlock {
    if per_source.is_empty() {
        return check(
            id,
            "not-run",
            Some("no selected card reads any evaluated source".into()),
        );
    }
    let status = ["fail", "unknown", "not-run"]
        .into_iter()
        .find(|wanted| per_source.iter().any(|c| c.status == *wanted))
        .unwrap_or("pass");
    let why = per_source
        .iter()
        .find(|c| c.status == status && c.why.is_some())
        .and_then(|c| c.why.clone())
        .filter(|_| status != "pass")
        .map(|why| {
            let affected = per_source.iter().filter(|c| c.status == status).count();
            if per_source.len() == 1 {
                why
            } else if affected == 1 {
                format!("{why}, for one source")
            } else {
                format!("{why}, for {affected} of {} sources", per_source.len())
            }
        });
    check(id, status, why)
}

pub fn assemble(a: Assembly<'_>) -> Report {
    let origin = if a.verb == Verb::Run {
        DataOrigin::Live
    } else {
        DataOrigin::Replay
    };
    let mut failed_cards: HashMap<&str, usize> = HashMap::new();
    let mut card_count: HashMap<&str, usize> = HashMap::new();
    let mut cards: Vec<CardBlock> = Vec::new();
    for card in &a.scope.cards {
        let eval = a
            .evals
            .iter()
            .find(|eval| eval.config.id == card.source)
            .expect("every scoped card has an evaluated source");
        *card_count.entry(card.source.as_str()).or_default() += 1;
        let page = a.details.filter(|details| details.card == card.id);
        let block = match &eval.snapshot {
            Some(snapshot) => {
                let facts = reduce_card(card, snapshot, eval.clocks.model_at, origin);
                if outcome_fails(&facts.outcome) {
                    *failed_cards.entry(card.source.as_str()).or_default() += 1;
                }
                card_block(&facts, page)
            }
            None => {
                let blank = blank_snapshot(eval.config);
                let intent = reduce_card(card, &blank, eval.clocks.model_at, origin).intent;
                let why = if a.verb == Verb::Check {
                    "check does not evaluate cards".to_string()
                } else {
                    eval.request_step
                        .blocks("request")
                        .and_then(|step| step.why)
                        .unwrap_or_else(|| "the source produced nothing to evaluate".into())
                };
                not_evaluated_block(card, &intent, why)
            }
        };
        cards.push(block);
    }

    let mut sources = Vec::new();
    let mut per_check: Vec<Vec<CheckBlock>> = vec![Vec::new(); CHECKS.len()];
    for eval in &a.evals {
        let id = eval.config.id.as_str();
        let cards_step = if a.verb == Verb::Check {
            Step::not_run("check does not evaluate cards")
        } else if let Some(blocked) = eval.request_step.blocks("request") {
            blocked
        } else if card_count.get(id).copied().unwrap_or(0) == 0 {
            Step::not_run("no selected card reads this source")
        } else if failed_cards.contains_key(id) {
            Step::fail("a card is refused or unavailable")
        } else {
            Step::pass()
        };
        let result = vec![
            check("config", "pass", None),
            step_block("request", &eval.request_step),
            step_block("execution", &eval.execution),
            step_block("dataset", &eval.dataset),
            step_block("cards", &cards_step),
            step_block("freshness", &eval.freshness),
            step_block("coverage", &eval.coverage),
            check(
                "appearance",
                "not-run",
                Some("no pixels are produced".into()),
            ),
        ];
        for (slot, block) in per_check.iter_mut().zip(&result) {
            // A source no selected card reads says nothing about the cards check.
            if block.id == "cards"
                && card_count.get(id).copied().unwrap_or(0) == 0
                && a.verb != Verb::Check
            {
                continue;
            }
            slot.push(block.clone());
        }
        sources.push(SourceBlock {
            id: eval.config.id.clone(),
            data_origin: eval.data_origin,
            clocks: Some(ClocksBlock {
                request_at: eval.clocks.request_at.map(|at| at.to_string()),
                model_at: eval.clocks.model_at.to_string(),
            }),
            request: request_block(eval, a.show_argv),
            attempt: eval.attempt.map(|attempt| AttemptBlock {
                latency_ms: attempt.latency_ms,
                exit: attempt.exit,
            }),
            dataset: match &eval.dataset_value {
                Some(dataset) => DatasetBlock::from_dataset(dataset),
                None => DatasetBlock::unread(eval.dataset_status),
            },
            error: eval.error.as_ref().map(|error| ErrorBlock {
                kind: error.kind.id(),
                text: error.text.clone(),
                stderr_excerpt: error.stderr_excerpt.clone(),
            }),
            result,
        });
    }

    let checks: Vec<CheckBlock> = CHECKS
        .iter()
        .zip(&per_check)
        .map(|((id, _), blocks)| aggregate(id, blocks))
        .collect();
    let status = if !a.errors.is_empty() || checks.iter().any(|c| c.status == "fail") {
        Status::Findings
    } else if checks.iter().any(|c| c.required && c.status == "unknown") {
        Status::Undetermined
    } else {
        Status::Ok
    };
    Report {
        schema: REPORT_SCHEMA_ID,
        command: Some(a.verb.name()),
        status,
        clock: Some(a.clock),
        config: Some(a.config),
        errors: a.errors,
        checks,
        sources,
        cards,
    }
}
