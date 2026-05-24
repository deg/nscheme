//! Lexer for R7RS-small Scheme source text.
//!
//! Converts a UTF-8 source string into a stream of [`Token`]s. The lexer
//! is permissive about what it emits (e.g. it preserves the raw text of
//! numbers rather than parsing them to a numeric type) so that downstream
//! beads — particularly the numeric tower (nscheme-c92) — can decide how
//! to interpret each lexeme. Errors carry a [`Span`] for diagnostics.
//!
//! The grammar implemented here corresponds to R7RS §7.1.1 (lexical
//! syntax). Complex number syntax (`a+bi`, `a@b`) is recognized but
//! parsed lazily — the lexer emits the entire complex literal as a
//! single [`TokenKind::Number`] lexeme and lets the numeric tower decide
//! whether to support it.

use std::fmt;

use thiserror::Error;

/// Byte offsets into the original source string. Half-open: `start..end`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// The exactness annotation, if any, that prefixed a numeric literal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exactness {
    /// No `#e` or `#i` prefix; exactness inferred from the literal shape.
    Default,
    /// `#e` prefix — force exact.
    Exact,
    /// `#i` prefix — force inexact.
    Inexact,
}

/// A numeric literal as recognized by the lexer. The raw text (sans the
/// `#e`/`#i`/`#b`/`#o`/`#d`/`#x` prefixes) is preserved verbatim so the
/// numeric tower can do precise parsing later.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumberLexeme {
    /// Numeric radix: 2, 8, 10, or 16.
    pub radix: u32,
    /// Exactness annotation as written.
    pub exactness: Exactness,
    /// The body of the number, after radix/exactness prefixes have been
    /// consumed. Includes sign, digits, decimal point, exponent, and any
    /// rational `a/b` or complex `a+bi` / `a@b` shape.
    pub body: String,
}

/// All token kinds the lexer can produce.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `#(` — start of a vector literal.
    VectorStart,
    /// `#u8(` — start of a bytevector literal.
    BytevectorStart,
    /// `'`
    Quote,
    /// `` ` ``
    Quasiquote,
    /// `,`
    Unquote,
    /// `,@`
    UnquoteSplicing,
    /// `#;` — the parser elides the following datum.
    DatumComment,
    /// `.` — used both for improper-list dotted pairs and (rarely) as
    /// part of a peculiar identifier; the parser disambiguates.
    Dot,
    /// `#t`, `#f`, `#true`, `#false`.
    Boolean(bool),
    /// A numeric literal.
    Number(NumberLexeme),
    /// A character literal.
    Character(char),
    /// A string literal, with escapes already resolved.
    String(String),
    /// An identifier (a regular one, a `|...|`-quoted one, or a peculiar
    /// identifier like `+`, `-`, or `...`).
    Identifier(String),
    /// `#N=` — R7RS datum-label definition (introduces a label that
    /// will refer to the following datum).
    DatumLabel(u64),
    /// `#N#` — R7RS datum-label reference (back-reference to an
    /// earlier `#N=…` label).
    DatumRef(u64),
    /// `#!fold-case` directive — switches the reader into
    /// case-insensitive identifier folding (R7RS §2.1).
    FoldCase,
    /// `#!no-fold-case` directive — restores case-sensitive
    /// identifier folding.
    NoFoldCase,
}

/// A token paired with its source span.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// Lexical errors. All variants carry a [`Span`] so the caller can render
/// a diagnostic against the original source.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LexError {
    #[error("unexpected character {ch:?} at byte {}", span.start)]
    UnexpectedChar { ch: char, span: Span },

    #[error("unterminated string starting at byte {}", span.start)]
    UnterminatedString { span: Span },

    #[error("unterminated block comment starting at byte {}", span.start)]
    UnterminatedBlockComment { span: Span },

    #[error("unterminated |…| identifier starting at byte {}", span.start)]
    UnterminatedQuotedIdentifier { span: Span },

    #[error("invalid escape sequence at byte {}", span.start)]
    InvalidEscape { span: Span },

    #[error("invalid hex scalar value at byte {}", span.start)]
    InvalidHexScalar { span: Span },

    #[error("invalid character literal at byte {}", span.start)]
    InvalidCharacter { span: Span },

    #[error("invalid `#` syntax at byte {}", span.start)]
    InvalidHashSyntax { span: Span },

    #[error("conflicting number prefixes at byte {}", span.start)]
    ConflictingNumberPrefix { span: Span },
}

