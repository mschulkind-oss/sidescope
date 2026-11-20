//! Structured chart failures. Messages are plain, sanitized and never carry
//! command output or argument values.

use std::fmt;

const MESSAGE_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartErrorKind {
    ConfigInvalid,
    MissingExecutable,
    SpawnFailed,
    NonzeroExit,
    Timeout,
    OutputBound,
    Cancelled,
    ParseFailure,
    UnsupportedVersion,
    InvalidMapping,
    InvalidDataset,
    CardMismatch,
}

impl ChartErrorKind {
    /// Scheduled retry cannot help until the owner changes settings or asks.
    pub fn suspends_retry(self) -> bool {
        matches!(self, Self::ConfigInvalid | Self::MissingExecutable)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ConfigInvalid => "invalid configuration",
            Self::MissingExecutable => "missing executable",
            Self::SpawnFailed => "spawn failed",
            Self::NonzeroExit => "nonzero exit",
            Self::Timeout => "timeout",
            Self::OutputBound => "output bound",
            Self::Cancelled => "cancelled",
            Self::ParseFailure => "parse failure",
            Self::UnsupportedVersion => "unsupported version",
            Self::InvalidMapping => "invalid mapping",
            Self::InvalidDataset => "invalid dataset",
            Self::CardMismatch => "card mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartError {
    pub kind: ChartErrorKind,
    message: String,
}

impl ChartError {
    pub fn new(kind: ChartErrorKind, message: impl AsRef<str>) -> Self {
        Self {
            kind,
            message: sanitize_text(message.as_ref(), MESSAGE_LIMIT),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ChartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.label(), self.message)
    }
}

impl std::error::Error for ChartError {}

/// Drops every control character (ANSI escape introducers and newlines
/// included) and truncates to `max_chars` characters with a trailing `…`.
pub fn sanitize_text(text: &str, max_chars: usize) -> String {
    let mut clean: String = text.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() > max_chars {
        clean = clean.chars().take(max_chars.saturating_sub(1)).collect();
        clean.push('…');
    }
    clean
}
