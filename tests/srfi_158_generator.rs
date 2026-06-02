//! (scheme generator) / (srfi 158) tests — bead nscheme-lul.10.
//!
//! Cases are mined and translated from the SRFI 158 reference test
//! suite (chicken-test.scm, Shiro Kawai / John Cowan / Thomas Gilray,
//! MIT). Rather than port the SRFI-64 harness (not an R7RS-large
//! deliverable), each `(test EXPECTED EXPR)` is checked by comparing
//! `run("EXPR")` against `run("'EXPECTED")` via `equal`, and
//! `(test-assert EXPR)` becomes `assert_true("EXPR")`, matching this
//! repo's test convention.

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
(import (scheme base) (scheme generator))
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

/// Assert that `expr` evaluates `equal?` to `expected` (a quoted datum).
fn assert_equal(expr: &str, expected: &str) {
    let got = run(expr).unwrap_or_else(|e| panic!("error evaluating `{expr}`: {e:?}"));
    let want = run(&format!("(quote {expected})"))
        .unwrap_or_else(|e| panic!("error evaluating expected `{expected}`: {e:?}"));
    assert!(equal(&got, &want), "expected `{expr}` => {want}, got {got}");
}

#[test]
fn constructors() {
    assert_equal("(generator->list (generator))", "()");
    assert_equal("(generator->list (generator 1 2 3))", "(1 2 3)");
    assert_equal(
        "(generator->list (circular-generator 1 2 3) 5)",
        "(1 2 3 1 2)",
    );
    assert_equal("(generator->list (make-iota-generator 3 8))", "(8 9 10)");
    assert_equal("(generator->list (make-iota-generator 3 8 2))", "(8 10 12)");
    assert_equal("(generator->list (make-range-generator 3) 4)", "(3 4 5 6)");
    assert_equal(
        "(generator->list (make-range-generator 3 8))",
        "(3 4 5 6 7)",
    );
    assert_equal("(generator->list (make-range-generator 3 8 2))", "(3 5 7)");
    assert_equal(
        "(generator->list (list->generator '(1 2 3 4 5)))",
        "(1 2 3 4 5)",
    );
    assert_equal(
        "(generator->list (vector->generator '#(1 2 3 4 5)))",
        "(1 2 3 4 5)",
    );
    assert_equal(
        "(generator->list (reverse-vector->generator '#(1 2 3 4 5)))",
        "(5 4 3 2 1)",
    );
    assert_equal(
        "(generator->list (string->generator \"abcde\"))",
        "(#\\a #\\b #\\c #\\d #\\e)",
    );
}

#[test]
fn coroutine_and_unfold() {
    assert_equal(
        "(generator->list
           (make-coroutine-generator
             (lambda (yield)
               (let loop ((i 0))
                 (when (< i 3) (yield i) (loop (+ i 1)))))))",
        "(0 1 2)",
    );
    assert_equal(
        "(generator->list
           (make-unfold-generator
             (lambda (s) (> s 5))
             (lambda (s) (* s 2))
             (lambda (s) (+ s 1))
             0))",
        "(0 2 4 6 8 10)",
    );
}

#[test]
fn operators_basic() {
    assert_equal(
        "(generator->list (gcons* 'a 'b (make-range-generator 0 2)))",
        "(a b 0 1)",
    );
    assert_equal(
        "(generator->list (gappend (make-range-generator 0 3) (make-range-generator 0 2)))",
        "(0 1 2 0 1)",
    );
    assert_equal("(generator->list (gappend))", "()");
    assert_equal(
        "(generator->list (gfilter odd? (make-range-generator 1 11)))",
        "(1 3 5 7 9)",
    );
    assert_equal(
        "(generator->list (gremove odd? (make-range-generator 1 11)))",
        "(2 4 6 8 10)",
    );
}

#[test]
fn take_drop_while() {
    assert_equal(
        "(generator->list (gtake (make-range-generator 1 3) 3))",
        "(1 2)",
    );
    assert_equal(
        "(generator->list (gtake (make-range-generator 1 3) 3 0))",
        "(1 2 0)",
    );
    assert_equal(
        "(generator->list (gdrop (make-range-generator 1 5) 2))",
        "(3 4)",
    );
    assert_equal(
        "(generator->list (gtake-while (lambda (x) (< x 3)) (make-range-generator 1 5)))",
        "(1 2)",
    );
    assert_equal(
        "(generator->list (gdrop-while (lambda (x) (< x 3)) (make-range-generator 1 5)))",
        "(3 4)",
    );
}