/// Tokenize an entire source string.
pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();
    while let Some(tok) = lexer.next_token()? {
        tokens.push(tok);
    }
    Ok(tokens)
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    /// Return the next non-trivia token, or `None` at end of input.
    /// Trivia = whitespace, `;` line comments, and `#| … |#` block
    /// comments (which nest per R7RS).
    fn next_token(&mut self) -> Result<Option<Token>, LexError> {
        self.skip_trivia()?;
        let start = self.pos;
        let Some(c) = self.peek_char() else {
            return Ok(None);
        };
        match c {
            '(' => {
                self.advance(c);
                Ok(Some(self.tok(TokenKind::LParen, start)))
            }
            ')' => {
                self.advance(c);
                Ok(Some(self.tok(TokenKind::RParen, start)))
            }
            '\'' => {
                self.advance(c);
                Ok(Some(self.tok(TokenKind::Quote, start)))
            }
            '`' => {
                self.advance(c);
                Ok(Some(self.tok(TokenKind::Quasiquote, start)))
            }
            ',' => {
                self.advance(c);
                if self.peek_char() == Some('@') {
                    self.advance('@');
                    return Ok(Some(self.tok(TokenKind::UnquoteSplicing, start)));
                }
                Ok(Some(self.tok(TokenKind::Unquote, start)))
            }
            '"' => self.lex_string(start).map(Some),
            '|' => self.lex_quoted_identifier(start).map(Some),
            '#' => self.lex_hash(start).map(Some),
            c if c.is_ascii_digit() => Ok(Some(self.lex_number_default(start))),
            '+' | '-' => Ok(Some(self.lex_sign_starting(start))),
            '.' => Ok(Some(self.lex_dot_starting(start))),
            c if is_initial(c) => Ok(Some(self.lex_identifier(start))),
            other => {
                let span = char_span_of(start, other);
                self.advance(other);
                Err(LexError::UnexpectedChar { ch: other, span })
            }
        }
    }

    fn skip_trivia(&mut self) -> Result<(), LexError> {
        loop {
            let Some(c) = self.peek_char() else {
                return Ok(());
            };
            if c.is_whitespace() {
                self.advance(c);
                continue;
            }
            if c == ';' {
                // Line comment.
                while let Some(c) = self.peek_char() {
                    self.advance(c);
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }
            if c == '#' && self.peek_byte_at(1) == Some(b'|') {
                self.lex_block_comment()?;
                continue;
            }
            return Ok(());
        }
    }

    fn lex_block_comment(&mut self) -> Result<(), LexError> {
        let start = self.pos;
        self.advance('#');
        self.advance('|');
        let mut depth: usize = 1;
        while depth > 0 {
            let Some(c) = self.peek_char() else {
                return Err(LexError::UnterminatedBlockComment {
                    span: Span::new(start, self.pos),
                });
            };
            if c == '|' && self.peek_byte_at(1) == Some(b'#') {
                self.advance('|');
                self.advance('#');
                depth -= 1;
            } else if c == '#' && self.peek_byte_at(1) == Some(b'|') {
                self.advance('#');
                self.advance('|');
                depth += 1;
            } else {
                self.advance(c);
            }
        }
        Ok(())
    }

    fn lex_string(&mut self, start: usize) -> Result<Token, LexError> {
        self.advance('"');
        let mut out = String::new();
        loop {
            let Some(c) = self.peek_char() else {
                return Err(LexError::UnterminatedString {
                    span: Span::new(start, self.pos),
                });
            };
            if c == '"' {
                self.advance('"');
                return Ok(self.tok(TokenKind::String(out), start));
            }
            if c == '\\' {
                self.advance('\\');
                self.lex_string_escape(start, &mut out)?;
                continue;
            }
            self.advance(c);
            out.push(c);
        }
    }

    fn lex_string_escape(&mut self, str_start: usize, out: &mut String) -> Result<(), LexError> {
        let esc_start = self.pos - 1; // the '\\' we just consumed
        let Some(c) = self.peek_char() else {
            return Err(LexError::UnterminatedString {
                span: Span::new(str_start, self.pos),
            });
        };
        match c {
            'a' => {
                self.advance(c);
                out.push('\u{0007}');
            }
            'b' => {
                self.advance(c);
                out.push('\u{0008}');
            }
            't' => {
                self.advance(c);
                out.push('\t');
            }
            'n' => {
                self.advance(c);
                out.push('\n');
            }
            'r' => {
                self.advance(c);
                out.push('\r');
            }
            '"' => {
                self.advance(c);
                out.push('"');
            }
            '\\' => {
                self.advance(c);
                out.push('\\');
            }
            '|' => {
                self.advance(c);
                out.push('|');
            }
            'x' => {
                self.advance(c);
                let scalar = self.lex_hex_scalar(esc_start)?;
                if self.peek_char() != Some(';') {
                    return Err(LexError::InvalidEscape {
                        span: Span::new(esc_start, self.pos),
                    });
                }
                self.advance(';');
                out.push(scalar);
            }
            // Line continuation: \ <intraline whitespace>* <line ending> <intraline whitespace>*
            ws if ws == ' ' || ws == '\t' || ws == '\n' || ws == '\r' => {
                self.skip_intraline_whitespace();
                let Some(nl) = self.peek_char() else {
                    return Err(LexError::InvalidEscape {
                        span: Span::new(esc_start, self.pos),
                    });
                };
                if nl != '\n' && nl != '\r' {
                    return Err(LexError::InvalidEscape {
                        span: Span::new(esc_start, self.pos),
                    });
                }
                // Consume the line ending (handle CRLF as one).
                if nl == '\r' {
                    self.advance('\r');
                    if self.peek_char() == Some('\n') {
                        self.advance('\n');
                    }
                } else {
                    self.advance('\n');
                }
                self.skip_intraline_whitespace();
            }
            _ => {
                return Err(LexError::InvalidEscape {
                    span: Span::new(esc_start, self.pos + c.len_utf8()),
                });
            }
        }
        Ok(())
    }

    fn lex_hex_scalar(&mut self, esc_start: usize) -> Result<char, LexError> {
        let digits_start = self.pos;
        while matches!(self.peek_char(), Some(c) if c.is_ascii_hexdigit()) {
            let c = self.peek_char().unwrap();
            self.advance(c);
        }
        let digits = &self.src[digits_start..self.pos];
        if digits.is_empty() {
            return Err(LexError::InvalidHexScalar {
                span: Span::new(esc_start, self.pos),
            });
        }
        let code = u32::from_str_radix(digits, 16).map_err(|_| LexError::InvalidHexScalar {
            span: Span::new(esc_start, self.pos),
        })?;
        char::from_u32(code).ok_or(LexError::InvalidHexScalar {
            span: Span::new(esc_start, self.pos),
        })
    }

    fn skip_intraline_whitespace(&mut self) {
        while matches!(self.peek_char(), Some(' ' | '\t')) {
            let c = self.peek_char().unwrap();
            self.advance(c);
        }
    }

    fn lex_quoted_identifier(&mut self, start: usize) -> Result<Token, LexError> {
        self.advance('|');
        let mut out = String::new();
        loop {
            let Some(c) = self.peek_char() else {
                return Err(LexError::UnterminatedQuotedIdentifier {
                    span: Span::new(start, self.pos),
                });
            };
            if c == '|' {
                self.advance('|');
                return Ok(self.tok(TokenKind::Identifier(out), start));
            }
            if c == '\\' {
                self.advance('\\');
                self.lex_string_escape(start, &mut out)?;
                continue;
            }
            self.advance(c);
            out.push(c);
        }
    }

    fn lex_hash(&mut self, start: usize) -> Result<Token, LexError> {
        self.advance('#');
        let Some(c) = self.peek_char() else {
            return Err(LexError::InvalidHashSyntax {
                span: Span::new(start, self.pos),
            });
        };
        match c {
            't' => Ok(self.lex_boolean(start, true)),
            'f' => Ok(self.lex_boolean(start, false)),
            '(' => {
                self.advance('(');
                Ok(self.tok(TokenKind::VectorStart, start))
            }
            'u' => self.lex_bytevector_start(start),
            '\\' => self.lex_character(start),
            ';' => {
                self.advance(';');
                Ok(self.tok(TokenKind::DatumComment, start))
            }
            '!' => self.lex_directive(start),
            '0'..='9' => self.lex_datum_label(start),
            'e' | 'E' | 'i' | 'I' | 'b' | 'B' | 'o' | 'O' | 'd' | 'D' | 'x' | 'X' => {
                self.lex_number_prefixed(start)
            }
            _ => {
                self.advance(c);
                Err(LexError::InvalidHashSyntax {
                    span: Span::new(start, self.pos),
                })
            }
        }
    }

    /// Lex `#N=` (`DatumLabel`) or `#N#` (`DatumRef`).
    fn lex_datum_label(&mut self, start: usize) -> Result<Token, LexError> {
        let mut n: u64 = 0;
        while let Some(c) = self.peek_char() {
            if let Some(d) = c.to_digit(10) {
                n = n.saturating_mul(10).saturating_add(u64::from(d));
                self.advance(c);
            } else {
                break;
            }
        }
        match self.peek_char() {
            Some('=') => {
                self.advance('=');
                Ok(self.tok(TokenKind::DatumLabel(n), start))
            }
            Some('#') => {
                self.advance('#');
                Ok(self.tok(TokenKind::DatumRef(n), start))
            }
            _ => Err(LexError::InvalidHashSyntax {
                span: Span::new(start, self.pos),
            }),
        }
    }

    /// Lex `#!fold-case` / `#!no-fold-case` directives.
    fn lex_directive(&mut self, start: usize) -> Result<Token, LexError> {
        self.advance('!');
        let dir_start = self.pos;
        while let Some(c) = self.peek_char() {
            if is_delimiter(c) {
                break;
            }
            self.advance(c);
        }
        let name = &self.src[dir_start..self.pos];
        match name {
            "fold-case" => Ok(self.tok(TokenKind::FoldCase, start)),
            "no-fold-case" => Ok(self.tok(TokenKind::NoFoldCase, start)),
            _ => Err(LexError::InvalidHashSyntax {
                span: Span::new(start, self.pos),
            }),
        }
    }

    fn lex_boolean(&mut self, start: usize, value: bool) -> Token {
        // Accept #t, #f, #true, #false.
        let long = if value { "true" } else { "false" };
        let short = if value { 't' } else { 'f' };
        // First consume the short form character.
        self.advance(short);
        // If the next chars spell out the long suffix, consume it.
        let suffix = &long[1..]; // "rue" or "alse"
        if self.src[self.pos..].starts_with(suffix) {
            let next_after = self.src[self.pos + suffix.len()..].chars().next();
            // Only consume if the longer form is followed by a delimiter
            // (or EOF). #trues should not lex as #true followed by `s`.
            if next_after.is_none_or(is_delimiter) {
                for _ in suffix.chars() {
                    let ch = self.peek_char().unwrap();
                    self.advance(ch);
                }
            }
        }
        self.tok(TokenKind::Boolean(value), start)
    }

    fn lex_bytevector_start(&mut self, start: usize) -> Result<Token, LexError> {
        self.advance('u');
        if self.peek_char() == Some('8') && self.peek_byte_at(1) == Some(b'(') {
            self.advance('8');
            self.advance('(');
            return Ok(self.tok(TokenKind::BytevectorStart, start));
        }
        Err(LexError::InvalidHashSyntax {
            span: Span::new(start, self.pos),
        })
    }

    fn lex_character(&mut self, start: usize) -> Result<Token, LexError> {
        self.advance('\\');
        let Some(first) = self.peek_char() else {
            return Err(LexError::InvalidCharacter {
                span: Span::new(start, self.pos),
            });
        };
        // Read the run of non-delimiter chars after #\.
        let run_start = self.pos;
        self.advance(first);
        while let Some(c) = self.peek_char() {
            if is_delimiter(c) {
                break;
            }
            self.advance(c);
        }
        let run = &self.src[run_start..self.pos];
        // Single-character literal: just one char in the run.
        if run.chars().count() == 1 {
            let ch = run.chars().next().unwrap();
            return Ok(self.tok(TokenKind::Character(ch), start));
        }
        // #\xHEX form.
        if let Some(hex) = run.strip_prefix('x') {
            let code = u32::from_str_radix(hex, 16).map_err(|_| LexError::InvalidCharacter {
                span: Span::new(start, self.pos),
            })?;
            let ch = char::from_u32(code).ok_or(LexError::InvalidCharacter {
                span: Span::new(start, self.pos),
            })?;
            return Ok(self.tok(TokenKind::Character(ch), start));
        }
        // Named characters.
        let named = match run {
            "alarm" => Some('\u{0007}'),
            "backspace" => Some('\u{0008}'),
            "delete" => Some('\u{007F}'),
            "escape" => Some('\u{001B}'),
            "newline" => Some('\n'),
            "null" => Some('\0'),
            "return" => Some('\r'),
            "space" => Some(' '),
            "tab" => Some('\t'),
            _ => None,
        };
        named
            .map(|ch| self.tok(TokenKind::Character(ch), start))
            .ok_or(LexError::InvalidCharacter {
                span: Span::new(start, self.pos),
            })
    }

    fn lex_number_prefixed(&mut self, start: usize) -> Result<Token, LexError> {
        // We've already consumed one '#' in lex_hash; the current peek
        // is the first prefix letter. R7RS allows at most one radix and
        // one exactness prefix, in either order. Each prefix letter is
        // introduced by its own '#'.
        let mut radix: Option<u32> = None;
        let mut exactness = Exactness::Default;
        while let Some(c) = self.peek_char() {
            let conflict = || LexError::ConflictingNumberPrefix {
                span: Span::new(start, self.pos),
            };
            match c {
                'b' | 'B' => {
                    if radix.is_some() {
                        return Err(conflict());
                    }
                    radix = Some(2);
                    self.advance(c);
                }
                'o' | 'O' => {
                    if radix.is_some() {
                        return Err(conflict());
                    }
                    radix = Some(8);
                    self.advance(c);
                }
                'd' | 'D' => {
                    if radix.is_some() {
                        return Err(conflict());
                    }
                    radix = Some(10);
                    self.advance(c);
                }
                'x' | 'X' => {
                    if radix.is_some() {
                        return Err(conflict());
                    }
                    radix = Some(16);
                    self.advance(c);
                }
                'e' | 'E' => {
                    if !matches!(exactness, Exactness::Default) {
                        return Err(conflict());
                    }
                    exactness = Exactness::Exact;
                    self.advance(c);
                }
                'i' | 'I' => {
                    if !matches!(exactness, Exactness::Default) {
                        return Err(conflict());
                    }
                    exactness = Exactness::Inexact;
                    self.advance(c);
                }
                _ => break,
            }
            // A second prefix is introduced by another '#'.
            if self.peek_char() == Some('#') {
                self.advance('#');
            } else {
                break;
            }
        }
        let body = self.consume_number_body();
        if body.is_empty() {
            return Err(LexError::InvalidHashSyntax {
                span: Span::new(start, self.pos),
            });
        }
        Ok(self.tok(
            TokenKind::Number(NumberLexeme {
                radix: radix.unwrap_or(10),
                exactness,
                body,
            }),
            start,
        ))
    }

    fn lex_number_default(&mut self, start: usize) -> Token {
        let body = self.consume_number_body();
        self.tok(
            TokenKind::Number(NumberLexeme {
                radix: 10,
                exactness: Exactness::Default,
                body,
            }),
            start,
        )
    }

    /// Consume the body of a number — sign, digits, decimal, exponent,
    /// rational separator, imaginary suffix, etc. Stops at a delimiter.
    /// Lexical validity beyond "characters that can appear in a number"
    /// is deferred to the numeric parser.
    fn consume_number_body(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if is_delimiter(c) {
                break;
            }
            self.advance(c);
        }
        self.src[start..self.pos].to_string()
    }

    fn lex_sign_starting(&mut self, start: usize) -> Token {
        // The current char is '+' or '-'. Decide between:
        //   - a number (sign followed by digit / dot / "inf.0" / "nan.0"
        //     / a bare `i` for the complex unit imaginary)
        //   - a peculiar identifier (just `+`, `-`, or sign followed by sign-subsequent…)
        let sign = self.peek_char().unwrap();
        let next = self.peek_char_at(1);
        let is_number = match next {
            Some(c) if c.is_ascii_digit() => true,
            Some('.') => matches!(self.peek_char_at(2), Some(c) if c.is_ascii_digit()),
            Some('i' | 'I' | 'n' | 'N') => {
                // +inf.0 / +nan.0 — five-char identifiers.
                let rest = &self.src[self.pos + sign.len_utf8()..];
                let prefix5: String = rest.chars().take(5).collect();
                if prefix5.eq_ignore_ascii_case("inf.0")
                    || prefix5.eq_ignore_ascii_case("nan.0")
                {
                    true
                } else {
                    // R7RS `+i` / `-i` complex unit. The trailing `i`
                    // must be followed by a delimiter — otherwise we're
                    // looking at an identifier like `+inf.0xyz`.
                    matches!(next, Some('i' | 'I'))
                        && self.peek_char_at(2).is_none_or(is_delimiter)
                }
            }
            _ => false,
        };
        if is_number {
            return self.lex_number_default(start);
        }
        self.lex_identifier(start)
    }

    fn lex_dot_starting(&mut self, start: usize) -> Token {
        // A `.` followed by a digit is a number (`.5`). A standalone `.`
        // (followed by a delimiter) is the dotted-pair separator. A `.`
        // followed by a dot-subsequent is a peculiar identifier (e.g.
        // `...`).
        let next = self.peek_char_at(1);
        match next {
            Some(c) if c.is_ascii_digit() => self.lex_number_default(start),
            Some(c) if is_delimiter(c) => {
                self.advance('.');
                self.tok(TokenKind::Dot, start)
            }
            None => {
                self.advance('.');
                self.tok(TokenKind::Dot, start)
            }
            _ => self.lex_identifier(start),
        }
    }

    fn lex_identifier(&mut self, start: usize) -> Token {
        let first = self.peek_char().unwrap();
        self.advance(first);
        while let Some(c) = self.peek_char() {
            if is_delimiter(c) {
                break;
            }
            self.advance(c);
        }
        let text = &self.src[start..self.pos];
        self.tok(TokenKind::Identifier(text.to_string()), start)
    }

    // -------------------------------------------------------------
    // Low-level cursor helpers.
    // -------------------------------------------------------------

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_char_at(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    fn peek_byte_at(&self, n: usize) -> Option<u8> {
        self.bytes.get(self.pos + n).copied()
    }

    fn advance(&mut self, c: char) {
        self.pos += c.len_utf8();
    }

    fn tok(&self, kind: TokenKind, start: usize) -> Token {
        Token {
            kind,
            span: Span::new(start, self.pos),
        }
    }
}

