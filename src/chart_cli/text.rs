//! The short human summary of a report. Built only from the report value:
//! it never recomputes a fact.

use super::report::Report;

pub fn render(report: &Report) -> String {
    let verb = report.command.unwrap_or("-");
    let mut out = format!(
        "sidescope charts {verb}: {} (exit {})\n",
        report.status.name(),
        report.status.exit_code()
    );
    for error in &report.errors {
        let key = error
            .key
            .as_deref()
            .map(|key| format!(" [{key}]"))
            .unwrap_or_default();
        out.push_str(&format!("error {}{key}: {}\n", error.id, error.text));
    }
    for check in &report.checks {
        let why = check
            .why
            .as_deref()
            .map(|why| format!(" — {why}"))
            .unwrap_or_default();
        out.push_str(&format!("check {}: {}{why}\n", check.id, check.status));
    }
    for source in &report.sources {
        let error = source
            .error
            .as_ref()
            .map(|error| format!(" — {}", error.kind))
            .unwrap_or_default();
        out.push_str(&format!(
            "source {}: {}{error}\n",
            source.id,
            match &source.dataset {
                super::report::DatasetBlock::Valid(dataset) => dataset.status,
                super::report::DatasetBlock::Unread { status } => status,
            }
        ));
    }
    for card in &report.cards {
        let headline = card
            .display
            .as_ref()
            .and_then(|display| display.headline.as_ref())
            .map(|headline| format!(" — {}: {}", headline.label, headline.value))
            .unwrap_or_default();
        out.push_str(&format!(
            "card {}: {}{headline}\n",
            card.id, card.outcome.kind
        ));
    }
    out
}
