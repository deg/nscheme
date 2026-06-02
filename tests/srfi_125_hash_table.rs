//! (scheme hash-table) / (srfi 125) tests — bead nscheme-lul.5.
//!
//! Cases are mined and translated from the SRFI 125 reference test
//! suite (tables-test.sps, William D Clinger, MIT). Rather than port the
//! bespoke `test`/`fail` harness, each case is run through `eval_source`
//! and asserted with `assert_true` (a combined predicate) or an equality
//! check on `run(...)`, matching this repo's test convention.
//!
//! NOTE: nscheme lacks SRFI 126, on which the upstream implementation is
//! built; the vendored library substitutes an alist-backed `hashtable-*`
//! shim (see lib/srfi/125.sld). These tests exercise the SRFI 125 API
//! exported by the shim-backed library.

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

/// A small fixture of hash tables built from SRFI 128 comparators,
/// reused as a prelude across the cases below.
// `(scheme comparator)` and `(scheme hash-table)` both export
// `string-hash` / `string-ci-hash` (the latter as SRFI 125's deprecated
// renames). The upstream tables-test.sps resolves the same collision by
// renaming; here we simply drop the deprecated names we never call.
const PRELUDE: &str = r"
(import (scheme base)
        (scheme comparator)
        (except (scheme hash-table)
                string-hash string-ci-hash hash hash-by-identity))
(define number-comparator (make-comparator real? = < number-hash))
(define string-comparator
  (make-comparator string? string=? string<? string-hash))
(define default-comparator (make-default-comparator))

;; A fixnum-keyed table mapping i*i -> i for i in 0..4.
(define (fresh-fixnum)
  (let ((ht (make-hash-table number-comparator)))
    (hash-table-set! ht 0 0 1 1 4 2 9 3 16 4)
    ht))
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

/// Assert that a Scheme expression evaluates to the given expected value.
fn assert_eval(expr: &str, expected: &str) {
    let got = run(expr).unwrap_or_else(|e| panic!("error evaluating `{expr}`: {e:?}"));
    let want = run(expected).unwrap_or_else(|e| panic!("error evaluating `{expected}`: {e:?}"));
    assert!(equal(&got, &want), "expected `{expr}` => {want}, got {got}");
}

#[test]
fn predicates() {
    assert_true(
        "(and (hash-table? (make-hash-table number-comparator))
              (not (hash-table? '#()))
              (not (hash-table? number-comparator))
              (hash-table-empty? (make-hash-table number-comparator))
              (not (hash-table-empty? (fresh-fixnum))))",
    );
}

#[test]
fn contains_and_ref_default() {
    assert_true(
        "(let ((ht (fresh-fixnum)))
           (and (hash-table-contains? ht 4)
                (not (hash-table-contains? ht 5))
                (= 2 (hash-table-ref/default ht 4 'nope))
                (eq? 'nope (hash-table-ref/default ht 7 'nope))))",
    );
}

#[test]
fn ref_with_failure_and_success() {
    assert_true(
        "(let ((ht (fresh-fixnum)))
           (and (= 3 (hash-table-ref ht 9))
                (= 30 (hash-table-ref ht 9 (lambda () 'fail) (lambda (v) (* v 10))))
                (eq? 'missing (hash-table-ref ht 99 (lambda () 'missing)))))",
    );
}

#[test]
fn set_and_size() {
    assert_eval(
        "(let ((ht (make-hash-table number-comparator)))
           (hash-table-set! ht 121 11 144 12 169 13)
           (hash-table-size ht))",
        "3",
    );
}

#[test]
fn delete_returns_count() {
    // hash-table-delete! returns the number of keys actually removed.
    assert_eval(
        "(let ((ht (fresh-fixnum)))
           (hash-table-delete! ht 0 1 100 4))",
        "3",
    );
}