fn char_span_of(start: usize, c: char) -> Span {
    Span::new(start, start + c.len_utf8())
}

fn is_initial(c: char) -> bool {
    c.is_ascii_alphabetic()
        || matches!(
            c,
            '!' | '$' | '%' | '&' | '*' | '/' | ':' | '<' | '=' | '>' | '?' | '^' | '_' | '~'
        )
}

/// A delimiter terminates a token. R7RS §7.1.1: whitespace, parens,
/// `"`, `;`, `|`, and `#` (when it starts a new token).
fn is_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '"' | ';' | '|')
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input)
            .expect("lex")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    fn one(input: &str) -> TokenKind {
        let toks = tokenize(input).expect("lex");
        assert_eq!(
            toks.len(),
            1,
            "expected exactly one token in {input:?}, got {toks:?}"
        );
        toks.into_iter().next().unwrap().kind
    }

    // -- structural punctuation ---------------------------------------

    #[test]
    fn parens() {
        assert_eq!(kinds("()"), vec![TokenKind::LParen, TokenKind::RParen]);
    }

    #[test]
    fn nested_parens_and_whitespace() {
        assert_eq!(
            kinds("(  ( ))"),
            vec![
                TokenKind::LParen,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::RParen
            ],
        );
    }

    #[test]
    fn vector_and_bytevector_starts() {
        assert_eq!(
            kinds("#(1) #u8(2)"),
            vec![
                TokenKind::VectorStart,
                TokenKind::Number(NumberLexeme {
                    radix: 10,
                    exactness: Exactness::Default,
                    body: "1".into(),
                }),
                TokenKind::RParen,
                TokenKind::BytevectorStart,
                TokenKind::Number(NumberLexeme {
                    radix: 10,
                    exactness: Exactness::Default,
                    body: "2".into(),
                }),
                TokenKind::RParen,
            ],
        );
    }

    // -- reader macros ------------------------------------------------

    #[test]
    fn quote_family() {
        assert_eq!(
            kinds("'a `b ,c ,@d"),
            vec![
                TokenKind::Quote,
                TokenKind::Identifier("a".into()),
                TokenKind::Quasiquote,
                TokenKind::Identifier("b".into()),
                TokenKind::Unquote,
                TokenKind::Identifier("c".into()),
                TokenKind::UnquoteSplicing,
                TokenKind::Identifier("d".into()),
            ],
        );
    }

    #[test]
    fn datum_comment_token() {
        // The lexer emits the marker; the parser elides the following datum.
        assert_eq!(
            kinds("(a #;b c)"),
            vec![
                TokenKind::LParen,
                TokenKind::Identifier("a".into()),
                TokenKind::DatumComment,
                TokenKind::Identifier("b".into()),
                TokenKind::Identifier("c".into()),
                TokenKind::RParen,
            ],
        );
    }

    // -- comments and whitespace --------------------------------------

    #[test]
    fn line_comment_skipped() {
        assert_eq!(
            kinds("a ; this is ignored\nb"),
            vec![
                TokenKind::Identifier("a".into()),
                TokenKind::Identifier("b".into()),
            ]
        );
    }

    #[test]
    fn block_comment_nested() {
        assert_eq!(
            kinds("a #| outer #| inner |# still outer |# b"),
            vec![
                TokenKind::Identifier("a".into()),
                TokenKind::Identifier("b".into()),
            ],
        );
    }

    #[test]
    fn block_comment_unterminated_is_error() {
        assert!(matches!(
            tokenize("a #| oops"),
            Err(LexError::UnterminatedBlockComment { .. }),
        ));
    }

    // -- booleans -----------------------------------------------------

    #[test]
    fn boolean_short_forms() {
        assert_eq!(one("#t"), TokenKind::Boolean(true));
        assert_eq!(one("#f"), TokenKind::Boolean(false));
    }

    #[test]
    fn boolean_long_forms() {
        assert_eq!(one("#true"), TokenKind::Boolean(true));
        assert_eq!(one("#false"), TokenKind::Boolean(false));
    }

    #[test]
    fn hash_t_followed_by_non_delimiter_is_short_form_then_identifier() {
        // #trues should not lex as one token; #t is the boolean, then "rues"
        // begins an identifier.
        let toks = tokenize("#trues").expect("lex");
        assert_eq!(toks[0].kind, TokenKind::Boolean(true));
        assert_eq!(toks[1].kind, TokenKind::Identifier("rues".into()));
    }

    // -- characters ---------------------------------------------------

    #[test]
    fn character_literal_single() {
        assert_eq!(one(r"#\a"), TokenKind::Character('a'));
        assert_eq!(one(r"#\ "), TokenKind::Character(' '));
        assert_eq!(one(r"#\("), TokenKind::Character('('));
    }

    #[test]
    fn character_literal_named() {
        assert_eq!(one(r"#\newline"), TokenKind::Character('\n'));
        assert_eq!(one(r"#\space"), TokenKind::Character(' '));
        assert_eq!(one(r"#\tab"), TokenKind::Character('\t'));
        assert_eq!(one(r"#\null"), TokenKind::Character('\0'));
        assert_eq!(one(r"#\delete"), TokenKind::Character('\u{007F}'));
    }

    #[test]
    fn character_literal_hex() {
        assert_eq!(one(r"#\x41"), TokenKind::Character('A'));
        assert_eq!(one(r"#\x3BB"), TokenKind::Character('λ'));
    }

    #[test]
    fn character_literal_invalid() {
        assert!(matches!(
            tokenize(r"#\notaname"),
            Err(LexError::InvalidCharacter { .. }),
        ));
    }

    // -- strings ------------------------------------------------------

    #[test]
    fn string_simple() {
        assert_eq!(one(r#""hello""#), TokenKind::String("hello".into()));
    }

    #[test]
    fn string_escapes() {
        assert_eq!(
            one(r#""a\nb\tc\\d\"e""#),
            TokenKind::String("a\nb\tc\\d\"e".into()),
        );
    }

    #[test]
    fn string_hex_escape() {
        // \x3BB; -> λ
        assert_eq!(one(r#""\x3BB;""#), TokenKind::String("λ".into()));
    }

    #[test]
    fn string_line_continuation() {
        // R7RS: \<intraline ws>* <line ending> <intraline ws>* is elided.
        assert_eq!(
            one("\"abc\\   \n   def\""),
            TokenKind::String("abcdef".into()),
        );
    }

    #[test]
    fn string_unterminated() {
        assert!(matches!(
            tokenize(r#""nope"#),
            Err(LexError::UnterminatedString { .. }),
        ));
    }

    #[test]
    fn string_bad_escape() {
        assert!(matches!(
            tokenize(r#""bad\q""#),
            Err(LexError::InvalidEscape { .. }),
        ));
    }

    // -- numbers ------------------------------------------------------

    fn num(body: &str) -> TokenKind {
        TokenKind::Number(NumberLexeme {
            radix: 10,
            exactness: Exactness::Default,
            body: body.into(),
        })
    }

    #[test]
    fn integer() {
        assert_eq!(one("0"), num("0"));
        assert_eq!(one("42"), num("42"));
        assert_eq!(one("-7"), num("-7"));
        assert_eq!(one("+7"), num("+7"));
    }

    #[test]
    fn decimal() {
        assert_eq!(one("3.14"), num("3.14"));
        assert_eq!(one(".5"), num(".5"));
        assert_eq!(one("-.5"), num("-.5"));
        assert_eq!(one("1e10"), num("1e10"));
        assert_eq!(one("-2.5e-3"), num("-2.5e-3"));
    }

    #[test]
    fn rational() {
        assert_eq!(one("3/4"), num("3/4"));
        assert_eq!(one("-7/2"), num("-7/2"));
    }

    #[test]
    fn radix_prefix() {
        assert_eq!(
            one("#b1010"),
            TokenKind::Number(NumberLexeme {
                radix: 2,
                exactness: Exactness::Default,
                body: "1010".into(),
            }),
        );
        assert_eq!(
            one("#xFF"),
            TokenKind::Number(NumberLexeme {
                radix: 16,
                exactness: Exactness::Default,
                body: "FF".into(),
            }),
        );
        assert_eq!(
            one("#o755"),
            TokenKind::Number(NumberLexeme {
                radix: 8,
                exactness: Exactness::Default,
                body: "755".into(),
            }),
        );
    }

    #[test]
    fn exactness_prefix() {
        assert_eq!(
            one("#e3.0"),
            TokenKind::Number(NumberLexeme {
                radix: 10,
                exactness: Exactness::Exact,
                body: "3.0".into(),
            }),
        );
        assert_eq!(
            one("#i5"),
            TokenKind::Number(NumberLexeme {
                radix: 10,
                exactness: Exactness::Inexact,
                body: "5".into(),
            }),
        );
    }

    #[test]
    fn radix_and_exactness_either_order() {
        let expected = TokenKind::Number(NumberLexeme {
            radix: 16,
            exactness: Exactness::Exact,
            body: "FF".into(),
        });
        assert_eq!(one("#e#xFF"), expected.clone());
        assert_eq!(one("#x#eFF"), expected);
    }

    #[test]
    fn infinity_and_nan() {
        assert_eq!(one("+inf.0"), num("+inf.0"));
        assert_eq!(one("-inf.0"), num("-inf.0"));
        assert_eq!(one("+nan.0"), num("+nan.0"));
    }

    // -- identifiers --------------------------------------------------

    #[test]
    fn identifier_simple() {
        assert_eq!(one("foo"), TokenKind::Identifier("foo".into()));
        assert_eq!(
            one("hello-world"),
            TokenKind::Identifier("hello-world".into())
        );
        assert_eq!(one("set!"), TokenKind::Identifier("set!".into()));
        assert_eq!(
            one("string->list"),
            TokenKind::Identifier("string->list".into())
        );
    }

    #[test]
    fn identifier_special_initials() {
        for s in [
            "!", "$x", "%x", "&x", "*", "/", ":x", "<", "=", ">", "?x", "^x", "_x", "~x",
        ] {
            let toks = tokenize(s).expect("lex");
            assert_eq!(toks.len(), 1, "{s:?} should be a single identifier");
            assert!(matches!(toks[0].kind, TokenKind::Identifier(_)), "{s:?}");
        }
    }

    #[test]
    fn identifier_peculiar() {
        assert_eq!(one("+"), TokenKind::Identifier("+".into()));
        assert_eq!(one("-"), TokenKind::Identifier("-".into()));
        assert_eq!(one("..."), TokenKind::Identifier("...".into()));
        assert_eq!(one("->foo"), TokenKind::Identifier("->foo".into()));
    }

    #[test]
    fn identifier_quoted() {
        assert_eq!(
            one("|hello world|"),
            TokenKind::Identifier("hello world".into())
        );
        assert_eq!(
            one(r"|with\nescape|"),
            TokenKind::Identifier("with\nescape".into())
        );
        assert_eq!(
            one(r"|with\|bar|"),
            TokenKind::Identifier("with|bar".into())
        );
    }

    #[test]
    fn identifier_quoted_unterminated() {
        assert!(matches!(
            tokenize("|oops"),
            Err(LexError::UnterminatedQuotedIdentifier { .. }),
        ));
    }

    // -- dot ----------------------------------------------------------

    #[test]
    fn dot_as_pair_separator() {
        assert_eq!(
            kinds("(a . b)"),
            vec![
                TokenKind::LParen,
                TokenKind::Identifier("a".into()),
                TokenKind::Dot,
                TokenKind::Identifier("b".into()),
                TokenKind::RParen,
            ],
        );
    }

    #[test]
    fn ellipsis_is_identifier_not_dot() {
        assert_eq!(one("..."), TokenKind::Identifier("...".into()));
    }

    // -- spans --------------------------------------------------------

    #[test]
    fn spans_match_source() {
        let toks = tokenize("(abc def)").expect("lex");
        assert_eq!(toks[0].span, Span::new(0, 1)); // '('
        assert_eq!(toks[1].span, Span::new(1, 4)); // abc
        assert_eq!(toks[2].span, Span::new(5, 8)); // def
        assert_eq!(toks[3].span, Span::new(8, 9)); // ')'
    }

    // -- larger fixture ----------------------------------------------

    #[test]
    fn fixture_mixed_program() {
        let src = r"
            ; factorial
            (define (fact n)
              (if (<= n 1)
                  1
                  (* n (fact (- n 1)))))
        ";
        let toks = tokenize(src).expect("lex");
        // Spot-check a few tokens rather than enumerate everything.
        assert!(
            toks.iter()
                .any(|t| matches!(&t.kind, TokenKind::Identifier(s) if s == "define"))
        );
        assert!(
            toks.iter()
                .any(|t| matches!(&t.kind, TokenKind::Identifier(s) if s == "fact"))
        );
        assert!(
            toks.iter()
                .any(|t| matches!(&t.kind, TokenKind::Identifier(s) if s == "<="))
        );
        assert!(
            toks.iter()
                .any(|t| matches!(&t.kind, TokenKind::Number(n) if n.body == "1"))
        );
    }
}
