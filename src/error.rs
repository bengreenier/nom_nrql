//! Parse error type for NRQL parser.

use std::fmt;

/// Error returned when NRQL parsing fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable message or expected token summary.
    pub message: String,
    /// Byte offset in the input where the error occurred (if available).
    pub offset: Option<usize>,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(off) = self.offset {
            write!(f, "parse error at offset {}: {}", off, self.message)
        } else {
            write!(f, "parse error: {}", self.message)
        }
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    pub fn new(message: impl Into<String>, offset: Option<usize>) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }
}
