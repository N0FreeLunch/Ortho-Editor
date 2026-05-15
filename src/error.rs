use thiserror::Error;
use std::io;

/// A structured error for parsing and verification tasks.
/// Designed to provide AI-friendly, actionable diagnostic information.
#[derive(Debug, Clone, Error)]
#[error(r#"{{"error": "DiagnosticError", "offset": {offset}, "expected": "{expected}", "actual": "{actual}", "hint": "{hint}"}}"#)]
pub struct DiagnosticError {
    pub offset: usize,
    pub expected: String,
    pub actual: String,
    pub hint: String,
}

impl DiagnosticError {
    pub fn new(
        offset: usize,
        expected: impl Into<String>,
        actual: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            offset,
            expected: expected.into(),
            actual: actual.into(),
            hint: hint.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParsingError {
    #[error("Invalid Huffman bit at offset {bit_offset}")]
    InvalidHuffmanBit { bit_offset: u64 },
    #[error("Invalid stat_id {stat_id} at offset {bit_offset}")]
    InvalidStatId { bit_offset: u64, stat_id: u32 },
    #[error("Unexpected segment end at offset {bit_offset}")]
    UnexpectedSegmentEnd { bit_offset: u64 },
    #[error("Bit symmetry failure at offset {bit_offset}")]
    BitSymmetryFailure { bit_offset: u64 },
    /// A value was read that violates a structural invariant (e.g., a magic number mismatch).
    #[error("Invariant violation in '{field}': expected {expected}, found {actual}")]
    InvariantViolation { field: String, expected: String, actual: String },
    /// A value was read that is technically valid but unexpected in the current context.
    #[error("Unexpected value for '{field}': {value} ({reason})")]
    UnexpectedValue { field: String, value: String, reason: String },
    /// A specific marker (e.g., "JM") was expected but not found.
    #[error("Missing marker '{marker}' at bit offset {bit_offset}")]
    MissingMarker { marker: String, bit_offset: u64 },
    /// A potential bit shift or drift was detected based on alignment rules.
    #[error("Potential bit drift: expected {expected_offset}, actual {actual_offset}")]
    BitDriftDetected { expected_offset: u64, actual_offset: u64 },
    /// Alignment requirement not met (e.g., not byte-aligned when expected).
    #[error("Alignment error at bit {bit_offset}: {reason}")]
    AlignmentError { bit_offset: u64, reason: String },
    #[error("Bit budget exceeded at bit {bit_offset}")]
    BitBudgetExceeded { bit_offset: u64 },
    #[error("IO error: {0}")]
    Io(String), 
    #[error("Parsing error: {0}")]
    Generic(String),
    #[error("Speculative rejection at bit {bit_offset} (confidence: {confidence})")]
    SpeculativeRejection { bit_offset: u64, confidence: u32 },
}

#[derive(Debug, Clone, Error)]
pub struct ParsingFailure {
    pub error: ParsingError,
    pub context_stack: Vec<String>,
    pub bit_offset: u64,
    /// The bit offset relative to the start of the current context.
    pub context_relative_offset: u64,
    /// An optional hint for forensic recovery.
    pub hint: Option<String>,
}

impl std::fmt::Display for ParsingFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ctx = self.context_stack.join(" -> ");
        write!(
            f, 
            "[Bit {}] [Rel +{}] [{}] {}", 
            self.bit_offset, 
            self.context_relative_offset,
            ctx, 
            self.error
        )?;
        if let Some(hint) = &self.hint {
            write!(f, " | Hint: {}", hint)?;
        }
        Ok(())
    }
}



/// Trait to abstract over anything that can provide context for a ParsingFailure.
pub trait BackingBitCursor {
    fn pos(&self) -> u64;
    fn context_stack(&self) -> Vec<String>;
    fn current_context_start(&self) -> u64;
}

impl ParsingFailure {
    pub fn new(error: ParsingError, cursor: &dyn BackingBitCursor) -> Self {
        let bit_offset = cursor.pos();
        let context_start = cursor.current_context_start();
        ParsingFailure {
            error,
            context_stack: cursor.context_stack(),
            bit_offset,
            context_relative_offset: bit_offset.saturating_sub(context_start),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: &str) -> Self {
        self.hint = Some(hint.to_string());
        self
    }
}

impl From<io::Error> for ParsingFailure {
    fn from(e: io::Error) -> Self {
        ParsingFailure {
            error: ParsingError::Io(e.to_string()),
            context_stack: Vec::new(),
            bit_offset: 0, // Unknown without cursor context
            context_relative_offset: 0,
            hint: Some("IO error converted to ParsingFailure".to_string()),
        }
    }
}

impl From<ParsingFailure> for io::Error {
    fn from(f: ParsingFailure) -> Self {
        io::Error::new(io::ErrorKind::Other, f.to_string())
    }
}

pub type ParsingResult<T> = Result<T, ParsingFailure>;
