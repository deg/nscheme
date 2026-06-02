//! (scheme charset) / (srfi 14) tests — bead nscheme-lul.7.
//!
//! Cases are mined and translated from the SRFI 14 reference test suite
//! (srfi-14-tests.scm, Olin Shivers, MIT-derived). The upstream suite is
//! one big `(test form ...)` over ASCII char-sets; each clause is run
//! through `eval_source` and asserted, matching this repo's convention.
//! All mined cases stay within ASCII so they pass on the Latin-1-only
//! reference implementation.

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

/// Helpers reused by the SRFI 14 test suite.
const PRELUDE: &str = r"
(import (scheme base) (scheme char) (scheme charset))
(define (vowel? c) (and (member c '(#\a #\e #\i #\o #\u)) #t))
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
fn predicate_and_constructors() {
    assert_true(
        "(and (not (char-set? 5))
              (char-set? (char-set #\\a #\\e #\\i #\\o #\\u)))",
    );
}

#[test]
fn equality_and_subset() {
    assert_true(
        "(and (char-set=)
              (char-set= (char-set))
              (char-set= (char-set #\\a #\\e #\\i #\\o #\\u)
                         (string->char-set \"ioeauaiii\"))
              (not (char-set= (char-set #\\e #\\i #\\o #\\u)
                              (string->char-set \"ioeauaiii\")))
              (char-set<=)
              (char-set<= (char-set))
              (char-set<= (char-set #\\a #\\e #\\i #\\o #\\u)
                          (string->char-set \"ioeauaiii\"))
              (char-set<= (char-set #\\e #\\i #\\o #\\u)
                          (string->char-set \"ioeauaiii\")))",
    );
}

#[test]
fn hash_is_bounded() {
    assert_true("(<= 0 (char-set-hash char-set:graphic 100) 99)");
}

#[test]
fn fold_counts_members() {
    assert_true(
        "(= 4 (char-set-fold (lambda (c i) (+ i 1)) 0
                             (char-set #\\e #\\i #\\o #\\u #\\e #\\e)))",
    );
}

#[test]
fn unfold_builds_sets() {
    assert_true(
        "(char-set= (string->char-set \"eiaou2468013579999\")
                    (char-set-unfold null? car cdr '(#\\a #\\e #\\i #\\o #\\u #\\u #\\u)
                                     char-set:digit))",
    );
    assert_true(
        "(char-set= (string->char-set \"eiaou246801357999\")
                    (char-set-unfold! null? car cdr '(#\\a #\\e #\\i #\\o #\\u)
                                      (string->char-set \"0123456789\")))",
    );
}

#[test]
fn for_each_and_delete() {
    assert_true(
        "(let ((cs (string->char-set \"0123456789\")))
           (char-set-for-each (lambda (c) (set! cs (char-set-delete cs c)))
                              (string->char-set \"02468000\"))
           (char-set= cs (string->char-set \"97531\")))",
    );
}

#[test]
fn map_copy_list_and_string() {
    assert_true(
        "(and (char-set= (char-set-map char-upcase (string->char-set \"aeiou\"))
                         (string->char-set \"IOUAEEEE\"))
              (char-set= (char-set-copy (string->char-set \"aeiou\"))
                         (string->char-set \"aeiou\"))
              (char-set= (string->char-set \"xy\") (list->char-set '(#\\x #\\y)))
              (equal? '(#\\x) (char-set->list (char-set #\\x)))
              (equal? \"x\" (char-set->string (char-set #\\x))))",
    );
}

#[test]
fn filter_and_ucs_range() {
    assert_true(
        "(and (char-set= (string->char-set \"aeiou12345\")
                         (char-set-filter vowel? char-set:ascii
                                          (string->char-set \"12345\")))
              (char-set= (string->char-set \"abcdef12345\")
                         (ucs-range->char-set 97 103 #t
                                              (string->char-set \"12345\"))))",
    );
}

#[test]
fn count_size_and_contains() {
    assert_true(
        "(and (= 10 (char-set-size (char-set-intersection char-set:ascii char-set:digit)))
              (= 5 (char-set-count vowel? char-set:ascii))
              (char-set-contains? (->char-set \"xyz\") #\\x)
              (not (char-set-contains? (->char-set \"xyz\") #\\a)))",
    );
}

#[test]
fn every_and_any() {
    assert_true(
        "(and (char-set-every char-lower-case? (->char-set \"abcd\"))
              (not (char-set-every char-lower-case? (->char-set \"abcD\")))
              (char-set-any char-lower-case? (->char-set \"abcd\"))
              (not (char-set-any char-lower-case? (->char-set \"ABCD\"))))",
    );
}

#[test]
fn cursors_traverse_the_set() {
    assert_true(
        "(char-set= (->char-set \"ABCD\")
                    (let ((cs (->char-set \"abcd\")))
                      (let lp ((cur (char-set-cursor cs)) (ans '()))
                        (if (end-of-char-set? cur) (list->char-set ans)
                            (lp (char-set-cursor-next cs cur)
                                (cons (char-upcase (char-set-ref cs cur)) ans))))))",
    );
}

#[test]
fn adjoin_and_delete_chars() {
    assert_true(
        "(and (char-set= (char-set-adjoin (->char-set \"123\") #\\x #\\a)
                         (->char-set \"123xa\"))
              (char-set= (char-set-delete (->char-set \"123\") #\\2 #\\a #\\2)
                         (->char-set \"13\")))",
    );
}

#[test]
fn set_algebra() {
    assert_true(
        "(and (char-set= (char-set-intersection char-set:hex-digit
                                                (char-set-complement char-set:digit))
                         (->char-set \"abcdefABCDEF\"))
              (char-set= (char-set-union char-set:hex-digit
                                         (->char-set \"abcdefghijkl\"))
                         (->char-set \"abcdefABCDEFghijkl0123456789\"))
              (char-set= (char-set-difference (->char-set \"abcdefghijklmn\")
                                              char-set:hex-digit)
                         (->char-set \"ghijklmn\"))
              (char-set= (char-set-xor (->char-set \"0123456789\")
                                       char-set:hex-digit)
                         (->char-set \"abcdefABCDEF\")))",
    );
}

#[test]
fn diff_plus_intersection() {
    assert_true(
        "(call-with-values
           (lambda ()
             (char-set-diff+intersection char-set:hex-digit char-set:letter))
           (lambda (d i)
             (and (char-set= d (->char-set \"0123456789\"))
                  (char-set= i (->char-set \"abcdefABCDEF\")))))",
    );
}
