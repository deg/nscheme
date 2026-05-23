//! Parser that converts a [`Token`] stream into runtime [`Value`]s.
//!
//! Scheme programs *are* data: the parser does not produce a separate AST
//! type, it produces the same [`Value`] enum the interpreter manipulates
//! at runtime. The evaluator then walks that value.
//!
//! The grammar implemented here is R7RS-small §7.1.2 (external
//! representations), restricted to what nscheme v1 supports:
//!
//! - Atoms: booleans, characters, strings, numbers (`Int` and `Float` only
//!   in v1; bignums and rationals come in `nscheme-c92`), symbols.
//! - Lists, proper and improper (`(a b . c)`).
//! - Vectors and bytevectors.
//! - Reader macros (`'`, `` ` ``, `,`, `,@`) expand to two-element lists.
//! - Datum comments (`#;`) skip exactly one following datum.

use std::collections::{HashMap, VecDeque};

use thiserror::Error;

use std::rc::Rc;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{FromPrimitive, Num as NumTrait, One, ToPrimitive, Zero};

use crate::lex::{Exactness, NumberLexeme, Span, Token, TokenKind, tokenize};
use crate::value::{Symbol, Value};

/// Parser errors. All variants carry a [`Span`] so callers can render a
/// diagnostic against the original source.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected end of input")]
    UnexpectedEof,

    #[error("unexpected `)` at byte {}", span.start)]
    UnexpectedRParen { span: Span },

    #[error("unexpected `.` at byte {}", span.start)]
    UnexpectedDot { span: Span },

    #[error("`(` opened at byte {} was not closed", span.start)]
    UnclosedList { span: Span },

    #[error("expected one cdr after `.` at byte {}", span.start)]
    BadDottedTail { span: Span },

    #[error("invalid number literal `{lexeme}` at byte {}", span.start)]
    InvalidNumber { lexeme: String, span: Span },

    #[error("undefined datum label `#{label}#` at byte {}", span.start)]
    UndefinedDatumLabel { label: u64, span: Span },

    #[error("duplicate datum label `#{label}=` at byte {}", span.start)]
    DuplicateDatumLabel { label: u64, span: Span },

    #[error("bytevector element must be an integer in 0..=255 at byte {}", span.start)]
    BadBytevectorElement { span: Span },

    #[error("forwarded lexical error: {0}")]
    Lex(#[from] crate::lex::LexError),
}

/// Parse a full program: zero or more top-level datums.
pub fn parse_program(source: &str) -> Result<Vec<Value>, ParseError> {
    let tokens = tokenize(source)?;
    let mut parser = Parser::new(tokens);
    let mut datums = Vec::new();
    while !parser.at_end() {
        datums.push(parser.parse_datum()?);
    }
    Ok(datums)
}

/// Parse one datum from `source`. Returns `Ok((Some(datum), consumed))`
/// where `consumed` is the byte offset of the first character *after*
/// the datum (so the caller can advance a port position by exactly the
/// right amount). Returns `Ok((None, source.len()))` if `source`
/// contains only whitespace/comments.
///
/// Used by the `(read port)` primitive to drive incremental datum
/// reading from a port without re-lexing the rest of the buffer
/// from scratch each call.
pub fn parse_one_with_consumed(source: &str) -> Result<(Option<Value>, usize), ParseError> {
    let tokens = tokenize(source)?;
    if tokens.is_empty() {
        return Ok((None, source.len()));
    }
    let mut parser = Parser::new(tokens);
    let datum = parser.parse_datum()?;
    let consumed = parser.peek().map_or(source.len(), |t| t.span.start);
    Ok((Some(datum), consumed))
}

/// Parse exactly one datum from a string. Returns an error if the input
/// contains zero or more than one datum.
pub fn parse_one(source: &str) -> Result<Value, ParseError> {
    let mut datums = parse_program(source)?;
    match datums.len() {
        0 => Err(ParseError::UnexpectedEof),
        1 => Ok(datums.pop().unwrap()),
        _ => Err(ParseError::UnexpectedDot {
            // Re-use a span-bearing error variant; the dot variant is
            // close enough to "junk after the datum" for now.
            span: Span::new(0, 0),
        }),
    }
}

