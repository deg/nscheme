//! (scheme show) / (srfi 159) tests — bead nscheme-oeg.8.
//!
//! Cases are mined and translated from the SRFI 159 reference test
//! suite (Duy Nguyen's portable reorganisation of Alex Shinn's chibi
//! `show`, BSD). Each upstream `(test EXPECTED (show #f …))` is
//! translated to a Scheme `(string=? EXPECTED (show #f …))` and
//! asserted with `assert_true`, matching this repo's test convention
//! (cf. `tests/srfi_128_comparator.rs`).
//!
//! Only the base + color combinators are exercised — the columnar and
//! unicode sub-libraries are not vendored (they need SRFI 117/130/151).

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::library::set_search_path;
use nscheme::value::Value;

/// The library lives in the repo's real `lib/` tree.
fn lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/lib", env!("CARGO_MANIFEST_DIR")))
}

const PRELUDE: &str = "(import (scheme base) (scheme show))\n";

fn run(expr: &str) -> Result<Value, EvalError> {
    set_search_path(vec![lib_dir()]);
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&format!("{PRELUDE}{expr}"), env)
}

/// Assert that a Scheme expression evaluates to #t.
fn assert_true(expr: &str) {
    match run(expr) {
        Ok(Value::Bool(true)) => {}
        Ok(other) => panic!("expected #t from `{expr}`, got {other}"),
        Err(e) => panic!("error evaluating `{expr}`: {e:?}"),
    }
}

#[test]
fn displayed_strings_and_chars() {
    assert_true(r#"(string=? "hi" (show #f "hi"))"#);
    assert_true(r#"(string=? "ab" (show #f "a" nothing "b"))"#);
    // each combines via displayed; a list element list is written.
    assert_true(r#"(string=? "abc" (show #f "a" "b" "c"))"#);
}

#[test]
fn written_objects() {
    assert_true(r#"(string=? "\"hi\"" (show #f (written "hi")))"#);
    assert_true(r#"(string=? "(1 2 3)" (show #f (written '(1 2 3))))"#);
    assert_true(r#"(string=? "(1 2 . 3)" (show #f (written '(1 2 . 3))))"#);
    assert_true(r##"(string=? "#(1 2 3)" (show #f (written '#(1 2 3))))"##);
}

#[test]
fn spacing_and_tabs() {
    assert_true(r#"(string=? "a    b" (show #f "a" (space-to 5) "b"))"#);
    assert_true(r#"(string=? "ab" (show #f "a" (space-to 0) "b"))"#);
    assert_true(r#"(string=? "abc  def" (show #f "abc" (tab-to 5) "def"))"#);
    assert_true(r#"(string=? "abcdef" (show #f "abc" (tab-to 3) "def"))"#);
}

#[test]
fn newlines_and_fresh_lines() {
    assert_true(r#"(string=? "abc\ndef\n" (show #f "abc" nl "def" nl))"#);
    assert_true(r#"(string=? "abc\ndef\n" (show #f "abc" fl "def" nl fl))"#);
}

#[test]
fn numeric_basics() {
    assert_true(r#"(string=? "-1" (show #f (numeric -1)))"#);
    assert_true(r#"(string=? "0" (show #f (numeric 0)))"#);
    assert_true(r#"(string=? "100" (show #f (numeric 100)))"#);
    assert_true(r#"(string=? "3/4" (show #f (numeric #e.75)))"#);
}

#[test]
fn numeric_precision() {
    assert_true(r#"(string=? "3.14" (show #f (with ((precision 2)) 3.14159)))"#);
    assert_true(r#"(string=? "3.00" (show #f (with ((precision 2)) 3.)))"#);
    assert_true(r#"(string=? "0.99" (show #f (with ((precision 2)) .99)))"#);
}

#[test]
fn numeric_radix() {
    assert_true(r#"(string=? "1001" (show #f (numeric 9 2)))"#);
    assert_true(r#"(string=? "11" (show #f (numeric 4 3)))"#);
    assert_true(r#"(string=? "57005" (show #f #xDEAD))"#);
}

#[test]
fn numeric_comma() {
    assert_true(
        r#"(string=? "299,792,458" (show #f (with ((comma-rule 3)) (numeric 299792458))))"#,
    );
    assert_true(r#"(string=? "100,000" (show #f (with ((comma-rule 3)) (numeric 100000))))"#);
}

#[test]
fn numeric_fitted() {
    assert_true(r#"(string=? "1.23" (show #f (numeric/fitted 4 1.2345 10 2)))"#);
    assert_true(r##"(string=? "#.##" (show #f (numeric/fitted 4 12.345 10 2)))"##);
}

#[test]
fn padded() {
    assert_true(r#"(string=? "abc  " (show #f (padded/right 5 "abc")))"#);
    assert_true(r#"(string=? "  abc" (show #f (padded 5 "abc")))"#);
    assert_true(r#"(string=? " abc " (show #f (padded/both 5 "abc")))"#);
    assert_true(r#"(string=? "abcdefghi" (show #f (padded 5 "abcdefghi")))"#);
}

#[test]
fn trimmed() {
    assert_true(r#"(string=? "abc" (show #f (trimmed/right 3 "abcde")))"#);
    assert_true(r#"(string=? "cde" (show #f (trimmed 3 "abcde")))"#);
    assert_true(r#"(string=? "bcd" (show #f (trimmed/both 3 "abcde")))"#);
    assert_true(r#"(string=? "abc" (show #f (trimmed/lazy 3 "abcde")))"#);
    assert_true(r#"(string=? "abc" (show #f (trimmed/lazy 3 "abc\nde")))"#);
}

#[test]
fn fitted() {
    assert_true(r#"(string=? "abc  " (show #f (fitted/right 5 "abc")))"#);
    assert_true(r#"(string=? "  abc" (show #f (fitted 5 "abc")))"#);
    assert_true(r#"(string=? " abc " (show #f (fitted/both 5 "abc")))"#);
    assert_true(r#"(string=? "defgh" (show #f (fitted 5 "abcdefgh")))"#);
}

#[test]
fn joined() {
    assert_true(r#"(string=? "1 2 3" (show #f (joined each '(1 2 3) " ")))"#);
}

#[test]
fn escaped() {
    assert_true(r#"(string=? "hi, bob!" (show #f (escaped "hi, bob!")))"#);
    assert_true(r#"(string=? "hi, \\'bob\\'" (show #f (escaped "hi, 'bob'" #\')))"#);
    assert_true(r#"(string=? "hi, ''bob''" (show #f (escaped "hi, 'bob'" #\' #\')))"#);
}

#[test]
fn maybe_escaped() {
    assert_true(r#"(string=? "bob" (show #f (maybe-escaped "bob" char-whitespace?)))"#);
}

#[test]
fn color_combinators_wrap_in_ansi_escapes() {
    // as-red wraps in CSI 31 … CSI 0 (reset back to the ambient color #f).
    assert_true(
        r#"(let ((s (show #f (as-red "x"))))
             (and (string=? s (string-append (string (integer->char 27)) "[31mx"
                                             (string (integer->char 27)) "[0m"))))"#,
    );
}
