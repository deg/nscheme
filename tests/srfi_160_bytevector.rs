//! (scheme bytevector) / (srfi 160 u8) tests — bead nscheme-oeg.5.
//!
//! Cases are mined and translated from the SRFI 160 reference test
//! suite (shared-tests.scm, John Cowan / Shiro Kawai, MIT). The shared
//! suite is written generically with an s16 prefix; here each case is
//! re-expressed for the u8 instantiation (all sample values stay in the
//! 0..255 u8 range). The SRFI-64-style harness is not an R7RS-large
//! deliverable, so each group is run through `eval_source` and its
//! combined `(and …)` asserted, matching this repo's test convention.

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

/// Sample vectors reused across the SRFI 160 tests (u8 instantiation).
const PRELUDE: &str = r"
(import (scheme base) (scheme bytevector) (scheme comparator))
(define v5 (u8vector 1 2 3 4 5))
(define (count-up i x) (values x (+ x 1)))
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
fn constructors_and_conversion() {
    // make-u8vector / u8vector / list round-trips.
    assert_true("(equal? (u8vector->list (make-u8vector 5 3)) '(3 3 3 3 3))");
    assert_true("(equal? (u8vector->list (u8vector 0 1 2 3 4)) '(0 1 2 3 4))");
    assert_true("(equal? (u8vector->list (list->u8vector '(7 8 9))) '(7 8 9))");
    // unfold up: '(10 11 12 13 14)
    assert_true("(equal? (u8vector->list (u8vector-unfold count-up 5 10)) '(10 11 12 13 14))");
}

#[test]
fn copy_append_concatenate() {
    assert_true("(equal? (u8vector->list (u8vector-copy v5)) '(1 2 3 4 5))");
    assert_true("(not (eqv? v5 (u8vector-copy v5)))");
    assert_true("(equal? (u8vector->list (u8vector-copy v5 1 3)) '(2 3))");
    assert_true("(equal? (u8vector->list (u8vector-reverse-copy v5)) '(5 4 3 2 1))");
    assert_true("(equal? (u8vector->list (u8vector-append v5 v5)) '(1 2 3 4 5 1 2 3 4 5))");
    assert_true(
        "(equal? (u8vector->list (u8vector-concatenate (list v5 v5)))
                 '(1 2 3 4 5 1 2 3 4 5))",
    );
    assert_true("(equal? (u8vector->list (u8vector-append-subvectors v5 1 3 v5 1 3)) '(2 3 2 3))");
}