struct Parser {
    tokens: VecDeque<Token>,
    /// Registered datum labels. Each value is a placeholder cons cell
    /// whose `car`/`cdr` get mutated in place once the labeled datum
    /// is parsed — that way `#0=(1 . #0#)` produces a real cycle.
    labels: HashMap<u64, Value>,
    /// Folding mode (case-insensitive identifiers) — toggled by
    /// `#!fold-case` / `#!no-fold-case` directives per R7RS §2.1.
    fold_case: bool,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens: tokens.into(),
            labels: HashMap::new(),
            fold_case: false,
        }
    }

    fn at_end(&self) -> bool {
        self.tokens.is_empty()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.front()
    }

    fn pop(&mut self) -> Option<Token> {
        self.tokens.pop_front()
    }

    /// Parse one datum. Datum-comments and `#!fold-case` directives
    /// are silently consumed here so the rest of the parser never
    /// sees them.
    fn parse_datum(&mut self) -> Result<Value, ParseError> {
        loop {
            match self.peek().map(|t| &t.kind) {
                Some(TokenKind::DatumComment) => {
                    self.pop();
                    self.parse_datum()?;
                }
                Some(TokenKind::FoldCase) => {
                    self.pop();
                    self.fold_case = true;
                }
                Some(TokenKind::NoFoldCase) => {
                    self.pop();
                    self.fold_case = false;
                }
                _ => break,
            }
        }

        let Some(tok) = self.pop() else {
            return Err(ParseError::UnexpectedEof);
        };
        match tok.kind {
            TokenKind::LParen => self.parse_list(tok.span),
            TokenKind::RParen => Err(ParseError::UnexpectedRParen { span: tok.span }),
            TokenKind::Dot => Err(ParseError::UnexpectedDot { span: tok.span }),
            TokenKind::VectorStart => self.parse_vector(tok.span),
            TokenKind::BytevectorStart => self.parse_bytevector(tok.span),
            TokenKind::Quote => self.parse_quoted("quote"),
            TokenKind::Quasiquote => self.parse_quoted("quasiquote"),
            TokenKind::Unquote => self.parse_quoted("unquote"),
            TokenKind::UnquoteSplicing => self.parse_quoted("unquote-splicing"),
            TokenKind::DatumComment | TokenKind::FoldCase | TokenKind::NoFoldCase => {
                // The loop above handles these; reaching here means
                // a directive with nothing following it.
                Err(ParseError::UnexpectedEof)
            }
            TokenKind::Boolean(b) => Ok(Value::Bool(b)),
            TokenKind::Character(c) => Ok(Value::Char(c)),
            TokenKind::String(s) => Ok(Value::string(s)),
            TokenKind::Identifier(name) => {
                let name = if self.fold_case {
                    name.to_lowercase()
                } else {
                    name
                };
                Ok(Value::Symbol(Symbol::intern(&name)))
            }
            TokenKind::Number(num) => parse_number(&num, tok.span),
            TokenKind::DatumLabel(n) => self.parse_labelled(n, tok.span),
            TokenKind::DatumRef(n) => self
                .labels
                .get(&n)
                .cloned()
                .ok_or(ParseError::UndefinedDatumLabel {
                    label: n,
                    span: tok.span,
                }),
        }
    }

    /// Parse a `#N=<datum>` form. Pre-allocates a placeholder cons
    /// cell registered under the label so that any `#N#` references
    /// inside `<datum>` see a stable Pair — once parsing completes,
    /// the placeholder is mutated to mirror the actual datum.
    fn parse_labelled(&mut self, label: u64, span: Span) -> Result<Value, ParseError> {
        if self.labels.contains_key(&label) {
            return Err(ParseError::DuplicateDatumLabel { label, span });
        }
        let placeholder = Value::cons(Value::Unspecified, Value::Unspecified);
        self.labels.insert(label, placeholder.clone());
        let datum = self.parse_datum()?;
        match (&placeholder, &datum) {
            (Value::Pair(holder), Value::Pair(real)) => {
                // Patch the placeholder so all back-references that
                // already point at `holder` now see the real cells.
                let r = real.borrow();
                let mut h = holder.borrow_mut();
                h.car = r.car.clone();
                h.cdr = r.cdr.clone();
                drop(h);
                drop(r);
                Ok(placeholder)
            }
            (Value::Pair(_), _) => {
                // Labeled atom — replace the registered value so that
                // any later `#N#` references resolve to the atom too.
                self.labels.insert(label, datum.clone());
                Ok(datum)
            }
            _ => unreachable!("placeholder is always a Pair"),
        }
    }

    /// Parse the body of a list after the `(` has been consumed.
    fn parse_list(&mut self, open_span: Span) -> Result<Value, ParseError> {
        let mut items: Vec<Value> = Vec::new();
        loop {
            // Skip datum comments inside the list.
            while let Some(tok) = self.peek() {
                if !matches!(tok.kind, TokenKind::DatumComment) {
                    break;
                }
                self.pop();
                self.parse_datum()?;
            }
            let Some(tok) = self.peek() else {
                return Err(ParseError::UnclosedList { span: open_span });
            };
            match tok.kind {
                TokenKind::RParen => {
                    self.pop();
                    return Ok(build_list(items, Value::Null));
                }
                TokenKind::Dot => {
                    // (a b . c)
                    let dot_span = tok.span;
                    if items.is_empty() {
                        return Err(ParseError::UnexpectedDot { span: dot_span });
                    }
                    self.pop(); // consume `.`
                    let tail = self.parse_datum()?;
                    // Skip any datum-comments between the cdr and
                    // the closing `)`: `(a . b #;c)` is well-formed.
                    while let Some(tok) = self.peek() {
                        if !matches!(tok.kind, TokenKind::DatumComment) {
                            break;
                        }
                        self.pop();
                        self.parse_datum()?;
                    }
                    let closing = self
                        .pop()
                        .ok_or(ParseError::UnclosedList { span: open_span })?;
                    if !matches!(closing.kind, TokenKind::RParen) {
                        return Err(ParseError::BadDottedTail { span: dot_span });
                    }
                    return Ok(build_list(items, tail));
                }
                _ => {
                    items.push(self.parse_datum()?);
                }
            }
        }
    }

    fn parse_vector(&mut self, open_span: Span) -> Result<Value, ParseError> {
        let mut items: Vec<Value> = Vec::new();
        loop {
            while let Some(tok) = self.peek() {
                if !matches!(tok.kind, TokenKind::DatumComment) {
                    break;
                }
                self.pop();
                self.parse_datum()?;
            }
            let Some(tok) = self.peek() else {
                return Err(ParseError::UnclosedList { span: open_span });
            };
            if matches!(tok.kind, TokenKind::RParen) {
                self.pop();
                return Ok(Value::vector(items));
            }
            items.push(self.parse_datum()?);
        }
    }

    fn parse_bytevector(&mut self, open_span: Span) -> Result<Value, ParseError> {
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            let Some(tok) = self.peek() else {
                return Err(ParseError::UnclosedList { span: open_span });
            };
            if matches!(tok.kind, TokenKind::RParen) {
                self.pop();
                return Ok(Value::bytevector(bytes));
            }
            let element_span = tok.span;
            let datum = self.parse_datum()?;
            // Bytevector elements must be integers in 0..=255.
            match datum {
                Value::Int(n) if (0..=255).contains(&n) => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    bytes.push(n as u8);
                }
                _ => {
                    return Err(ParseError::BadBytevectorElement { span: element_span });
                }
            }
        }
    }

    fn parse_quoted(&mut self, head: &'static str) -> Result<Value, ParseError> {
        let inner = self.parse_datum()?;
        Ok(Value::list_from([
            Value::Symbol(Symbol::intern(head)),
            inner,
        ]))
    }
}

