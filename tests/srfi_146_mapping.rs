//! (scheme mapping) / (srfi 146) tests — bead nscheme-oeg.6.
//!
//! Cases are mined and translated from the SRFI 146 reference test
//! suite (srfi/146/test.sld, Marc Nieper-Wißkirchen, MIT). Rather than
//! port the SRFI-64 harness (not an R7RS-large deliverable), each
//! `(test-assert EXPR)` becomes `assert_true("EXPR")` and each
//! `(test EXPECTED EXPR)` becomes an equality check on `run("EXPR")`,
//! matching this repo's test convention.

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

/// Prelude mirroring the SRFI 146 test suite: `comparator` bound to the
/// default comparator (from `(scheme comparator)`), plus a few sample
/// mappings reused across cases.
const PRELUDE: &str = r"
(import (scheme base) (scheme mapping)
        (only (scheme comparator) make-default-comparator =?))
(define comparator (make-default-comparator))
(define mapping0 (mapping comparator))
(define mapping1 (mapping comparator 'a 1 'b 2 'c 3))
(define mapping2 (mapping comparator 'a 1 'b 2 'd 4))
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

/// Assert that a Scheme expression evaluates to `expected` (read from src).
fn assert_eval(expr: &str, expected: &str) {
    let got = run(expr).unwrap_or_else(|e| panic!("error evaluating `{expr}`: {e:?}"));
    let want = run(expected).unwrap_or_else(|e| panic!("error evaluating `{expected}`: {e:?}"));
    assert!(equal(&got, &want), "expected `{expr}` => {want}, got {got}");
}

#[test]
fn predicates() {
    assert_true("(mapping? (mapping comparator))");
    assert_true("(not (mapping? (list 1 2 3)))");
    assert_true("(mapping-empty? mapping0)");
    assert_true("(not (mapping-empty? mapping1))");
    assert_true("(mapping-contains? mapping1 'b)");
    assert_true("(not (mapping-contains? mapping1 'z))");
}

#[test]
fn disjointness() {
    // mapping1 = {a b c}, mapping2 = {a b d} share keys -> not disjoint.
    assert_true("(not (mapping-disjoint? mapping1 mapping2))");
    assert_true("(mapping-disjoint? mapping1 (mapping comparator 'd 1 'e 2 'f 3))");
}

#[test]
fn accessors() {
    assert_eval("(mapping-ref mapping1 'b)", "2");
    assert_eval("(mapping-ref mapping1 'd (lambda () 42))", "42");
    assert_eval(
        "(mapping-ref mapping1 'b (lambda () #f) (lambda (x) (* x x)))",
        "4",
    );
    assert_eval("(mapping-ref/default mapping1 'c 42)", "3");
    assert_eval("(mapping-ref/default mapping1 'd 42)", "42");
    assert_true("(eq? (mapping-key-comparator mapping1) comparator)");
}

#[test]
fn updaters() {
    // mapping-set replaces; mapping-adjoin keeps the earliest binding.
    assert_eval(
        "(mapping-ref (mapping-set mapping1 'c 4 'd 4 'd 5) 'c)",
        "4",
    );
    assert_eval(
        "(mapping-ref (mapping-set mapping1 'c 4 'd 4 'd 5) 'd)",
        "5",
    );
    assert_eval(
        "(mapping-ref (mapping-adjoin mapping1 'c 4 'd 4 'd 5) 'c)",
        "3",
    );
    assert_eval(
        "(mapping-ref (mapping-adjoin mapping1 'c 4 'd 4 'd 5) 'd)",
        "4",
    );
}

#[test]
fn replace_and_delete() {
    assert_eval(
        "(mapping-ref/default (mapping-replace mapping1 'd 4) 'd #f)",
        "#f",
    );
    assert_eval("(mapping-ref (mapping-replace mapping1 'c 6) 'c)", "6");
    assert_eval(
        "(mapping-ref/default (mapping-delete mapping1 'b) 'b 42)",
        "42",
    );
    assert_eval(
        "(mapping-ref/default (mapping-delete-all mapping1 '(a b)) 'b 42)",
        "42",
    );
}

#[test]
fn update_procedures() {
    assert_eval(
        "(mapping-ref (mapping-update mapping1 'b (lambda (x) (* x x))) 'b)",
        "4",
    );
    assert_eval(
        "(mapping-ref (mapping-update/default mapping1 'd (lambda (x) (* x x)) 4) 'd)",
        "16",
    );
}

#[test]
fn whole_mapping() {
    assert_eval("(mapping-size mapping0)", "0");
    assert_eval("(mapping-size mapping1)", "3");
    assert_eval(
        "(mapping-count (lambda (key value) (>= value 2)) mapping1)",
        "2",
    );
    assert_true("(mapping-any? (lambda (key value) (= value 3)) mapping1)");
    assert_true("(not (mapping-any? (lambda (key value) (= value 4)) mapping1))");
    assert_true("(mapping-every? (lambda (key value) (<= value 3)) mapping1)");
    assert_true("(not (mapping-every? (lambda (key value) (<= value 2)) mapping1))");
    assert_eval("(length (mapping-keys mapping1))", "3");
    assert_eval("(apply + (mapping-values mapping1))", "6");
}

#[test]
fn mapping_and_folding() {
    assert_eval(
        "(mapping-fold (lambda (key value acc) (+ value acc)) 0 mapping1)",
        "6",
    );
    assert_eval(
        "(let ((counter 0))
           (mapping-for-each (lambda (key value) (set! counter (+ counter value))) mapping1)
           counter)",
        "6",
    );
    assert_eval(
        "(apply + (mapping-map->list (lambda (key value) (* value value)) mapping1))",
        "14",
    );
    assert_eval(
        "(mapping-size (mapping-filter (lambda (key value) (<= value 2)) mapping1))",
        "2",
    );
    assert_eval(
        "(mapping-size (mapping-remove (lambda (key value) (<= value 2)) mapping1))",
        "1",
    );
}

#[test]
fn conversion() {
    assert_eval(
        "(mapping-ref (alist->mapping comparator '((a . 1) (b . 2) (c . 3))) 'b)",
        "2",
    );
    assert_eval("(cdr (assq 'b (mapping->alist mapping1)))", "2");
    assert_eval("(mapping-size (mapping-copy mapping1))", "3");
}

#[test]
fn submappings() {
    assert_true("(mapping=? comparator mapping1 (mapping comparator 'a 1 'b 2 'c 3))");
    assert_true("(not (mapping=? comparator mapping1 mapping2))");
    assert_true("(mapping<? comparator (mapping comparator 'a 1 'c 3) mapping1)");
    assert_true("(mapping>? comparator mapping1 (mapping comparator 'a 1 'c 3))");
    assert_true("(mapping<=? comparator (mapping comparator 'a 1 'c 3) mapping1)");
    assert_true("(mapping>=? comparator mapping1 (mapping comparator 'a 1 'c 3))");
}

#[test]
fn set_theory_operations() {
    assert_eval("(mapping-ref (mapping-union mapping1 mapping2) 'd)", "4");
    assert_eval("(mapping-ref (mapping-union mapping1 mapping2) 'c)", "3");
    assert_eval(
        "(mapping-ref (mapping-intersection mapping1 (mapping comparator 'a 1 'b 2 'c 4)) 'c)",
        "3",
    );
    assert_eval(
        "(mapping-size (mapping-difference mapping2 (mapping comparator 'd 4 'e 5 'f 6)))",
        "2",
    );
}

#[test]
fn ordered_key_operations() {
    assert_eval("(mapping-min-key mapping1)", "'a");
    assert_eval("(mapping-max-key mapping1)", "'c");
    assert_eval("(mapping-min-value mapping1)", "1");
    assert_eval("(mapping-max-value mapping1)", "3");
    let m4 = "(mapping comparator 'a 1 'b 2 'c 3 'd 4 'e 5 'f 6)";
    assert_eval(
        &format!("(mapping-values (mapping-range< {m4} 'd))"),
        "'(1 2 3)",
    );
    assert_eval(
        &format!("(mapping-values (mapping-range<= {m4} 'd))"),
        "'(1 2 3 4)",
    );
    assert_eval(
        &format!("(mapping-values (mapping-range> {m4} 'd))"),
        "'(5 6)",
    );
    assert_eval(
        &format!("(mapping-values (mapping-range= {m4} 'd))"),
        "'(4)",
    );
    assert_eval(
        "(mapping-fold/reverse (lambda (key value acc) (cons value acc)) '() mapping1)",
        "'(1 2 3)",
    );
}

#[test]
fn comparators() {
    assert_true("(comparator? mapping-comparator)");
    assert_true("(=? comparator mapping1 (mapping comparator 'a 1 'b 2 'c 3))");
    assert_true("(not (=? comparator mapping1 mapping2))");
    // A mapping used as a key in another mapping.
    assert_eval(
        "(let ((m0 (mapping comparator mapping1 \"a\" mapping2 \"b\")))
           (mapping-ref m0 (mapping comparator 'a 1 'b 2 'c 3)))",
        "\"a\"",
    );
}
