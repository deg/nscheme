//! Source-location diagnostics (bead nscheme-tn3).
//!
//! Turns a byte [`Span`] into a human-readable, rustc-style location
//! block — `line:col`, the offending source line, and a caret underline.
//! Used to give lex/parse errors and uncaught runtime errors a "where",
//! not just a "what".

use crate::lex::Span;

/// 1-based `(line, column)` of a byte offset in `source`.
#[must_use]
pub fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let off = offset.min(source.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= off {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// A located diagnostic block for `span` against `source`:
///
/// ```text
///   at line 3:7
///     (foo (bar baz))
///          ^^^
/// ```
///
/// Returns an empty string if `source` is empty (e.g. a span into
/// synthesized/macro-expanded code with no original text).
#[must_use]
pub fn locate(source: &str, span: Span) -> String {
    if source.is_empty() {
        return String::new();
    }
    let (line, col) = line_col(source, span.start);
    let line_text = source.lines().nth(line - 1).unwrap_or("");
    // Caret width: the span clamped to the remainder of this line, at
    // least one column.
    let (end_line, end_col) = line_col(source, span.end);
    let width = if end_line == line {
        end_col.saturating_sub(col).max(1)
    } else {
        line_text.chars().count().saturating_sub(col - 1).max(1)
    };
    let pad = " ".repeat(col - 1);
    let carets = "^".repeat(width);
    format!("  at line {line}:{col}\n    {line_text}\n    {pad}{carets}")
}
