//! (scheme mapping hash) / (srfi 146 hash) tests — bead nscheme-oeg.7.
//!
//! Cases are mined and translated from the SRFI 146 reference test
//! suite (srfi/146/hash/test.sld, Marc Nieper-Wißkirchen, MIT). Rather
//! than port the SRFI-64 harness, each `test-assert` becomes an
//! `assert_true`, each `test-equal` an equality check on `run`, and
//! `test-error` an error check — matching this repo's convention (see
//! `tests/srfi_128_comparator.rs`).

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

/// Default comparator + a few sample hashmaps reused across tests.
const PRELUDE: &str = r"
(import (scheme base) (scheme mapping hash) (scheme comparator))
(define comparator (make-default-comparator))
(define hashmap0 (hashmap comparator))
(define hashmap1 (hashmap comparator 'a 1 'b 2 'c 3))
(define hashmap2 (hashmap comparator 'c 1 'd 2 'e 3))
(define hashmap3 (hashmap comparator 'd 1 'e 2 'f 3))
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

/// Assert that a Scheme expression evaluates to a given integer.
fn assert_int(expr: &str, expected: i64) {
    match run(expr) {
        Ok(Value::Int(n)) if n == expected => {}
        Ok(other) => panic!("expected {expected} from `{expr}`, got {other}"),
        Err(e) => panic!("error evaluating `{expr}`: {e:?}"),
    }
}

#[test]
fn predicates() {
    assert_true(
        "(and (hashmap? (hashmap comparator))
              (not (hashmap? (list 1 2 3)))
              (hashmap-empty? hashmap0)
              (not (hashmap-empty? hashmap1))
              (hashmap-contains? hashmap1 'b)
              (not (hashmap-contains? hashmap1 'z))
              (hashmap-disjoint? hashmap1 hashmap3)
              (not (hashmap-disjoint? hashmap1 hashmap2)))",
    );
}

#[test]
fn accessors() {
    assert_int("(hashmap-ref hashmap1 'b)", 2);
    assert_int("(hashmap-ref hashmap1 'd (lambda () 42))", 42);
    assert_int(
        "(hashmap-ref hashmap1 'b (lambda () #f) (lambda (x) (* x x)))",
        4,
    );
    assert_int("(hashmap-ref/default hashmap1 'c 42)", 3);
    assert_int("(hashmap-ref/default hashmap1 'd 42)", 42);
    // hashmap-ref on a missing key without a failure thunk raises.
    let err = run("(hashmap-ref hashmap1 'd)").unwrap_err();
    assert!(matches!(err, EvalError::Raised(_) | EvalError::Runtime(_)));
}

#[test]
fn key_comparator_is_preserved() {
    assert_true("(eq? comparator (hashmap-key-comparator hashmap1))");
}

#[test]
fn updaters_set_and_adjoin() {
    // adjoin keeps the earliest binding for a key already present.
    assert_int(
        "(hashmap-ref (hashmap-adjoin hashmap1 'c 4 'd 4 'd 5) 'c)",
        3,
    );
    assert_int(
        "(hashmap-ref (hashmap-adjoin hashmap1 'c 4 'd 4 'd 5) 'd)",
        4,
    );
    // set overrides, last binding wins.
    assert_int("(hashmap-ref (hashmap-set hashmap1 'c 4 'd 4 'd 5) 'c)", 4);
    assert_int("(hashmap-ref (hashmap-set hashmap1 'c 4 'd 4 'd 5) 'd)", 5);
}

#[test]
fn updaters_replace_and_delete() {
    // replace on a key not in the map is a no-op.
    assert_int(
        "(hashmap-ref/default (hashmap-replace hashmap1 'd 4) 'd 99)",
        99,
    );
    assert_int("(hashmap-ref (hashmap-replace hashmap1 'c 6) 'c)", 6);
    assert_int(
        "(hashmap-ref/default (hashmap-delete hashmap1 'b) 'b 42)",
        42,
    );
    assert_int(
        "(hashmap-ref/default (hashmap-delete-all hashmap1 '(a b)) 'b 42)",
        42,
    );
}

#[test]
fn updaters_intern_and_update() {
    // intern on an existing key returns the stored value unchanged.
    assert_int(
        "(call-with-values
           (lambda () (hashmap-intern hashmap1 'b (lambda () (error \"unused\"))))
           (lambda (m v) v))",
        2,
    );
    // intern on a missing key inserts and returns the new value.
    assert_int(
        "(call-with-values
           (lambda () (hashmap-intern hashmap1 'd (lambda () 42)))
           (lambda (m v) (hashmap-ref m 'd)))",
        42,
    );
    assert_int(
        "(hashmap-ref (hashmap-update hashmap1 'b (lambda (x) (* x x))) 'b)",
        4,
    );
    assert_int(
        "(hashmap-ref (hashmap-update/default hashmap1 'd (lambda (x) (* x x)) 4) 'd)",
        16,
    );
}

#[test]
fn whole_hashmap_queries() {
    assert_int("(hashmap-size hashmap0)", 0);
    assert_int("(hashmap-size hashmap1)", 3);
    assert_int("(hashmap-count (lambda (k v) (>= v 2)) hashmap1)", 2);
    assert_true("(hashmap-any? (lambda (k v) (= v 3)) hashmap1)");
    assert_true("(not (hashmap-any? (lambda (k v) (= v 4)) hashmap1))");
    assert_true("(hashmap-every? (lambda (k v) (<= v 3)) hashmap1)");
    assert_true("(not (hashmap-every? (lambda (k v) (<= v 2)) hashmap1))");
}

#[test]
fn find_locates_entries() {
    // hashmap-find returns the matching key/value pair.
    assert_true(
        "(call-with-values
           (lambda ()
             (hashmap-find (lambda (k v) (and (eq? k 'b) (= v 2)))
                           hashmap1
                           (lambda () (error \"unused\"))))
           (lambda (k v) (and (eq? k 'b) (= v 2))))",
    );
    // Failure thunk is used when nothing matches.
    assert_int(
        "(hashmap-find (lambda (k v) (eq? k 'z)) hashmap1 (lambda () 42))",
        42,
    );
}

#[test]
fn mapping_and_folding() {
    assert_int("(hashmap-fold (lambda (k v acc) (+ v acc)) 0 hashmap1)", 6);
    assert_int("(length (hashmap-keys hashmap1))", 3);
    assert_int("(length (hashmap-values hashmap1))", 3);
    assert_int("(apply + (hashmap-values hashmap1))", 6);
    // hashmap-map remaps values, preserving the value count.
    assert_int(
        "(apply + (hashmap-values (hashmap-map (lambda (k v) (values k (* v 10))) comparator hashmap1)))",
        60,
    );
    // filter keeps a subset; remove drops it.
    assert_int(
        "(hashmap-size (hashmap-filter (lambda (k v) (even? v)) hashmap1))",
        1,
    );
    assert_int(
        "(hashmap-size (hashmap-remove (lambda (k v) (even? v)) hashmap1))",
        2,
    );
}

#[test]
fn conversion_roundtrip() {
    assert_int("(length (hashmap->alist hashmap1))", 3);
    // alist->hashmap rebuilds the same entries.
    assert_int(
        "(hashmap-ref (alist->hashmap comparator '((a . 1) (b . 2) (c . 3))) 'c)",
        3,
    );
    assert_int("(hashmap-size (hashmap-copy hashmap1))", 3);
}

#[test]
fn set_theory_operations() {
    // union keeps the left value on key collisions ('c is 3 in hashmap1).
    assert_int("(hashmap-ref (hashmap-union hashmap1 hashmap2) 'c)", 3);
    assert_int("(hashmap-size (hashmap-union hashmap1 hashmap2))", 5);
    // intersection keeps only shared keys ('c).
    assert_int("(hashmap-size (hashmap-intersection hashmap1 hashmap2))", 1);
    assert_true("(hashmap-contains? (hashmap-intersection hashmap1 hashmap2) 'c)");
    // difference drops shared keys from the left map.
    assert_int("(hashmap-size (hashmap-difference hashmap1 hashmap2))", 2);
    assert_true("(not (hashmap-contains? (hashmap-difference hashmap1 hashmap2) 'c))");
}

#[test]
fn comparison_predicates() {
    assert_true("(hashmap=? comparator hashmap1 hashmap1)");
    assert_true("(not (hashmap=? comparator hashmap1 hashmap2))");
    assert_true("(hashmap<? comparator (hashmap comparator 'a 1) (hashmap comparator 'a 1 'b 2))");
    assert_true("(hashmap>? comparator (hashmap comparator 'a 1 'b 2) (hashmap comparator 'a 1))");
    assert_true("(hashmap<=? comparator hashmap1 hashmap1)");
}

#[test]
fn comparator_export() {
    // The library re-exports comparator? and provides hashmap-comparator.
    assert_true("(comparator? hashmap-comparator)");
    assert_true("(comparator? (make-hashmap-comparator comparator))");
    assert!(equal(
        &run("(comparator? (make-hashmap-comparator comparator))").unwrap(),
        &Value::Bool(true)
    ));
}
