//! Tests for `call/cc` and related continuation operations.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Symbol, Value, equal};

fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

#[test]
fn call_cc_returns_value_when_continuation_unused() {
    // (call/cc (lambda (k) 42)) just returns 42 — k is never invoked.
    let v = run("(call/cc (lambda (k) 42))").unwrap();
    assert!(equal(&v, &Value::Int(42)));
}

#[test]
fn escape_continuation() {
    // Classic escape: jump out of a computation.
    let src = "
        (define result
          (+ 1 (call/cc (lambda (k) (+ 2 (k 100))))))
        result
    ";
    // Without escape: 1 + (2 + 100) = 103.
    // WITH escape: (k 100) replaces the call/cc with 100, so the outer
    // `+` gets 1 and 100 → 101.
    assert!(equal(&run(src).unwrap(), &Value::Int(101)));
}

#[test]
fn continuation_as_value() {
    // Capture a continuation, store it, then invoke later.
    let src = "
        (define saved #f)
        (define (capture) (call/cc (lambda (k) (set! saved k) 'first)))
        (define a (capture))
        (if (eq? a 'first)
            (saved 'second)
            a)
    ";
    let v = run(src).unwrap();
    assert!(equal(&v, &Value::Symbol(Symbol::intern("second"))));
}

#[test]
fn call_cc_alias_call_with_current_continuation() {
    let v = run("(call-with-current-continuation (lambda (k) 'ok))").unwrap();
    assert!(equal(&v, &Value::Symbol(Symbol::intern("ok"))));
}

#[test]
fn dynamic_wind_runs_before_thunk_after() {
    // The bootstrap dynamic-wind runs before, then thunk, then after.
    let src = "
        (define trace '())
        (define (note! x) (set! trace (cons x trace)))
        (dynamic-wind
          (lambda () (note! 'before))
          (lambda () (note! 'thunk) 42)
          (lambda () (note! 'after)))
        (reverse trace)
    ";
    let v = run(src).unwrap();
    let expected = Value::list_from([
        Value::Symbol(Symbol::intern("before")),
        Value::Symbol(Symbol::intern("thunk")),
        Value::Symbol(Symbol::intern("after")),
    ]);
    assert!(equal(&v, &expected));
}

#[test]
fn apply_with_arglist() {
    assert!(equal(
        &run("(apply + '(1 2 3 4))").unwrap(),
        &Value::Int(10)
    ));
    assert!(equal(
        &run("(apply + 10 '(1 2 3))").unwrap(),
        &Value::Int(16)
    ));
}

#[test]
fn apply_to_user_procedure() {
    let v = run("(define (sum-three a b c) (+ a b c))
         (apply sum-three '(1 2 3))")
    .unwrap();
    assert!(equal(&v, &Value::Int(6)));
}
