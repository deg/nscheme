//! (scheme ideque) / (srfi 134) tests — bead nscheme-lul.9.
//!
//! Cases are mined and translated from the SRFI 134 reference test
//! suite (ideque-tests.scm, two-list variant, Shiro Kawai, MIT).
//! Rather than port the test harness, each case is run through
//! `eval_source`: `(test-assert EXPR)` becomes `assert_true("EXPR")`,
//! `(test EXPECTED EXPR)` becomes an `equal` check on `run("EXPR")`,
//! and `(test-error EXPR)` becomes an expected `Err`. Only cases whose
//! expressions use this library's exports plus base/quote are used.

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

const PRELUDE: &str = "(import (scheme base) (scheme ideque))";

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

/// Assert that a Scheme expression evaluates to a value `equal?` to the
/// value produced by `expected`.
fn assert_equal(expr: &str, expected: &str) {
    let got = run(expr).unwrap_or_else(|e| panic!("error evaluating `{expr}`: {e:?}"));
    let want = run(expected).unwrap_or_else(|e| panic!("error evaluating `{expected}`: {e:?}"));
    assert!(
        equal(&got, &want),
        "expected `{expected}` from `{expr}`, got {got}"
    );
}

#[test]
fn constructors() {
    assert_equal("(ideque->list (ideque))", "'()");
    assert_equal("(ideque->list (list->ideque '()))", "'()");
    assert_equal("(ideque->list (ideque 1 2 3))", "'(1 2 3)");
    assert_equal("(ideque->list (list->ideque '(4 5 6 7)))", "'(4 5 6 7)");
    assert_equal(
        "(ideque->list (ideque-unfold zero? values (lambda (n) (- n 1)) 10))",
        "'(10 9 8 7 6 5 4 3 2 1)",
    );
    assert_equal(
        "(ideque->list (ideque-unfold-right zero? values (lambda (n) (- n 1)) 10))",
        "'(1 2 3 4 5 6 7 8 9 10)",
    );
    assert_equal(
        "(ideque->list (ideque-tabulate 6 (lambda (n) (* n 2))))",
        "'(0 2 4 6 8 10)",
    );
    assert_equal("(ideque->list (ideque-tabulate 0 values))", "'()");
}

#[test]
fn predicates() {
    assert_true("(ideque? (ideque))");
    assert_true("(not (ideque? 1))");
    assert_true("(ideque-empty? (ideque))");
    assert_true("(not (ideque-empty? (ideque 1)))");
    assert_true("(ideque= eq?)");
    assert_true("(ideque= eq? (ideque 1))");
    assert_true("(ideque= char-ci=? (ideque #\\a #\\b) (ideque #\\A #\\B))");
    assert_true("(ideque= char-ci=? (ideque) (ideque))");
    assert_true("(not (ideque= char-ci=? (ideque #\\a #\\b) (ideque #\\A #\\B #\\c)))");
    assert_true("(ideque= char-ci=? (ideque #\\a #\\b) (ideque #\\A #\\B) (ideque #\\a #\\B))");
}

#[test]
fn queue_operations() {
    run("(ideque-front (ideque))").unwrap_err();
    run("(ideque-back (ideque))").unwrap_err();
    assert_equal("(ideque-front (ideque 1 2 3))", "1");
    assert_equal("(ideque-back (ideque 1 2 3))", "3");
    assert_equal("(ideque-front (ideque-remove-front (ideque 1 2 3)))", "2");
    assert_equal("(ideque-back (ideque-remove-back (ideque 1 2 3)))", "2");
    assert_equal("(ideque-front (ideque-remove-back (ideque 1 2 3)))", "1");
    assert_equal("(ideque-back (ideque-remove-front (ideque 1 2 3)))", "3");
    assert_true("(ideque-empty? (ideque-remove-front (ideque 1)))");
    assert_equal("(ideque-front (ideque-add-front (ideque 1 2 3) 0))", "0");
    assert_equal("(ideque-back (ideque-add-back (ideque 1 2 3) 0))", "0");
}