#[test]
fn predicates() {
    assert_true(
        "(and (u8? 5)
              (not (u8? 256))
              (u8vector? v5)
              (not (u8vector? #t))
              (u8vector-empty? (u8vector))
              (not (u8vector-empty? v5))
              (u8vector= (u8vector 1 2 3) (u8vector 1 2 3))
              (u8vector= (u8vector 1 2 3) (u8vector 1 2 3) (u8vector 1 2 3))
              (not (u8vector= (u8vector 1 2 3) (u8vector 3 2 1)))
              (not (u8vector= (u8vector 1 2 3) (u8vector 1 2))))",
    );
}

#[test]
fn selectors() {
    assert_true("(= (u8vector-ref (u8vector 1 2 3) 0) 1)");
    assert_true("(= (u8vector-length (u8vector 1 2 3)) 3)");
}

#[test]
fn iteration_take_drop_segment() {
    assert_true("(equal? (u8vector->list (u8vector-take v5 2)) '(1 2))");
    assert_true("(equal? (u8vector->list (u8vector-take-right v5 2)) '(4 5))");
    assert_true("(equal? (u8vector->list (u8vector-drop v5 2)) '(3 4 5))");
    assert_true("(equal? (u8vector->list (u8vector-drop-right v5 2)) '(1 2 3))");
    assert_true(
        "(equal? (map u8vector->list (u8vector-segment v5 3))
                 '((1 2 3) (4 5)))",
    );
}

#[test]
fn fold_map_count_cumulate() {
    assert_true("(= (u8vector-fold + 0 (u8vector 1 2 3)) 6)");
    assert_true("(= (u8vector-fold-right + 0 (u8vector 1 2 3)) 6)");
    assert_true(
        "(equal? (u8vector-fold list 0 (u8vector 1 2 3) (u8vector 4 5 6))
                 '(((0 1 4) 2 5) 3 6))",
    );
    assert_true("(equal? (u8vector->list (u8vector-map (lambda (x) (* x 2)) v5)) '(2 4 6 8 10))");
    assert_true("(= (u8vector-count odd? v5) 3)");
    assert_true("(equal? (u8vector->list (u8vector-cumulate + 0 v5)) '(1 3 6 10 15))");
}

#[test]
fn mutators_map_bang_and_swap() {
    assert_true(
        "(let ((v (u8vector 1 2 3 4 5)))
           (u8vector-map! (lambda (x) (* x 2)) v)
           (equal? (u8vector->list v) '(2 4 6 8 10)))",
    );
    assert_true(
        "(let ((v (u8vector 1 2 3 4 5)))
           (u8vector-swap! v 0 4)
           (equal? (u8vector->list v) '(5 2 3 4 1)))",
    );
    assert_true(
        "(let ((v (u8vector 1 2 3 4 5)))
           (u8vector-reverse! v)
           (equal? (u8vector->list v) '(5 4 3 2 1)))",
    );
    assert_true(
        "(let ((v (u8vector 1 2 3 4 5)))
           (u8vector-fill! v 9 1 3)
           (equal? (u8vector->list v) '(1 9 9 4 5)))",
    );
}

#[test]
fn searching() {
    assert_true("(equal? (u8vector->list (u8vector-take-while odd? v5)) '(1))");
    assert_true("(equal? (u8vector->list (u8vector-drop-while odd? v5)) '(2 3 4 5))");
    assert_true("(= (u8vector-index even? v5) 1)");
    assert_true("(= (u8vector-index-right even? v5) 3)");
    assert_true("(= (u8vector-skip odd? v5) 1)");
    assert_true("(= (u8vector-any (lambda (x) (and (even? x) (* x 2))) v5) 4)");
    assert_true("(not (u8vector-any (lambda (x) (> x 100)) v5))");
    assert_true("(u8vector-every (lambda (x) (< x 100)) v5)");
    assert_true("(not (u8vector-every odd? v5))");
}

#[test]
fn filter_remove_partition() {
    assert_true("(equal? (u8vector->list (u8vector-filter odd? v5)) '(1 3 5))");
    assert_true("(equal? (u8vector->list (u8vector-remove odd? v5)) '(2 4))");
    assert_true(
        "(call-with-values
           (lambda () (u8vector-partition even? v5))
           (lambda (vec cnt)
             (and (= cnt 2)
                  (equal? (u8vector->list (u8vector-copy vec 0 2)) '(2 4)))))",
    );
}

#[test]
fn vector_conversions_and_reverse_list() {
    assert_true("(equal? (u8vector->vector v5) '#(1 2 3 4 5))");
    assert_true("(equal? (u8vector->list (vector->u8vector '#(6 7 8))) '(6 7 8))");
    assert_true("(equal? (reverse-u8vector->list v5) '(5 4 3 2 1))");
    assert_true("(equal? (u8vector->list (reverse-list->u8vector '(1 2 3))) '(3 2 1))");
}

#[test]
fn generator_and_comparator() {
    // make-u8vector-generator yields elements then eof.
    assert_true(
        "(let ((g (make-u8vector-generator (u8vector 10 20))))
           (let* ((a (g)) (b (g)) (c (g)))
             (and (= a 10) (= b 20) (eof-object? c))))",
    );
    // u8vector-comparator orders by length then lexicographically.
    assert_true(
        "(let ((cmp u8vector-comparator))
           (and (=? cmp (u8vector 1 2 3) (u8vector 1 2 3))
                (<? cmp (u8vector 1 2) (u8vector 1 2 3))
                (<? cmp (u8vector 1 2 3) (u8vector 1 3 4))
                (exact-integer?
                  ((comparator-hash-function cmp) (u8vector 1 2 3)))))",
    );
}
