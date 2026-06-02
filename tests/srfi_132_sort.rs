//! (scheme sort) / (srfi 132) tests — bead nscheme-lul.4.
//!
//! Cases are mined and translated from the SRFI 132 reference test
//! suite (srfi-132-test.sps, William D Clinger, MIT; embedding Olin
//! Shivers's sort harness). Rather than port the SRFI-64 harness (not an
//! R7RS-large deliverable), each `(or (equal? EXPR EXPECTED) (fail …))`
//! case is translated into an `(equal? EXPR 'EXPECTED)` assertion run
//! through `eval_source`, matching this repo's test convention.

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

const PRELUDE: &str = "(import (scheme base) (scheme sort))";

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
fn list_sort_orders_elements() {
    // list-sort:iota10
    assert_true(
        "(equal? (list-sort > (list 9 8 6 3 0 4 2 5 7 1))
                 '(9 8 7 6 5 4 3 2 1 0))",
    );
    // list-sort:empty-list / singleton
    assert_true("(equal? (list-sort > (list)) '())");
    assert_true("(equal? (list-sort > (list 987)) '(987))");
}

#[test]
fn list_stable_sort_is_stable() {
    // list-stable-sort:iota10-quotient2 — ties preserve input order.
    assert_true(
        "(equal? (list-stable-sort (lambda (x y) (> (quotient x 2) (quotient y 2)))
                                    (list 9 8 6 3 0 4 2 5 7 1))
                 '(9 8 6 7 4 5 3 2 0 1))",
    );
}

#[test]
fn list_sort_bang_orders_elements() {
    // list-sort!:iota10 and list-stable-sort!:iota10
    assert_true(
        "(equal? (list-sort! > (list 9 8 6 3 0 4 2 5 7 1))
                 '(9 8 7 6 5 4 3 2 1 0))",
    );
    assert_true(
        "(equal? (list-stable-sort! > (list 9 8 6 3 0 4 2 5 7 1))
                 '(9 8 7 6 5 4 3 2 1 0))",
    );
}

#[test]
fn vector_sort_orders_elements() {
    // vector-sort:iota10
    assert_true(
        "(equal? (vector-sort > (vector 9 8 6 3 0 4 2 5 7 1))
                 '#(9 8 7 6 5 4 3 2 1 0))",
    );
    // vector-sort:iota10:4:8 — sort the [4,8) sub-range only.
    assert_true(
        "(equal? (vector-sort > (vector 9 8 6 3 0 4 2 5 7 1) 4 8)
                 '#(5 4 2 0))",
    );
}

#[test]
fn vector_stable_sort_is_stable_and_ranged() {
    // vector-stable-sort:iota10-quotient2
    assert_true(
        "(equal? (vector-stable-sort (lambda (x y) (> (quotient x 2) (quotient y 2)))
                                      (vector 9 8 6 3 0 4 2 5 7 1))
                 '#(9 8 6 7 4 5 3 2 0 1))",
    );
    // vector-stable-sort:iota10:2:6
    assert_true(
        "(equal? (vector-stable-sort > (vector 9 8 6 3 0 4 2 5 7 1) 2 6)
                 '#(6 4 3 0))",
    );
}

#[test]
fn vector_sort_bang_mutates_in_place() {
    // vector-sort!:iota10
    assert_true(
        "(equal? (let ((v (vector 9 8 6 3 0 4 2 5 7 1)))
                   (vector-sort! > v)
                   v)
                 '#(9 8 7 6 5 4 3 2 1 0))",
    );
}

#[test]
fn sorted_predicates() {
    // list-sorted? and vector-sorted?, including a sub-range.
    assert_true("(list-sorted? > '(9 8 7))");
    assert_true("(not (list-sorted? > '(9 8 10 7)))");
    assert_true("(vector-sorted? > '#(9 8 7 6 5))");
    assert_true("(vector-sorted? > '#(9 8 7 6 5) 1 2)");
}

