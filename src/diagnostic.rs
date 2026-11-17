//! Source-location diagnostics (bead nscheme-tn3).
//!
//! Turns a byte [`Span`] into a human-readable, rustc-style location
//! block — `line:col`, the offending source line, and a caret underline.
//! Used to give lex/parse errors and uncaught runtime errors a "where",
//! not just a "what".

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::lex::Span;

thread_local! {
    /// The source text currently being evaluated, for byte -> line:col.
    static SOURCE: RefCell<String> = const { RefCell::new(String::new()) };
    /// Map from a parsed pair's pointer identity to its source span.
    /// Populated by the parser; read by the evaluator to locate the form
    /// it's currently evaluating (bead nscheme-tn3).
    static SPANS: RefCell<HashMap<usize, Span>> = RefCell::new(HashMap::new());
    /// The span of the innermost form the evaluator has entered so far —
    /// i.e. where an error is happening right now.
    static CURRENT: Cell<Option<Span>> = const { Cell::new(None) };
}

/// Start tracking a new top-level source string: store it and reset the
/// per-program span map and current-form pointer.
pub fn begin_source(source: &str) {
    SOURCE.with(|s| {
        let mut s = s.borrow_mut();
        s.clear();
        s.push_str(source);
    });
    SPANS.with(|m| m.borrow_mut().clear());
    CURRENT.with(|c| c.set(None));
}

/// Record the source span of a parsed pair (keyed by pointer identity).
pub fn record_span(pair_id: usize, span: Span) {
    SPANS.with(|m| {
        m.borrow_mut().insert(pair_id, span);
    });
}

/// Note that the evaluator is now entering the pair `pair_id`; if it has
/// a recorded span, it becomes the "current" error location. Pairs with
/// no span (e.g. macro-expanded forms) leave the current location at the
/// nearest enclosing recorded form.
pub fn note_current(pair_id: usize) {
    SPANS.with(|m| {
        if let Some(&span) = m.borrow().get(&pair_id) {
            CURRENT.with(|c| c.set(Some(span)));
        }
    });
}

/// A located block for the form currently being evaluated, if known.
#[must_use]
pub fn current_location() -> Option<String> {
    let span = CURRENT.with(Cell::get)?;
    SOURCE.with(|s| {
        let src = s.borrow();
        if src.is_empty() {
            return None;
        }
        let loc = locate(&src, span);
        (!loc.is_empty()).then_some(loc)
    })
}

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

/// The full report for an evaluation error from running `source`, as the
/// CLI prints it and the browser wrapper shows it: the message, then a
/// `line:col` + caret block, then the call trace when there is one.
///
/// Drains the backtrace recorded by the failed evaluation (via
/// [`crate::eval::take_backtrace`]), so call it once per error, straight
/// after the evaluation that failed.
#[must_use]
pub fn format_error(source: &str, e: &crate::eval::EvalError) -> String {
    use crate::eval::EvalError;
    // Always drain the call-chain backtrace so a stale one can't leak to
    // a later (e.g. parse) error.
    let trace = crate::eval::take_backtrace();
    if let EvalError::Parse(pe) = e {
        // Lex/parse errors carry a span into `source`.
        if let Some(span) = pe.span() {
            let loc = locate(source, span);
            if !loc.is_empty() {
                return format!("error: {pe}\n{loc}");
            }
        }
        return format!("error: {e}");
    }
    let mut out = format!("error: {e}");
    if let Some(loc) = current_location() {
        out.push('\n');
        out.push_str(&loc);
    }
    if !trace.is_empty() {
        out.push_str("\n  call trace (innermost first): ");
        out.push_str(&trace.join(" <- "));
    }
    out
}
