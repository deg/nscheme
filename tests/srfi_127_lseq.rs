//! (scheme lseq) / (srfi 127) tests — bead nscheme-lul.11.
//!
//! Cases are mined and translated from the SRFI 127 reference test
//! suite (lseqs-test.scm, John Cowan, MIT). Rather than port the
//! test harness, related (test …)/(test-assert …) cases are grouped and
//! run through `eval_source`, asserting the combined `(and …)`, matching
//! this repo's test convention.
//!
//! Note: `make-generator`/`make-lseq` are defined in the upstream *test*
//! file, not the library, so they live in PRELUDE here. They are what
//! exercises the lazy (generator-backed) path — a bare proper list only
//! tests the eager fast path. Unrealized lseqs are improper lists
//! `(value . generator)`, so results are `lseq-realize`d before compare.

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

/// Generator/lseq constructors from the SRFI 127 test suite, reused as a prelude.
const PRELUDE: &str = r"
(import (scheme base) (scheme lseq))
(define (make-generator . args)
  (lambda () (if (null? args)
                 (eof-object)
                 (let ((next (car args)))
                   (set! args (cdr args))
                   next))))
(define (make-lseq . args) (generator->lseq (apply make-generator args)))
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
fn constructor_and_laziness() {
    // make-lseq builds an improper list (value . generator): the tail is
    // a procedure until realized, and lseq-realize yields the full list.
    assert_true(
        "(let ((one23 (make-lseq 1 2 3)))
           (and (= 1 (car one23))
                (procedure? (cdr one23))
                (equal? '(1 2 3) (lseq-realize one23))))",
    );
}

#[test]
fn predicates() {
    assert_true(
        "(and (lseq? '())
              (lseq? '(1 2 3))
              (lseq? (make-lseq 1 2 3))
              (lseq? (cons 'x (lambda () 'x)))
              (not (lseq? 7)))",
    );
}

#[test]
fn lseq_equal() {
    assert_true(
        "(and (lseq=? = '() '())
              (lseq=? = '(1 2 3) '(1 2 3))
              (lseq=? = (make-lseq 1 2 3) (make-lseq 1 2 3))
              (lseq=? = (make-lseq 1 2 3) '(1 2 3))
              (not (lseq=? = '(1 2 3) '(1 2 4))))",
    );
}

#[test]
fn selectors_car_cdr_ref() {
    assert_true(
        "(and (= 1 (lseq-car (make-lseq 1 2 3)))
              (= 1 (lseq-car '(1 2 3)))
              (= 1 (lseq-first (make-lseq 1 2 3)))
              (= 2 (lseq-car (lseq-cdr '(1 2 3))))
              (= 2 (lseq-car (lseq-cdr (make-lseq 1 2 3))))
              (= 2 (lseq-car (lseq-rest (make-lseq 1 2 3))))
              (= 1 (lseq-ref '(1) 0))
              (= 2 (lseq-ref (make-lseq 1 2) 1)))",
    );
}

#[test]
fn take_and_drop() {
    assert_true(
        "(and (procedure? (cdr (lseq-take '(1 2 3 4 5) 3)))
              (equal? '(1 2 3) (lseq-realize (lseq-take '(1 2 3 4 5) 3)))
              (equal? '(3 4 5) (lseq-realize (lseq-drop '(1 2 3 4 5) 2)))
              (equal? '(3 4 5) (lseq-realize (lseq-drop (make-lseq 1 2 3 4 5) 2))))",
    );
}

#[test]
fn whole_length_append_zip() {
    assert_true(
        "(and (= 0 (lseq-length '()))
              (= 3 (lseq-length (make-lseq 1 2 3)))
              (equal? '(1 2 3 a b c) (lseq-realize (lseq-append '(1 2 3) '(a b c))))
              (equal? '((one 1 odd) (two 2 even) (three 3 odd))
                      (lseq-realize
                        (lseq-zip '(one two three)
                                  (make-lseq 1 2 3 4 5)
                                  (make-lseq 'odd 'even 'odd 'even 'odd)))))",
    );
}

#[test]
fn lseq_to_generator() {
    assert_true(
        "(let ((g (lseq->generator (make-lseq 1 2 3))))
           (and (= 1 (g)) (= 2 (g)) (= 3 (g)) (eof-object? (g))))",
    );
}

#[test]
fn mapping() {
    assert_true(
        "(and (equal? '() (lseq-map - '()))
              (equal? '(-1 -2 -3) (lseq-realize (lseq-map - '(1 2 3))))
              (equal? '(-1 -2 -3) (lseq-realize (lseq-map - (make-lseq 1 2 3))))
              (procedure? (cdr (lseq-map - '(1 2 3)))))",
    );
}

#[test]
fn for_each_is_eager() {
    assert_true(
        "(let ((output '()))
           (lseq-for-each (lambda (x) (set! output (cons x output)))
                          (make-lseq 1 2 3))
           (equal? output '(3 2 1)))",
    );
}

#[test]
fn filter_and_remove() {
    assert_true(
        "(and (procedure? (cdr (lseq-filter odd? '(1 2 3 4 5))))
              (equal? '(1 3 5) (lseq-realize (lseq-filter odd? '(1 2 3 4 5))))
              (equal? '(1 3 5) (lseq-realize (lseq-filter odd? (make-lseq 1 2 3 4 5))))
              (equal? '(1 3 5) (lseq-realize (lseq-remove even? (make-lseq 1 2 3 4 5)))))",
    );
}

#[test]
fn searching_find_and_tail() {
    assert_true(
        "(and (= 4 (lseq-find even? '(3 1 4 1 5 9 2 6)))
              (= 4 (lseq-find even? (make-lseq 3 1 4 1 5 9 2 6)))
              (not (lseq-find negative? (make-lseq 1 2 3 4 5)))
              (equal? '(-8 -5 0 0) (lseq-realize (lseq-find-tail even? '(3 1 37 -8 -5 0 0))))
              (not (lseq-find-tail even? '())))",
    );
}

#[test]
fn take_while_drop_while() {
    assert_true(
        "(and (equal? '(2 18) (lseq-realize (lseq-take-while even? '(2 18 3 10 22 9))))
              (equal? '(2 18) (lseq-realize (lseq-take-while even? (make-lseq 2 18 3 10 22 9))))
              (equal? '(3 10 22 9) (lseq-drop-while even? '(2 18 3 10 22 9)))
              (equal? '(3 10 22 9)
                      (lseq-realize (lseq-drop-while even? (make-lseq 2 18 3 10 22 9)))))",
    );
}

#[test]
fn any_every_index() {
    assert_true(
        // A SRFI 127 lseq is an ordinary (possibly lazy) list, so a
        // plain list is a valid lseq. lseq-every returns the last
        // truthy predicate result: (* 3 3) = 9.
        "(and (lseq-any integer? '(a 3 b 2.7))
              (not (lseq-any integer? '(a 3.1 b 2.7)))
              (lseq-any < '(3 1 4 1 5) '(2 7 1 8 2))
              (= 9 (lseq-every (lambda (n) (if (= n 0) 1 (* n n))) '(1 2 3)))
              (= 2 (lseq-index even? '(3 1 4 1 5 9)))
              (= 1 (lseq-index < '(3 1 4 1 5 9 2 5 6) '(2 7 1 8 2)))
              (not (lseq-index = '(3 1 4 1 5 9 2 5 6) '(2 7 1 8 2))))",
    );
}

#[test]
fn member_variants() {
    assert_true(
        "(and (equal? '(a b c) (lseq-realize (lseq-memq 'a (make-lseq 'a 'b 'c))))
              (not (lseq-memq 'a (make-lseq 'b 'c 'd)))
              (equal? '(101 102) (lseq-realize (lseq-memv 101 (make-lseq 100 101 102))))
              (equal? '((a) c) (lseq-realize (lseq-member (list 'a) (make-lseq 'b '(a) 'c))))
              (equal? '(2 3) (lseq-realize (lseq-member 2.0 (make-lseq 1 2 3) =))))",
    );
}
