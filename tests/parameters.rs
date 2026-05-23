//! Tests for `make-parameter` and `parameterize` (R7RS §4.2.6).

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
fn parameter_read_returns_initial_value() {
    let v = run("(define p (make-parameter 42)) (p)").unwrap();
    assert!(equal(&v, &Value::Int(42)));
}

#[test]
fn parameter_direct_set() {
    // Calling a parameter with one arg sets it.
    let v = run("(define p (make-parameter 1)) (p 99) (p)").unwrap();
    assert!(equal(&v, &Value::Int(99)));
}

#[test]
fn parameterize_overrides_for_body() {
    let v = run("(define p (make-parameter 'outer))
         (list (p)
               (parameterize ((p 'inner)) (p))
               (p))")
    .unwrap();
    let expected = Value::list_from([
        Value::Symbol(Symbol::intern("outer")),
        Value::Symbol(Symbol::intern("inner")),
        Value::Symbol(Symbol::intern("outer")),
    ]);
    assert!(equal(&v, &expected));
}

#[test]
fn parameterize_multiple_bindings() {
    let v = run("(define p1 (make-parameter 1))
         (define p2 (make-parameter 2))
         (parameterize ((p1 10) (p2 20))
           (+ (p1) (p2)))")
    .unwrap();
    assert!(equal(&v, &Value::Int(30)));
}

#[test]
fn parameterize_nests() {
    let v = run("(define p (make-parameter 1))
         (parameterize ((p 2))
           (let ((middle (p)))
             (parameterize ((p 3))
               (list middle (p)))))")
    .unwrap();
    let expected = Value::list_from([Value::Int(2), Value::Int(3)]);
    assert!(equal(&v, &expected));
}

#[test]
fn parameterize_restores_after_uncaught_raise() {
    // R7RS-style: a raise that unwinds past a parameterize should
    // still restore the parameter. Our impl does this via the
    // ParameterRestore frame being fired during unwind.
    let src = "
        (define p (make-parameter 'outer))
        (guard (e (else (p)))
          (parameterize ((p 'inner))
            (raise 'boom)))
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("outer"))
    ));
}

#[test]
fn parameter_is_a_procedure() {
    let v = run("(procedure? (make-parameter 0))").unwrap();
    assert!(equal(&v, &Value::Bool(true)));
}
