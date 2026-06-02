//! (scheme comparator) / (srfi 128) tests — bead nscheme-lul.1.
//!
//! Cases are mined and translated from the SRFI 128 reference test
//! suite (comparators-test.scm, John Cowan, MIT). Rather than port the
//! SRFI-64 harness (not an R7RS-large deliverable; see the epic notes),
//! each group is run through `eval_source` and its combined `(and …)`
//! asserted, matching this repo's test convention.

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::library::set_search_path;
use nscheme::value::{Symbol, Value, equal};

/// The library lives in the repo's real `lib/` tree.
fn lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/lib", env!("CARGO_MANIFEST_DIR")))
}

/// Comparators defined by the SRFI 128 test suite, reused as a prelude.
const PRELUDE: &str = r"
(import (scheme base) (scheme comparator))
(define default-comparator (make-default-comparator))
(define real-comparator (make-comparator real? = < number-hash))
(define degenerate-comparator (make-comparator (lambda (x) #t) equal? #f #f))
(define boolean-comparator
  (make-comparator boolean? eq? (lambda (x y) (and (not x) y)) boolean-hash))
(define bool-pair-comparator (make-pair-comparator boolean-comparator boolean-comparator))
(define num-vector-comparator
  (make-vector-comparator real-comparator vector? vector-length vector-ref))
(define eq-comparator (make-eq-comparator))
(define eqv-comparator (make-eqv-comparator))
(define equal-comparator (make-equal-comparator))
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
fn predicates() {
    assert_true(
        "(and (comparator? real-comparator)
              (not (comparator? =))
              (comparator-ordered? real-comparator)
              (comparator-hashable? real-comparator)
              (not (comparator-ordered? degenerate-comparator))
              (not (comparator-hashable? degenerate-comparator)))",
    );
}

#[test]
fn boolean_and_pair_constructors() {
    assert_true(
        "(and (=? boolean-comparator #t #t)
              (not (=? boolean-comparator #t #f))
              (<? boolean-comparator #f #t)
              (not (<? boolean-comparator #t #t))
              (comparator-test-type bool-pair-comparator '(#t . #f))
              (not (comparator-test-type bool-pair-comparator 32))
              (not (comparator-test-type bool-pair-comparator '(32 . #f)))
              (=? bool-pair-comparator '(#t . #t) '(#t . #t))
              (not (=? bool-pair-comparator '(#t . #t) '(#f . #t)))
              (<? bool-pair-comparator '(#f . #t) '(#t . #t))
              (<? bool-pair-comparator '(#t . #f) '(#t . #t)))",
    );
}

#[test]
fn vector_constructor() {
    assert_true(
        "(and (comparator-test-type num-vector-comparator '#(1 2 3))
              (comparator-test-type num-vector-comparator '#())
              (not (comparator-test-type num-vector-comparator 1))
              (not (comparator-test-type num-vector-comparator '#(a 2 3)))
              (=? num-vector-comparator '#(1 2 3) '#(1 2 3))
              (not (=? num-vector-comparator '#(1 2 3) '#(1 2 6)))
              (<? num-vector-comparator '#(1 2) '#(1 2 3))
              (<? num-vector-comparator '#(1 2 3) '#(1 3 4))
              (<? num-vector-comparator '#(3 4) '#(1 2 3))
              (not (<? num-vector-comparator '#(1 2 3) '#(1 2 3))))",
    );
}

#[test]
fn eq_eqv_equal_constructors() {
    assert_true(
        "(let ((p (cons #t #f)) (p2 (cons #t #f)) (rp (cons #f #t)))
           (and (=? eq-comparator #t #t)
                (not (=? eq-comparator #f #t))
                (=? eqv-comparator p p)
                (not (=? eqv-comparator p p2))
                (=? equal-comparator p p2)
                (not (=? equal-comparator p rp))))",
    );
}

#[test]
fn hash_functions_are_nonnegative_exact_integers() {
    assert_true(
        "(and (exact-integer? (boolean-hash #f))
              (not (negative? (boolean-hash #t)))
              (exact-integer? (char-hash #\\a))
              (not (negative? (char-hash #\\b)))
              (= (char-ci-hash #\\a) (char-ci-hash #\\A))
              (exact-integer? (string-hash \"f\"))
              (= (string-ci-hash \"f\") (string-ci-hash \"F\"))
              (exact-integer? (symbol-hash 'f))
              (exact-integer? (number-hash 3))
              (not (negative? (number-hash -3)))
              (exact-integer? (number-hash 3.0)))",
    );
}

#[test]
fn default_comparator_orders_across_types() {
    assert_true(
        "(and (<? default-comparator '() '(a))
              (not (=? default-comparator '() '(a)))
              (=? default-comparator #t #t)
              (<? default-comparator #f #t)
              (=? default-comparator #\\a #\\a)
              (<? default-comparator #\\a #\\b)
              (comparator-test-type default-comparator (make-bytevector 10))
              (=? default-comparator '(#t . #t) '(#t . #t))
              (<? default-comparator '(#f . #t) '(#t . #t))
              (=? default-comparator '#(#t #t) '#(#t #t))
              (<? default-comparator '#(#f #t) '#(#t #t)))",
    );
}

#[test]
fn default_comparator_hash_matches_type_hashers() {
    assert_true(
        "(and (= (comparator-hash default-comparator #t) (boolean-hash #t))
              (= (comparator-hash default-comparator #\\t) (char-hash #\\t))
              (= (comparator-hash default-comparator \"t\") (string-hash \"t\"))
              (= (comparator-hash default-comparator 't) (symbol-hash 't))
              (= (comparator-hash default-comparator 10) (number-hash 10)))",
    );
}

#[test]
fn register_default_extends_the_default_comparator() {
    assert_true(
        "(begin
           (comparator-register-default!
             (make-comparator procedure? (lambda (a b) #t) (lambda (a b) #f) (lambda (obj) 200)))
           (and (=? default-comparator (lambda () #t) (lambda () #f))
                (not (<? default-comparator (lambda () #t) (lambda () #f)))
                (= 200 (comparator-hash default-comparator (lambda () #t)))))",
    );
}

#[test]
fn invokers_check_type() {
    assert_true("(comparator-check-type boolean-comparator #t)");
    // comparator-check-type raises on a type mismatch.
    let err = run("(comparator-check-type boolean-comparator 't)").unwrap_err();
    assert!(matches!(err, EvalError::Raised(_) | EvalError::Runtime(_)));
}

#[test]
fn variadic_comparison_predicates() {
    assert_true(
        "(and (=? real-comparator 2 2.0 2)
              (<? real-comparator 2 3.0 4)
              (>? real-comparator 4.0 3.0 2)
              (<=? real-comparator 2.0 2 3.0)
              (>=? real-comparator 3 3.0 2)
              (not (=? real-comparator 1 2 3))
              (not (<? real-comparator 3 1 2))
              (not (<=? real-comparator 4 3 3)))",
    );
}

#[test]
fn comparator_if_syntax() {
    let sym = |s: &str| Value::Symbol(Symbol::intern(s));
    assert!(equal(
        &run("(comparator-if<=> real-comparator 1 2 'less 'equal 'greater)").unwrap(),
        &sym("less")
    ));
    assert!(equal(
        &run("(comparator-if<=> real-comparator 1 1 'less 'equal 'greater)").unwrap(),
        &sym("equal")
    ));
    assert!(equal(
        &run("(comparator-if<=> \"2\" \"1\" 'less 'equal 'greater)").unwrap(),
        &sym("greater")
    ));
}

#[test]
fn hash_bound_and_salt() {
    assert_true(
        "(and (exact-integer? (hash-bound))
              (exact-integer? (hash-salt))
              (< (hash-salt) (hash-bound)))",
    );
}
