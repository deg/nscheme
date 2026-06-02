//! (scheme list-queue) / (srfi 117) tests — bead nscheme-lul.8.
//!
//! Cases are mined and translated from the SRFI 117 reference test
//! suite (list-queues-test.scm, Alex Shinn, BSD-3-Clause). Rather than
//! port the test harness, each scenario is run through `eval_source`
//! and asserted, matching this repo's test convention: `test-assert`
//! becomes `assert_true`, `(test EXPECTED EXPR)` becomes an equality
//! check on `run`, and `test-error` becomes an `unwrap_err` check.

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

const PRELUDE: &str = r"
(import (scheme base) (scheme list-queue))
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

/// Assert that a Scheme expression evaluates `equal?` to the expected one.
fn assert_equal(expr: &str, expected: &str) {
    let got = run(expr).unwrap_or_else(|e| panic!("error evaluating `{expr}`: {e:?}"));
    let want = run(expected).unwrap_or_else(|e| panic!("error evaluating `{expected}`: {e:?}"));
    assert!(
        equal(&got, &want),
        "expected `{expected}` from `{expr}`, got {got}"
    );
}

#[test]
fn make_and_list() {
    assert_equal("(list-queue-list (make-list-queue '(1 1 1)))", "'(1 1 1)");
    assert_equal("(list-queue-list (list-queue 1 2 3))", "'(1 2 3)");
}

#[test]
fn make_with_explicit_last_and_back() {
    // (make-list-queue x1 (cddr x1)) ; back should be 3
    assert_equal(
        "(let* ((x1 (list 1 2 3)) (x2 (make-list-queue x1 (cddr x1)))) (list-queue-back x2))",
        "3",
    );
}

#[test]
fn predicate_and_empty() {
    assert_true("(list-queue? (list-queue 4 5))");
    assert_true("(not (list-queue? '(4 5)))");
    assert_true("(list-queue-empty? (list-queue))");
    assert_true("(not (list-queue-empty? (list-queue 1)))");
}

#[test]
fn append_is_nondestructive() {
    assert_equal(
        "(let* ((x (list-queue 1 2 3))
                (y (list-queue 4 5))
                (z (list-queue-append x y)))
           (list-queue-list z))",
        "'(1 2 3 4 5)",
    );
}

#[test]
fn front_and_back() {
    assert_equal(
        "(let ((z (list-queue-append (list-queue 1 2 3) (list-queue 4 5))))
           (list (list-queue-front z) (list-queue-back z)))",
        "'(1 5)",
    );
}

#[test]
fn remove_front_and_back() {
    assert_equal(
        "(let ((y (list-queue 4 5)))
           (list-queue-remove-front! y)
           (list-queue-list y))",
        "'(5)",
    );
    assert_true(
        "(let ((y (list-queue 5)))
           (list-queue-remove-back! y)
           (list-queue-empty? y))",
    );
}

#[test]
fn remove_from_empty_is_error() {
    let err = run("(list-queue-remove-front! (list-queue))").unwrap_err();
    assert!(matches!(err, EvalError::Raised(_) | EvalError::Runtime(_)));
    let err = run("(list-queue-remove-back! (list-queue))").unwrap_err();
    assert!(matches!(err, EvalError::Raised(_) | EvalError::Runtime(_)));
}

#[test]
fn remove_all_and_add() {
    assert_equal(
        "(let ((z (list-queue 1 2 3 4 5)))
           (list-queue-remove-all! z)
           (list-queue-add-front! z 1)
           (list-queue-add-front! z 0)
           (list-queue-add-back! z 2)
           (list-queue-add-back! z 3)
           (list-queue-list z))",
        "'(0 1 2 3)",
    );
}

#[test]
fn copy_is_independent() {
    assert_equal(
        "(let* ((a (list-queue 1 2 3)) (b (list-queue-copy a)))
           (list-queue-add-front! b 0)
           (list (list-queue-list a) (length (list-queue-list b))))",
        "'((1 2 3) 4)",
    );
}

#[test]
fn concatenate() {
    assert_equal(
        "(let* ((a (list-queue 1 2 3))
                (b (list-queue-copy a))
                (c (begin (list-queue-add-front! b 0)
                          (list-queue-concatenate (list a b)))))
           (list-queue-list c))",
        "'(1 2 3 0 1 2 3)",
    );
}

#[test]
fn map_and_map_bang_and_for_each() {
    assert_equal(
        "(list-queue-list (list-queue-map (lambda (x) (* x 10)) (list-queue 1 2 3)))",
        "'(10 20 30)",
    );
    assert_equal(
        "(let ((r (list-queue 1 2 3)))
           (list-queue-map! (lambda (x) (+ x 1)) r)
           (list-queue-list r))",
        "'(2 3 4)",
    );
    assert_equal(
        "(let ((s (list-queue 10 20 30)) (sum 0))
           (list-queue-for-each (lambda (x) (set! sum (+ sum x))) s)
           sum)",
        "60",
    );
}

#[test]
fn set_list_and_first_last() {
    assert_equal(
        "(let ((n (list-queue 5 6)))
           (list-queue-set-list! n (list 1 2))
           (list-queue-list n))",
        "'(1 2)",
    );
    assert_true(
        "(let* ((d (list 1 2 3))
                (e (cddr d))
                (f (make-list-queue d e)))
           (call-with-values
             (lambda () (list-queue-first-last f))
             (lambda (dx ex) (and (eq? d dx) (eq? e ex)))))",
    );
}

#[test]
fn unfold_and_unfold_right() {
    assert_equal(
        "(list-queue-list
           (list-queue-unfold (lambda (x) (> x 3)) (lambda (x) (* x 2)) (lambda (x) (+ x 1)) 0))",
        "'(0 2 4 6)",
    );
    assert_equal(
        "(list-queue-list
           (list-queue-unfold-right (lambda (x) (> x 3)) (lambda (x) (* x 2)) (lambda (x) (+ x 1)) 0))",
        "'(6 4 2 0)",
    );
    assert_equal(
        "(list-queue-list
           (list-queue-unfold (lambda (x) (> x 3)) (lambda (x) (* x 2)) (lambda (x) (+ x 1)) 0
                              (list-queue 8)))",
        "'(0 2 4 6 8)",
    );
}