#[test]
fn list_merge_is_stable() {
    // list-merge:nonempty:nonempty and the empty edge cases.
    assert_true(
        "(equal? (list-merge > (list 9 7 5 3 1) (list 9 6 3 0))
                 '(9 9 7 6 5 3 3 1 0))",
    );
    assert_true("(equal? (list-merge > (list) (list 9 6 3 0)) '(9 6 3 0))");
    assert_true(
        "(equal? (list-merge! > (list 9 7 5 3 1) (list 9 6 3 0))
                 '(9 9 7 6 5 3 3 1 0))",
    );
}

#[test]
fn vector_merge_is_stable() {
    assert_true(
        "(equal? (vector-merge > (vector 9 7 5 3 1) (vector 9 6 3 0))
                 '#(9 9 7 6 5 3 3 1 0))",
    );
}

#[test]
fn list_delete_neighbor_dups() {
    // list-delete-neighbor-dups:nonempty — only adjacent dups collapse.
    assert_true(
        "(equal? (list-delete-neighbor-dups char=? (list #\\a #\\a #\\a #\\b #\\b #\\a))
                 '(#\\a #\\b #\\a))",
    );
    assert_true(
        "(equal? (list-delete-neighbor-dups! char=? (list #\\a #\\a #\\a #\\b #\\b #\\a))
                 '(#\\a #\\b #\\a))",
    );
}

#[test]
fn vector_delete_neighbor_dups() {
    assert_true(
        "(equal? (vector-delete-neighbor-dups char=? (vector #\\a #\\a #\\a #\\b #\\b #\\a))
                 '#(#\\a #\\b #\\a))",
    );
    // The destructive variant returns the new end index and packs left.
    assert_true(
        "(equal? (let ((v (vector #\\a #\\a #\\a #\\b #\\b #\\a)))
                   (list (vector-delete-neighbor-dups! char=? v) v))
                 '(3 #(#\\a #\\b #\\a #\\b #\\b #\\a)))",
    );
}

#[test]
fn vector_find_median() {
    // vector-find-median:empty returns knil; odd-length picks the middle.
    assert_true("(equal? (vector-find-median < (vector) \"knil\") \"knil\")");
    assert_true("(equal? (vector-find-median < (vector 17) \"knil\") 17)");
    assert_true("(equal? (vector-find-median < (vector 7 6 9 3 1 18 15 7 8) \"knil\") 7)");
    // Even length with a `list` mean procedure: the two central order
    // statistics of #(18 1 11 14 12 5 18 2) are 11 and 12.
    assert_true(
        "(equal? (vector-find-median < (vector 18 1 11 14 12 5 18 2) \"knil\" list)
                 (list 11 12))",
    );
}

#[test]
fn vector_select_and_separate() {
    // vector-select!:ten:0 / :2 / :9 — kth smallest of an unsorted vector.
    assert_true(
        "(equal? (let ((v (vector 8 22 19 19 13 9 21 13 3 23))) (vector-select! < v 0)) 3)",
    );
    assert_true(
        "(equal? (let ((v (vector 8 22 19 19 13 9 21 13 3 23))) (vector-select! < v 2)) 9)",
    );
    assert_true(
        "(equal? (let ((v (vector 8 22 19 19 13 9 21 13 3 23))) (vector-select! < v 9)) 23)",
    );
    // vector-separate! partitions so the k smallest occupy the prefix; we
    // check the prefix is exactly the set of the k smallest (sorted).
    let v = run("(let ((v (vector 8 22 19 19 13 9 21 13 3 23)))
           (vector-separate! < v 3)
           (list-sort < (list (vector-ref v 0) (vector-ref v 1) (vector-ref v 2))))")
    .expect("vector-separate!");
    let expected = run("(list 3 8 9)").expect("expected list");
    assert!(
        equal(&v, &expected),
        "vector-separate! prefix mismatch: got {v}"
    );
}