#[test]
fn intern_and_update() {
    assert_true(
        "(let ((ht (fresh-fixnum)))
           (and (= 2 (hash-table-intern! ht 4 (lambda () 999)))
                (= 7 (hash-table-intern! ht 49 (lambda () 7)))
                (= 7 (hash-table-ref/default ht 49 'nope))
                (begin (hash-table-update!/default ht 9 (lambda (v) (+ v 100)) 0)
                       (= 103 (hash-table-ref/default ht 9 'nope)))
                (begin (hash-table-update!/default ht 81 (lambda (v) (+ v 1)) 5)
                       (= 6 (hash-table-ref/default ht 81 'nope)))))",
    );
}

#[test]
fn keys_values_entries() {
    assert_eval(
        "(let ((ht (make-hash-table number-comparator)))
           (hash-table-set! ht 1 10 2 20 3 30)
           (apply + (hash-table-keys ht)))",
        "6",
    );
    assert_eval(
        "(let ((ht (make-hash-table number-comparator)))
           (hash-table-set! ht 1 10 2 20 3 30)
           (apply + (hash-table-values ht)))",
        "60",
    );
}

#[test]
fn count_and_find() {
    assert_eval(
        "(let ((ht (fresh-fixnum)))
           (hash-table-count (lambda (k v) (even? v)) ht))",
        "3",
    );
    assert_true(
        "(let ((ht (fresh-fixnum)))
           (eq? 'none (hash-table-find (lambda (k v) (and (> v 100) #t)) ht (lambda () 'none))))",
    );
}

#[test]
fn fold_and_map_to_list() {
    assert_eval(
        "(let ((ht (make-hash-table number-comparator)))
           (hash-table-set! ht 1 10 2 20 3 30)
           (hash-table-fold (lambda (k v acc) (+ v acc)) 0 ht))",
        "60",
    );
    assert_eval(
        "(let ((ht (make-hash-table number-comparator)))
           (hash-table-set! ht 2 5)
           (hash-table-map->list (lambda (k v) (+ k v)) ht))",
        "'(7)",
    );
}

#[test]
fn map_and_for_each() {
    assert_eval(
        "(let* ((ht (fresh-fixnum))
                (sq (hash-table-map (lambda (v) (* v v)) number-comparator ht)))
           (hash-table-ref/default sq 16 'nope))",
        "16",
    );
    assert_eval(
        "(let ((ht (fresh-fixnum)) (sum 0))
           (hash-table-for-each (lambda (k v) (set! sum (+ sum v))) ht)
           sum)",
        "10",
    );
}

#[test]
fn prune_and_clear() {
    assert_eval(
        "(let ((ht (fresh-fixnum)))
           (hash-table-prune! (lambda (k v) (odd? v)) ht)
           (hash-table-size ht))",
        "3",
    );
    assert_true(
        "(let ((ht (fresh-fixnum)))
           (hash-table-clear! ht)
           (hash-table-empty? ht))",
    );
}

#[test]
fn copy_mutability_and_alist() {
    // A plain copy is immutable; an explicit (... #t) copy is mutable.
    assert_true(
        "(let ((ht (fresh-fixnum)))
           (and (not (hash-table-mutable? (hash-table-copy ht)))
                (not (hash-table-mutable? (hash-table-copy ht #f)))
                (hash-table-mutable? (hash-table-copy ht #t))))",
    );
    assert_eval(
        "(let ((ht (make-hash-table number-comparator)))
           (hash-table-set! ht 3 30)
           (hash-table->alist ht))",
        "'((3 . 30))",
    );
}

#[test]
fn set_operations() {
    // union! keeps existing values; xor! removes shared keys.
    assert_true(
        "(let ((a (make-hash-table number-comparator))
               (b (make-hash-table number-comparator)))
           (hash-table-set! a 1 'a1 2 'a2)
           (hash-table-set! b 2 'b2 3 'b3)
           (hash-table-union! a b)
           (and (eq? 'a2 (hash-table-ref/default a 2 'nope))
                (eq? 'b3 (hash-table-ref/default a 3 'nope))
                (= 3 (hash-table-size a))))",
    );
    assert_true(
        "(let ((a (make-hash-table number-comparator))
               (b (make-hash-table number-comparator)))
           (hash-table-set! a 1 'a1 2 'a2)
           (hash-table-set! b 2 'b2 3 'b3)
           (hash-table-xor! a b)
           (and (not (hash-table-contains? a 2))
                (hash-table-contains? a 1)
                (hash-table-contains? a 3)))",
    );
}

#[test]
fn equality_and_unfold() {
    assert_true(
        "(let ((a (fresh-fixnum)) (b (fresh-fixnum)))
           (and (hash-table=? number-comparator a b)
                (begin (hash-table-set! b 100 10)
                       (not (hash-table=? number-comparator a b)))))",
    );
    // hash-table-unfold builds a table by iterating a seed.
    assert_eval(
        "(let ((ht (hash-table-unfold
                     (lambda (i) (> i 3))
                     (lambda (i) (values i (* i i)))
                     (lambda (i) (+ i 1))
                     0
                     number-comparator)))
           (hash-table-ref/default ht 3 'nope))",
        "9",
    );
}