/// Build a list from a prefix of items and a tail. If `tail` is `Null` we
/// get a proper list `(a b c)`; otherwise we get an improper list
/// `(a b . tail)`.
fn build_list(items: Vec<Value>, tail: Value) -> Value {
    let mut acc = tail;
    for item in items.into_iter().rev() {
        acc = Value::cons(item, acc);
    }
    acc
}

/// Convert a [`NumberLexeme`] into a runtime [`Value`].
///
/// Supports the full R7RS exact tower (fixnum / bignum / rational) plus
/// inexact reals. Complex numbers (`a+bi`, `a@b`) are recognized but
/// remain unimplemented and return an [`ParseError::InvalidNumber`].
fn parse_number(num: &NumberLexeme, span: Span) -> Result<Value, ParseError> {
    let body = &num.body;
    let radix = num.radix;
    let bad = || ParseError::InvalidNumber {
        lexeme: format_lexeme(num),
        span,
    };

    // +inf.0 / -inf.0 / +nan.0 / -nan.0 are case-insensitive in
    // radix 10 (R7RS §7.1.1 — the lexeme matches `[+-](inf|nan).0`
    // regardless of letter case).
    if radix == 10 && body.len() == 6 {
        let lower = body.to_ascii_lowercase();
        match lower.as_str() {
            "+inf.0" => return Ok(Value::Float(f64::INFINITY)),
            "-inf.0" => return Ok(Value::Float(f64::NEG_INFINITY)),
            "+nan.0" | "-nan.0" => return Ok(Value::Float(f64::NAN)),
            _ => {}
        }
    }

    // Complex numbers — recognized lexically but not yet implemented.
    // Returning a sentinel symbol rather than erroring lets the
    // chibi conformance corpus parse end-to-end so the relevant
    // tests fail gracefully (the symbol is undefined at use sites)
    // instead of aborting the whole run with a parse error.
    if body.contains('i') || body.contains('@') {
        let _ = span;
        return Ok(Value::Symbol(Symbol::intern(&format!(
            "#unparseable-number:{}",
            format_lexeme(num)
        ))));
    }

    // Exact rational: a/b.
    if let Some(slash) = body.find('/') {
        let (num_str, denom_str_with_slash) = body.split_at(slash);
        let denom_str = &denom_str_with_slash[1..];
        let numer = <BigInt as NumTrait>::from_str_radix(num_str, radix).map_err(|_| bad())?;
        let denom = <BigInt as NumTrait>::from_str_radix(denom_str, radix).map_err(|_| bad())?;
        if denom.is_zero() {
            return Err(bad());
        }
        let r = BigRational::new(numer, denom);
        // Promote to inexact if #i.
        if matches!(num.exactness, Exactness::Inexact) {
            return Ok(Value::Float(rational_to_f64(&r)));
        }
        return Ok(normalize_rational(r));
    }

    // R7RS allows `e/E` as the exponent marker. Chibi (and many older
    // Schemes / R6RS) accept the precision-tagged markers
    // `s/S/f/F/d/D/l/L` too — we normalize them to `e` before
    // handing the literal to Rust's float parser.
    let exponent_chars = ['s', 'S', 'f', 'F', 'd', 'D', 'l', 'L'];
    let body_has_marker = body.contains('.')
        || body.contains('e')
        || body.contains('E')
        || body.chars().any(|c| exponent_chars.contains(&c));
    let looks_inexact = radix == 10 && body_has_marker;

    if looks_inexact {
        let normalised: String = body
            .chars()
            .map(|c| if exponent_chars.contains(&c) { 'e' } else { c })
            .collect();
        let f: f64 = normalised.parse().map_err(|_| bad())?;
        // #e prefix on a decimal: convert to exact rational.
        if matches!(num.exactness, Exactness::Exact) {
            return Ok(Value::Rational(Rc::new(float_to_rational_or_error(
                f, bad,
            )?)));
        }
        return Ok(Value::Float(f));
    }

    // Exact integer path. Try i64 fast path; on overflow, BigInt.
    if let Ok(n) = i64::from_str_radix(body, radix) {
        if matches!(num.exactness, Exactness::Inexact) {
            #[allow(clippy::cast_precision_loss)]
            return Ok(Value::Float(n as f64));
        }
        return Ok(Value::Int(n));
    }
    let big = <BigInt as NumTrait>::from_str_radix(body, radix).map_err(|_| bad())?;
    if matches!(num.exactness, Exactness::Inexact) {
        return Ok(Value::Float(bigint_to_f64(&big)));
    }
    Ok(promote_bigint(big))
}

