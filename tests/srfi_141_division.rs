//! (scheme division) / (srfi 141) tests — bead nscheme-oeg.4.
//!
//! SRFI 141 (Taylor Campbell, William D Clinger) ships no SRFI-64 test
//! suite in its repository, so these cases are derived directly from the
//! SRFI specification: the division identity `n = d*q + r` together with
//! each operator family's documented range/sign constraint on the
//! remainder. Each group is run through `eval_source` and its combined
//! `(and …)` asserted, matching this repo's test convention (see
//! `srfi_128_comparator.rs`).

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

const PRELUDE: &str = r"
(import (scheme base) (scheme division))
;; Collect both return values of a *\/ operator into a 2-element list so
;; the assertions below can compare them with equal?.
(define (qr proc n d)
  (call-with-values (lambda () (proc n d)) (lambda (q r) (list q r))))
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

#[test]
fn floor_division() {
    assert_true(
        "(and (equal? (qr floor/ 7 2) '(3 1))
              (equal? (qr floor/ -7 2) '(-4 1))
              (equal? (qr floor/ 7 -2) '(-4 -1))
              (equal? (qr floor/ -7 -2) '(3 -1))
              (= (floor-quotient -7 2) -4)
              (= (floor-remainder -7 2) 1))",
    );
}

#[test]
fn floor_remainder_has_sign_of_divisor() {
    // For floor/, the remainder has the same sign as the divisor.
    assert_true(
        "(and (= (+ (* 2 (floor-quotient -7 2)) (floor-remainder -7 2)) -7)
              (>= (floor-remainder -7 2) 0)
              (<= (floor-remainder 7 -2) 0))",
    );
}

#[test]
fn ceiling_division() {
    assert_true(
        "(and (equal? (qr ceiling/ 7 2) '(4 -1))
              (equal? (qr ceiling/ -7 2) '(-3 -1))
              (equal? (qr ceiling/ -7 -2) '(4 1))
              (= (ceiling-quotient 7 2) 4)
              (= (ceiling-remainder 7 2) -1))",
    );
}

#[test]
fn truncate_division() {
    // Truncating division is R5RS quotient/remainder; remainder has the
    // sign of the numerator.
    assert_true(
        "(and (equal? (qr truncate/ 7 2) '(3 1))
              (equal? (qr truncate/ -7 2) '(-3 -1))
              (equal? (qr truncate/ 7 -2) '(-3 1))
              (equal? (qr truncate/ -7 -2) '(3 -1))
              (= (truncate-quotient -7 2) -3)
              (= (truncate-remainder -7 2) -1))",
    );
}

#[test]
fn round_ties_to_even() {
    // 7/2 = 3.5 rounds to even 4; 5/2 = 2.5 rounds to even 2.
    assert_true(
        "(and (equal? (qr round/ 7 2) '(4 -1))
              (equal? (qr round/ 5 2) '(2 1))
              (= (round-quotient 7 2) 4)
              (= (round-remainder 5 2) 1)
              (= (+ (* 2 (round-quotient 7 2)) (round-remainder 7 2)) 7))",
    );
}

#[test]
fn euclidean_remainder_is_nonnegative() {
    // Euclidean division guarantees 0 <= r < |d| for every sign combo.
    assert_true(
        "(and (equal? (qr euclidean/ 7 2) '(3 1))
              (equal? (qr euclidean/ -7 2) '(-4 1))
              (equal? (qr euclidean/ 7 -2) '(-3 1))
              (>= (euclidean-remainder -7 2) 0)
              (< (euclidean-remainder -7 2) 2)
              (= (+ (* 2 (euclidean-quotient -7 2)) (euclidean-remainder -7 2)) -7))",
    );
}

#[test]
fn balanced_remainder_in_half_open_interval() {
    // Balanced division keeps r in [-|d|/2, |d|/2).
    assert_true(
        "(and (equal? (qr balanced/ 7 2) '(4 -1))
              (equal? (qr balanced/ -7 2) '(-3 -1))
              (equal? (qr balanced/ 5 2) '(3 -1))
              (= (+ (* 2 (balanced-quotient 7 2)) (balanced-remainder 7 2)) 7)
              (>= (balanced-remainder 7 2) -1)
              (< (balanced-remainder 7 2) 1))",
    );
}

#[test]
fn quotient_and_remainder_accessors_match_full_divide() {
    // The *-quotient / *-remainder accessors agree with the two-value /.
    assert_true(
        "(and (= (floor-quotient 17 5) (car (qr floor/ 17 5)))
              (= (floor-remainder 17 5) (cadr (qr floor/ 17 5)))
              (= (ceiling-quotient 17 5) (car (qr ceiling/ 17 5)))
              (= (truncate-remainder -17 5) (cadr (qr truncate/ -17 5)))
              (= (round-quotient 17 5) (car (qr round/ 17 5))))",
    );
}
