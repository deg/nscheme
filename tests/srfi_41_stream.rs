//! (scheme stream) / (srfi 41) tests — bead nscheme-lul.12.
//!
//! Cases are mined and translated from the SRFI 41 reference test
//! suite (r6rs-test.ss, Philip L. Bewig, MIT). The upstream suite uses
//! its own `assert` macro over an R6RS environment; here each case is
//! run through `eval_source` and asserted, matching this repo's test
//! convention (see `tests/srfi_128_comparator.rs`).
//!
//! `(test EXPECTED EXPR)`-style upstream assertions become equality
//! checks on `run(EXPR)`; `(test-assert EXPR)`-style ones become
//! `assert_true(EXPR)`.

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::library::set_search_path;
use nscheme::value::{Value, equal};

/// The library lives in the repo's real `lib/` tree.
fn lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/lib", env!("CARGO_MANIFEST_DIR")))
}

/// A handful of streams reused across the mined cases.
const PRELUDE: &str = r"
(import (scheme base) (scheme stream))
(define strm123 (stream 1 2 3))
";

fn run(expr: &str) -> Result<Value, EvalError> {
    set_search_path(vec![lib_dir()]);
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&format!("{PRELUDE}\n{expr}"), env)
}

/// Assert that a Scheme expression evaluates to #t.
fn assert_true(expr: &str) {
    match run(expr) {
        Ok(Value::Bool(true)) => {}
        Ok(other) => panic!("expected #t from `{expr}`, got {other}"),
        Err(e) => panic!("error evaluating `{expr}`: {e:?}"),
    }
}

/// Assert that a Scheme expression is `equal?` to a second expression.
fn assert_equal(expr: &str, expected: &str) {
    let got = run(expr).unwrap_or_else(|e| panic!("error evaluating `{expr}`: {e:?}"));
    let want = run(expected).unwrap_or_else(|e| panic!("error evaluating `{expected}`: {e:?}"));
    assert!(equal(&got, &want), "expected `{expr}` => {want}, got {got}");
}

