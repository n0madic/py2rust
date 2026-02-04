use rustpython_parser::text_size::TextRange;

/// Source code span (byte offset range) for error reporting.
///
/// Span represents a range in the source text, stored as byte offsets.
/// This is used throughout the compiler to track where each AST/HIR node
/// came from in the original source, enabling precise error messages.
///
/// Design note: We use byte offsets (usize) rather than line/column because:
/// 1. It's what RustPython parser provides
/// 2. It's more compact (2 usizes vs 4 for line:col pairs)
/// 3. The miette error reporting library can convert to line:col when needed
/// 4. Byte offsets are more precise for multi-byte Unicode characters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Length of the span in bytes.
    /// Uses saturating_sub to handle the edge case of end < start gracefully.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Convert from RustPython's TextRange to our Span.
/// This is used during lowering when we extract spans from the parser's AST.
impl From<TextRange> for Span {
    fn from(range: TextRange) -> Self {
        Self {
            start: range.start().into(),
            end: range.end().into(),
        }
    }
}
