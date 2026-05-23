//! Tests for `delay` / `force` / `make-promise` (R7RS §4.2.5).

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Value, equal};

fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

#[test]
fn delay_does_not_evaluate_body() {
    // The body has a side effect; delay alone shouldn't trigger it.
    let src = "
        (define run 0)
        (define p (delay (begin (set! run 1) 42)))
        run
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(0)));
}

#[test]
fn force_evaluates_once_and_caches() {
    let src = "
        (define counter 0)
        (define p (delay (begin (set! counter (+ counter 1)) counter)))
        (list (force p) (force p) (force p))
    ";
    // All three forces return 1 — the value was computed once and
    // cached.
    let v = run(src).unwrap();
    let expected = Value::list_from([Value::Int(1), Value::Int(1), Value::Int(1)]);
    assert!(equal(&v, &expected));
}

#[test]
fn force_on_non_promise_returns_it() {
    // R7RS: force on a non-promise returns the value as-is.
    assert!(equal(&run("(force 42)").unwrap(), &Value::Int(42)));
}

#[test]
fn force_chains_through_nested_promises() {
    // (delay (delay 42)) — force should unwrap to 42.
    let src = "(force (delay (delay 42)))";
    assert!(equal(&run(src).unwrap(), &Value::Int(42)));
}

#[test]
fn make_promise_creates_forced_promise() {
    let src = "
        (define p (make-promise 99))
        (list (promise? p) (force p))
    ";
    let v = run(src).unwrap();
    let expected = Value::list_from([Value::Bool(true), Value::Int(99)]);
    assert!(equal(&v, &expected));
}

#[test]
fn promise_predicate() {
    assert!(equal(
        &run("(promise? (delay 1))").unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(&run("(promise? 1)").unwrap(), &Value::Bool(false)));
}

#[test]
fn promise_captures_definition_env() {
    // The delay's body refers to a binding from its lexical scope,
    // not from the force-site scope. R7RS says delay's expression
    // executes in the scope where the delay form appeared.
    let src = "
        (define p
          (let ((x 100))
            (delay x)))
        (let ((x 999))
          (force p))
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(100)));
}
