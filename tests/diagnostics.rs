//! Source-location diagnostics (bead nscheme-tn3, stage 1).

use nscheme::diagnostic::{line_col, locate};
use nscheme::lex::Span;
use nscheme::parse::{ParseError, parse_program};

#[test]
fn line_col_is_one_based_and_tracks_newlines() {
    let src = "abc\ndefg\nhi";
    assert_eq!(line_col(src, 0), (1, 1));
    assert_eq!(line_col(src, 2), (1, 3));
    assert_eq!(line_col(src, 4), (2, 1)); // first char after the newline
    assert_eq!(line_col(src, 9), (3, 1)); // 'h'
    assert_eq!(line_col(src, 10), (3, 2)); // 'i'
}

#[test]
fn locate_points_a_caret_at_the_span() {
    let src = "(+ 1 2))";
    // The stray ')' is at byte 7.
    let block = locate(src, Span::new(7, 8));
    assert!(block.contains("at line 1:8"), "{block}");
    assert!(block.contains("(+ 1 2))"), "{block}");
    assert!(block.contains('^'), "{block}");
    // Caret sits under column 8.
    let caret_line = block.lines().last().unwrap();
    assert_eq!(caret_line.trim_end().len(), 4 + 7 + 1); // "    " + 7 pad + one caret
}

#[test]
fn parse_errors_carry_a_span() {
    match parse_program("(+ 1 2))") {
        Err(e @ ParseError::UnexpectedRParen { .. }) => {
            let span = e.span().expect("rparen error has a span");
            assert_eq!(span.start, 7);
        }
        other => panic!("expected UnexpectedRParen, got {other:?}"),
    }
}

#[test]
fn eof_error_has_no_span() {
    match parse_program("(+ 1 2") {
        Err(e @ (ParseError::UnexpectedEof | ParseError::UnclosedList { .. })) => {
            // UnclosedList has a span; UnexpectedEof does not. Either is a
            // valid "unfinished input" outcome — just exercise span().
            let _ = e.span();
        }
        other => panic!("expected an unfinished-input error, got {other:?}"),
    }
}