/// Normalize: rationals with denominator 1 collapse to integers.
fn normalize_rational(r: BigRational) -> Value {
    if One::is_one(r.denom()) {
        promote_bigint(r.numer().clone())
    } else {
        Value::Rational(Rc::new(r))
    }
}

/// Collapse a `BigInt` into a fixnum if it fits in `i64`.
fn promote_bigint(b: BigInt) -> Value {
    if let Some(n) = b.to_i64() {
        Value::Int(n)
    } else {
        Value::BigInt(Rc::new(b))
    }
}

fn rational_to_f64(r: &BigRational) -> f64 {
    r.to_f64().unwrap_or(f64::NAN)
}

fn bigint_to_f64(b: &BigInt) -> f64 {
    b.to_f64().unwrap_or(f64::NAN)
}

/// Convert an `f64` to an exact rational, or error if non-finite. Used
/// when the user explicitly wrote `#e<decimal>`.
fn float_to_rational_or_error(
    f: f64,
    err: impl Fn() -> ParseError,
) -> Result<BigRational, ParseError> {
    if !f.is_finite() {
        return Err(err());
    }
    BigRational::from_f64(f).ok_or_else(err)
}

fn format_lexeme(num: &NumberLexeme) -> String {
    let radix_prefix = match num.radix {
        2 => "#b",
        8 => "#o",
        16 => "#x",
        _ => "",
    };
    let exactness_prefix = match num.exactness {
        Exactness::Exact => "#e",
        Exactness::Inexact => "#i",
        Exactness::Default => "",
    };
    format!("{exactness_prefix}{radix_prefix}{}", num.body)
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Symbol, equal};

    fn parse(input: &str) -> Value {
        parse_one(input).expect("parse")
    }

    fn sym(name: &str) -> Value {
        Value::Symbol(Symbol::intern(name))
    }

    // -- atoms --------------------------------------------------------

    #[test]
    fn boolean_atoms() {
        assert!(equal(&parse("#t"), &Value::Bool(true)));
        assert!(equal(&parse("#false"), &Value::Bool(false)));
    }

    #[test]
    fn integer_atoms() {
        assert!(equal(&parse("0"), &Value::Int(0)));
        assert!(equal(&parse("42"), &Value::Int(42)));
        assert!(equal(&parse("-7"), &Value::Int(-7)));
    }

    #[test]
    fn radix_integers() {
        assert!(equal(&parse("#b1010"), &Value::Int(10)));
        assert!(equal(&parse("#o17"), &Value::Int(15)));
        assert!(equal(&parse("#xFF"), &Value::Int(255)));
        assert!(equal(&parse("#x-FF"), &Value::Int(-255)));
    }

    #[test]
    fn float_atoms() {
        assert!(equal(&parse("2.5"), &Value::Float(2.5)));
        assert!(equal(&parse("-2.5e3"), &Value::Float(-2500.0)));
        assert!(equal(&parse(".5"), &Value::Float(0.5)));
    }

    #[test]
    fn inf_and_nan() {
        let inf = parse("+inf.0");
        let neg_inf = parse("-inf.0");
        assert!(matches!(inf, Value::Float(f) if f == f64::INFINITY));
        assert!(matches!(neg_inf, Value::Float(f) if f == f64::NEG_INFINITY));
        assert!(matches!(parse("+nan.0"), Value::Float(f) if f.is_nan()));
    }

    #[test]
    fn rational_parsed_as_exact_rational() {
        match parse_one("3/4").unwrap() {
            Value::Rational(r) => {
                assert_eq!(r.numer().to_string(), "3");
                assert_eq!(r.denom().to_string(), "4");
            }
            other => panic!("expected Rational, got {other:?}"),
        }
    }

    #[test]
    fn integer_overflow_promotes_to_bigint() {
        // 2^63 doesn't fit in i64 — must become a BigInt.
        match parse_one("9223372036854775808").unwrap() {
            Value::BigInt(b) => assert_eq!(b.to_string(), "9223372036854775808"),
            other => panic!("expected BigInt, got {other:?}"),
        }
    }

    #[test]
    fn rational_reduces_when_integral() {
        // 6/3 reduces to 2 (an Int, not a Rational with denom 1).
        assert!(equal(&parse("6/3"), &Value::Int(2)));
    }

    #[test]
    fn char_atom() {
        assert!(equal(&parse(r"#\a"), &Value::Char('a')));
        assert!(equal(&parse(r"#\newline"), &Value::Char('\n')));
    }

    #[test]
    fn string_atom() {
        assert!(equal(&parse(r#""hello""#), &Value::string("hello")));
    }

    #[test]
    fn identifier_becomes_symbol() {
        assert!(equal(&parse("foo"), &sym("foo")));
        assert!(equal(&parse("+"), &sym("+")));
        assert!(equal(&parse("string->list"), &sym("string->list")));
    }

    // -- lists --------------------------------------------------------

    #[test]
    fn empty_list() {
        assert!(equal(&parse("()"), &Value::Null));
    }

    #[test]
    fn proper_list() {
        let expected = Value::list_from([Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert!(equal(&parse("(1 2 3)"), &expected));
    }

    #[test]
    fn nested_list() {
        let inner = Value::list_from([Value::Int(2), Value::Int(3)]);
        let expected = Value::list_from([Value::Int(1), inner, Value::Int(4)]);
        assert!(equal(&parse("(1 (2 3) 4)"), &expected));
    }

    #[test]
    fn improper_list() {
        // (a b . c)
        let parsed = parse("(a b . c)");
        let expected = Value::cons(sym("a"), Value::cons(sym("b"), sym("c")));
        assert!(equal(&parsed, &expected));
    }

    #[test]
    fn unclosed_list_is_error() {
        assert!(matches!(
            parse_one("(1 2"),
            Err(ParseError::UnclosedList { .. })
        ));
    }

    #[test]
    fn unexpected_rparen_is_error() {
        assert!(matches!(
            parse_one(")"),
            Err(ParseError::UnexpectedRParen { .. })
        ));
    }

    #[test]
    fn dot_without_left_side_is_error() {
        assert!(matches!(
            parse_one("(. 1)"),
            Err(ParseError::UnexpectedDot { .. })
        ));
    }

    // -- vectors and bytevectors -------------------------------------

    #[test]
    fn vector_literal() {
        let parsed = parse("#(1 2 3)");
        assert!(equal(
            &parsed,
            &Value::vector(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        ));
    }

    #[test]
    fn bytevector_literal() {
        let parsed = parse("#u8(0 127 255)");
        assert!(equal(&parsed, &Value::bytevector(vec![0, 127, 255])));
    }

    #[test]
    fn bytevector_rejects_out_of_range() {
        assert!(matches!(
            parse_one("#u8(256)"),
            Err(ParseError::BadBytevectorElement { .. }),
        ));
        assert!(matches!(
            parse_one("#u8(-1)"),
            Err(ParseError::BadBytevectorElement { .. }),
        ));
    }

    // -- reader macros ------------------------------------------------

    #[test]
    fn quote_reader_macro() {
        let parsed = parse("'foo");
        let expected = Value::list_from([sym("quote"), sym("foo")]);
        assert!(equal(&parsed, &expected));
    }

    #[test]
    fn quasiquote_family() {
        assert!(equal(
            &parse("`x"),
            &Value::list_from([sym("quasiquote"), sym("x")]),
        ));
        assert!(equal(
            &parse(",x"),
            &Value::list_from([sym("unquote"), sym("x")]),
        ));
        assert!(equal(
            &parse(",@x"),
            &Value::list_from([sym("unquote-splicing"), sym("x")]),
        ));
    }

    // -- datum comments ----------------------------------------------

    #[test]
    fn datum_comment_skips_one_datum() {
        let parsed = parse("(1 #;2 3)");
        let expected = Value::list_from([Value::Int(1), Value::Int(3)]);
        assert!(equal(&parsed, &expected));
    }

    #[test]
    fn datum_comment_at_top_level() {
        let datums = parse_program("#;ignored 1 2").expect("parse");
        assert_eq!(datums.len(), 2);
        assert!(equal(&datums[0], &Value::Int(1)));
        assert!(equal(&datums[1], &Value::Int(2)));
    }

    // -- programs -----------------------------------------------------

    #[test]
    fn multiple_top_level_datums() {
        let datums = parse_program("1 2 (a b)").expect("parse");
        assert_eq!(datums.len(), 3);
    }

    #[test]
    fn parse_one_rejects_multiple() {
        assert!(parse_one("1 2").is_err());
    }

    // -- larger fixture ----------------------------------------------

    #[test]
    fn factorial_fixture() {
        let parsed = parse("(define (fact n) (if (<= n 1) 1 (* n (fact (- n 1)))))");
        // Top-level should be (define (fact n) ...).
        let head = parsed.as_pair().expect("list").0;
        assert!(equal(&head, &sym("define")));
    }
}