#[test]
fn combine_map_group_merge() {
    assert_equal(
        "(generator->list
           (gcombine (lambda args (let ((s (apply + args))) (values s s)))
                     10 (generator 1 2 3) (generator 4 5 6 7)))",
        "(15 22 31)",
    );
    assert_equal(
        "(generator->list (gmap - (generator 1 2 3 4 5)))",
        "(-1 -2 -3 -4 -5)",
    );
    assert_equal(
        "(generator->list (gmap + (generator 1 2 3 4 5) (generator 6 7 8 9)))",
        "(7 9 11 13)",
    );
    assert_equal(
        "(generator->list (ggroup (generator 1 2 3 4 5 6 7 8) 3))",
        "((1 2 3) (4 5 6) (7 8))",
    );
    assert_equal(
        "(generator->list (ggroup (generator 1 2 3 4 5 6 7 8) 3 0))",
        "((1 2 3) (4 5 6) (7 8 0))",
    );
    assert_equal(
        "(generator->list (gmerge < (generator 1 2 3) (generator 4 5 6)))",
        "(1 2 3 4 5 6)",
    );
    assert_equal(
        "(generator->list (gflatten (generator '(1 2 3) '(a b c))))",
        "(1 2 3 a b c)",
    );
    assert_equal(
        "(generator->list (gindex (list->generator '(a b c d e f)) (list->generator '(0 2 4))))",
        "(a c e)",
    );
    assert_equal(
        "(generator->list (gselect (list->generator '(a b c d e f)) (list->generator '(#t #f #f #t #t #f))))",
        "(a d e)",
    );
    assert_equal(
        "(generator->list (gdelete-neighbor-dups (generator 1 1 2 3 3 3) =))",
        "(1 2 3)",
    );
    assert_equal(
        "(generator->list
           (gstate-filter (lambda (item state) (values (even? state) (+ 1 state)))
                          0 (generator 'a 'b 'c 'd 'e 'f 'g 'h 'i 'j)))",
        "(a c e g i)",
    );
}

#[test]
fn consumers() {
    assert_equal("(generator->list (generator 1 2 3 4 5) 3)", "(1 2 3)");
    assert_equal(
        "(generator->reverse-list (generator 1 2 3 4 5))",
        "(5 4 3 2 1)",
    );
    assert_equal("(generator->vector (generator 1 2 3 4 5))", "#(1 2 3 4 5)");
    assert_equal("(generator->vector (generator 1 2 3 4 5) 3)", "#(1 2 3)");
    assert_equal("(generator->string (generator #\\a #\\b #\\c))", "\"abc\"");
    assert_true("(= 3 (generator-find (lambda (x) (> x 2)) (make-range-generator 1 5)))");
    assert_true("(not (generator-find (lambda (x) (> x 10)) (make-range-generator 1 5)))");
    assert_true("(= 2 (generator-count odd? (make-range-generator 1 5)))");
    assert_true("(generator-any odd? (make-range-generator 2 5))");
    assert_true("(not (generator-every odd? (make-range-generator 2 5)))");
    assert_equal(
        "(generator-map->list (lambda values (apply + values))
                              (generator 1 4) (generator 2 5) (generator 3 6))",
        "(6 15)",
    );
    assert_true(
        "(let ((n 0))
           (generator-for-each (lambda values (set! n (apply + values)))
             (generator 1) (generator 2) (generator 3))
           (= n 6))",
    );
}

#[test]
fn accumulators() {
    assert_true(
        "(let ((a (make-accumulator * 1 -)))
           (a 1) (a 2) (a 4) (= (a (eof-object)) -8))",
    );
    assert_true(
        "(let ((a (count-accumulator)))
           (a 1) (a 2) (a 4) (= (a (eof-object)) 3))",
    );
    assert_equal(
        "(let ((a (list-accumulator)))
           (a 1) (a 2) (a 4) (a (eof-object)))",
        "(1 2 4)",
    );
    assert_equal(
        "(let ((a (reverse-list-accumulator)))
           (a 1) (a 2) (a 4) (a (eof-object)))",
        "(4 2 1)",
    );
    assert_equal(
        "(let ((a (vector-accumulator)))
           (a 1) (a 2) (a 4) (a (eof-object)))",
        "#(1 2 4)",
    );
    assert_equal(
        "(let ((a (reverse-vector-accumulator)))
           (a 1) (a 2) (a 4) (a (eof-object)))",
        "#(4 2 1)",
    );
    assert_equal(
        "(let ((a (string-accumulator)))
           (a #\\a) (a #\\b) (a #\\c) (a (eof-object)))",
        "\"abc\"",
    );
    assert_true(
        "(let ((a (sum-accumulator)))
           (a 1) (a 2) (a 4) (= (a (eof-object)) 7))",
    );
    assert_true(
        "(let ((a (product-accumulator)))
           (a 1) (a 2) (a 4) (= (a (eof-object)) 8))",
    );
}