#[test]
fn accessors() {
    assert_equal(
        "(ideque->list (ideque-take (ideque 1 2 3 4) 4))",
        "'(1 2 3 4)",
    );
    assert_equal(
        "(ideque->list (ideque-take-right (ideque 1 2 3 4) 4))",
        "'(1 2 3 4)",
    );
    assert_equal("(ideque->list (ideque-drop (ideque 1 2 3 4) 4))", "'()");
    assert_equal(
        "(ideque->list (ideque-drop-right (ideque 1 2 3 4) 4))",
        "'()",
    );
    assert_equal(
        "(map (lambda (n) (ideque-ref (ideque 3 2 1) n)) '(0 1 2))",
        "'(3 2 1)",
    );
    run("(ideque-ref (ideque 3 2 1) -1)").unwrap_err();
    run("(ideque-ref (ideque 3 2 1) 3)").unwrap_err();
    run("(ideque->list (ideque-take (ideque 1 2 3 4 5 6 7) 10))").unwrap_err();
}

#[test]
fn whole_ideque() {
    assert_equal("(ideque-length (ideque 1 2 3 4 5 6 7))", "7");
    assert_equal("(ideque-length (ideque))", "0");
    assert_equal("(ideque->list (ideque-append))", "'()");
    assert_equal(
        "(ideque->list (ideque-append (ideque 1 2 3) (ideque 'a 'b 'c 'd) (ideque) (ideque 5 6 7 8 9)))",
        "'(1 2 3 a b c d 5 6 7 8 9)",
    );
    assert_equal("(ideque->list (ideque-reverse (ideque)))", "'()");
    assert_equal(
        "(ideque->list (ideque-reverse (ideque 1 2 3 4 5)))",
        "'(5 4 3 2 1)",
    );
    assert_equal("(ideque-count odd? (ideque 1 2 3 4 5))", "3");
    assert_equal(
        "(ideque->list (ideque-zip (ideque 1 2 3) (ideque 'a 'b 'c 'd 'e)))",
        "'((1 a) (2 b) (3 c))",
    );
}

#[test]
fn mapping() {
    assert_true("(ideque-empty? (ideque-map list (ideque)))");
    assert_equal(
        "(ideque->list (ideque-map - (ideque 1 2 3 4 5)))",
        "'(-1 -2 -3 -4 -5)",
    );
    assert_equal(
        "(ideque->list (ideque-filter-map (lambda (x) (and (number? x) (- x))) (ideque 1 3 'a -5 8)))",
        "'(-1 -3 5 -8)",
    );
    assert_equal(
        "(ideque-fold cons 'z (ideque 1 2 3 4 5))",
        "'(5 4 3 2 1 . z)",
    );
    assert_equal(
        "(ideque-fold-right cons 'z (ideque 1 2 3 4 5))",
        "'(1 2 3 4 5 . z)",
    );
    assert_equal(
        "(ideque->list (ideque-append-map (lambda (x) (list x x)) (ideque 'a 'b 'c)))",
        "'(a a b b c c)",
    );
}

#[test]
fn filtering_and_searching() {
    assert_equal(
        "(ideque->list (ideque-filter odd? (ideque 1 2 3 4 5)))",
        "'(1 3 5)",
    );
    assert_equal(
        "(ideque->list (ideque-remove odd? (ideque 1 2 3 4 5)))",
        "'(2 4)",
    );
    assert_equal(
        "(ideque-find number? (ideque 'a 3 'b 'c 4 'd) (lambda () 'boo))",
        "3",
    );
    assert_equal("(ideque-find number? (ideque 'a 'b 'c 'd))", "#f");
    assert_equal(
        "(ideque-find-right number? (ideque 'a 3 'b 'c 4 'd) (lambda () 'boo))",
        "4",
    );
    assert_equal(
        "(ideque->list (ideque-take-while (lambda (n) (< n 5)) (ideque 1 3 2 5 8 4 6 3 4 2)))",
        "'(1 3 2)",
    );
    assert_equal(
        "(ideque->list (ideque-drop-while (lambda (n) (< n 5)) (ideque 1 3 2 5 8 4 6 3 4 2)))",
        "'(5 8 4 6 3 4 2)",
    );
    assert_equal(
        "(ideque-any (lambda (x) (and (number? x) x)) (ideque 'a 3 'b 'c 4 'd 'e))",
        "3",
    );
    assert_equal(
        "(ideque-every (lambda (x) (and (number? x) x)) (ideque 1 5 3 2 9))",
        "9",
    );
}

#[test]
fn generator_round_trip() {
    // ideque->generator drains in front-to-back order; generator->ideque
    // rebuilds the same sequence.
    assert_equal(
        "(ideque->list (generator->ideque (ideque->generator (ideque 1 2 3 4 5))))",
        "'(1 2 3 4 5)",
    );
}
