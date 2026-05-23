//! Tests for `values`, `call-with-values`, `let-values`, `let*-values`
//! (R7RS §4.2.2 and §6.10).

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
fn values_single_collapses() {
    // R7RS: (values v) ≡ v.
    assert!(equal(&run("(values 42)").unwrap(), &Value::Int(42)));
}

#[test]
fn call_with_values_unpacks_multi() {
    // Classic R7RS example: divide-and-conquer divmod via (values q r).
    let src = "
        (call-with-values
          (lambda () (values 10 3))
          (lambda (a b) (list 'pair a b)))
    ";
    let v = run(src).unwrap();
    let expected = Value::list_from([
        Value::Symbol(Symbol::intern("pair")),
        Value::Int(10),
        Value::Int(3),
    ]);
    assert!(equal(&v, &expected));
}

#[test]
fn call_with_values_with_single_value_producer() {
    // (values 42) collapses to 42; call-with-values still passes one arg.
    let src = "
        (call-with-values
          (lambda () 99)
          (lambda (x) (* x 2)))
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(198)));
}

#[test]
fn call_with_values_with_zero_values() {
    let src = "
        (call-with-values
          (lambda () (values))
          (lambda () 'no-values))
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("no-values"))
    ));
}

#[test]
fn let_values_destructures() {
    let src = "
        (let-values (((a b) (values 1 2))
                     ((c)   (values 3)))
          (+ a b c))
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(6)));
}

#[test]
fn let_values_with_rest_formal() {
    let src = "
        (let-values (((a . rest) (values 1 2 3 4)))
          (list a rest))
    ";
    let v = run(src).unwrap();
    let expected = Value::list_from([
        Value::Int(1),
        Value::list_from([Value::Int(2), Value::Int(3), Value::Int(4)]),
    ]);
    assert!(equal(&v, &expected));
}

#[test]
fn let_star_values_sequential_bindings() {
    // Each let*-values binding can see the previous ones — same as
    // let* but with multi-value RHS.
    let src = "
        (let*-values (((a b) (values 1 2))
                      ((c)   (values (+ a b))))
          c)
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(3)));
}

#[test]
fn values_packet_detection() {
    // values-packet? distinguishes a multi-packet from a single value.
    assert!(equal(
        &run("(values-packet? (values 1 2))").unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(
        &run("(values-packet? (values 1))").unwrap(),
        &Value::Bool(false)
    ));
    assert!(equal(
        &run("(values-packet? 42)").unwrap(),
        &Value::Bool(false)
    ));
}