#[test]
fn primitive_predicates() {
    // stream-null / stream-cons / stream? / stream-null? / stream-pair?
    assert_true(
        "(and (stream? stream-null)
              (stream-null? stream-null)
              (not (stream-pair? stream-null))
              (stream? (stream-cons 1 stream-null))
              (not (stream-null? (stream-cons 1 stream-null)))
              (stream-pair? (stream-cons 1 stream-null))
              (not (stream? \"four\"))
              (not (stream-null? \"four\"))
              (not (stream-pair? \"four\")))",
    );
}

#[test]
fn car_and_cdr() {
    assert_equal("(stream-car strm123)", "1");
    assert_equal("(stream-car (stream-cdr strm123))", "2");
}

#[test]
fn list_and_stream_round_trip() {
    assert_equal("(stream->list (list->stream '()))", "'()");
    assert_equal("(stream->list (list->stream '(1 2 3)))", "'(1 2 3)");
    assert_equal("(stream->list (stream))", "'()");
    assert_equal("(stream->list (stream 1 2 3))", "'(1 2 3)");
    // bounded stream->list over an infinite stream
    assert_equal("(stream->list 3 (stream-from 1))", "'(1 2 3)");
}

#[test]
fn append_and_concat() {
    assert_equal(
        "(stream->list (stream-append strm123 strm123))",
        "'(1 2 3 1 2 3)",
    );
    assert_equal(
        "(stream->list (stream-append strm123 stream-null))",
        "'(1 2 3)",
    );
    assert_equal(
        "(stream->list (stream-concat (stream strm123 strm123)))",
        "'(1 2 3 1 2 3)",
    );
}

#[test]
fn constant_and_from_and_iterate() {
    assert_equal("(stream-ref (stream-constant 1) 100)", "1");
    assert_equal("(stream-ref (stream-constant 1 2 3) 3)", "1");
    assert_equal("(stream-ref (stream-from 0) 100)", "100");
    assert_equal("(stream-ref (stream-from 1 2) 100)", "201");
    assert_equal(
        "(stream->list 3 (stream-iterate (lambda (n) (+ n 1)) 1))",
        "'(1 2 3)",
    );
}

#[test]
fn drop_take_filter() {
    assert_equal("(stream->list (stream-drop 1 strm123))", "'(2 3)");
    assert_equal("(stream->list (stream-drop-while odd? strm123))", "'(2 3)");
    assert_equal("(stream->list (stream-take 2 strm123))", "'(1 2)");
    assert_equal("(stream->list (stream-take-while odd? strm123))", "'(1)");
    assert_equal("(stream->list (stream-filter odd? strm123))", "'(1 3)");
    assert_equal("(stream->list (stream-filter even? strm123))", "'(2)");
    // filter over an infinite stream stays lazy
    assert_true("(odd? (stream-ref (stream-filter odd? (stream-from 0)) 10))");
}

#[test]
fn fold_scan_length() {
    assert_equal("(stream-fold + 0 strm123)", "6");
    assert_equal("(stream->list (stream-scan + 0 strm123))", "'(0 1 3 6)");
    assert_equal("(stream-length strm123)", "3");
    assert_equal("(stream-length (stream))", "0");
}

#[test]
fn for_each_side_effects() {
    assert_equal(
        "(let ((sum 0)) (stream-for-each (lambda (x) (set! sum (+ sum x))) strm123) sum)",
        "6",
    );
}

#[test]
fn map_and_zip() {
    assert_equal("(stream->list (stream-map - strm123))", "'(-1 -2 -3)");
    assert_equal("(stream->list (stream-map + strm123 strm123))", "'(2 4 6)");
    assert_equal(
        "(stream->list (stream-map + strm123 (stream-from 1)))",
        "'(2 4 6)",
    );
    assert_equal("(stream->list (stream-zip strm123))", "'((1) (2) (3))");
    assert_equal(
        "(stream->list (stream-zip strm123 strm123))",
        "'((1 1) (2 2) (3 3))",
    );
}

#[test]
fn range_ref_reverse() {
    assert_equal("(stream->list (stream-range 0 5))", "'(0 1 2 3 4)");
    assert_equal("(stream->list (stream-range 5 0))", "'(5 4 3 2 1)");
    assert_equal("(stream->list (stream-range 0 5 2))", "'(0 2 4)");
    assert_equal("(stream-ref strm123 2)", "3");
    assert_equal("(stream->list (stream-reverse strm123))", "'(3 2 1)");
}

#[test]
fn stream_let_and_define_stream() {
    assert_equal(
        "(stream->list
           (stream-let loop ((strm strm123))
             (if (stream-null? strm)
                 stream-null
                 (stream-cons (* 2 (stream-car strm)) (loop (stream-cdr strm))))))",
        "'(2 4 6)",
    );
    assert_equal(
        "(stream->list
           (let ()
             (define-stream (double strm)
               (if (stream-null? strm)
                   stream-null
                   (stream-cons (* 2 (stream-car strm)) (double (stream-cdr strm)))))
             (double strm123)))",
        "'(2 4 6)",
    );
}

#[test]
fn stream_match_patterns() {
    assert_equal("(stream-match stream-null (() 'ok))", "'ok");
    assert_equal("(stream-match strm123 (() 'no) (else 'ok))", "'ok");
    assert_equal("(stream-match (stream 1) (() 'no) ((a) a))", "1");
    assert_equal("(stream-match (stream 1) (() 'no) ((_) 'ok))", "'ok");
    assert_equal("(stream-match strm123 ((a b c) (list a b c)))", "'(1 2 3)");
    assert_equal("(stream-match strm123 ((a . _) a))", "1");
    assert_equal("(stream-match strm123 (s (stream->list s)))", "'(1 2 3)");
    // a fender that fails falls through to the next clause
    assert_equal(
        "(stream-match strm123 ((a . _) (= a 2) 'yes) (_ 'no))",
        "'no",
    );
    assert_equal(
        "(stream-match (stream 1 1 2) ((a b c) (= a b) 'yes) (_ 'no))",
        "'yes",
    );
}

#[test]
fn stream_of_comprehension() {
    assert_equal(
        "(stream->list
           (stream-of (+ y 6)
             (x in (stream-range 1 6))
             (odd? x)
             (y is (* x x))))",
        "'(7 15 31)",
    );
    assert_equal(
        "(stream->list
           (stream-of (* x x)
             (x in (stream-range 1 5))
             (odd? x)))",
        "'(1 9)",
    );
}

#[test]
fn stream_of_cartesian_comprehension() {
    // Two `in` generators yield the cartesian product. This used to
    // crash ("stream-cdr null"): nested stream-let loops reused the
    // identifiers loop/strm, which nscheme's macro hygiene mis-handled.
    // Fixed by scope-based hygiene (nscheme-d6o). Guard for
    // nscheme-lul.12.1.
    assert_equal(
        "(stream->list
           (stream-of (* x y)
             (x in (stream-range 1 4))
             (y in (stream-range 1 5))))",
        "'(1 2 3 4 2 4 6 8 3 6 9 12)",
    );
}

#[test]
fn unfold_and_unfolds() {
    assert_equal(
        "(stream->list (stream-unfold (lambda (x) (* x x)) (lambda (x) (< x 10)) (lambda (x) (+ x 1)) 0))",
        "'(0 1 4 9 16 25 36 49 64 81)",
    );
    assert_equal(
        "(stream->list
           (stream-unfolds
             (lambda (x)
               (let ((n (car x)) (s (cdr x)))
                 (if (zero? n)
                     (values 'dummy '())
                     (values (cons (- n 1) (stream-cdr s)) (list (stream-car s))))))
             (cons 5 (stream-from 0))))",
        "'(0 1 2 3 4)",
    );
}

#[test]
fn error_cases_are_raised() {
    // stream-car on a null stream raises.
    let err = run("(stream-car stream-null)").unwrap_err();
    assert!(matches!(err, EvalError::Raised(_) | EvalError::Runtime(_)));
    // stream-ref past the end raises.
    let err = run("(stream-ref strm123 5)").unwrap_err();
    assert!(matches!(err, EvalError::Raised(_) | EvalError::Runtime(_)));
}
