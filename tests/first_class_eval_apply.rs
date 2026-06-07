//! eval / apply / load as first-class procedures (bead nscheme-iii).
//!
//! They were special forms (primitives can't reach the evaluator/env);
//! now they are real `Procedure` values that can be passed around.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Value, equal};

fn run(src: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&format!("(import (scheme base) (scheme eval))\n{src}"), env)
}

fn eq(expr: &str, expected: &str) {
    let v = run(&format!("(equal? {expr} {expected})")).unwrap();
    assert!(equal(&v, &Value::Bool(true)), "{expr} should equal {expected}");
}
fn truthy(expr: &str) {
    assert!(equal(&run(expr).unwrap(), &Value::Bool(true)), "{expr} should be #t");
}

#[test]
fn apply_basic_and_spread() {
    eq("(apply + 1 2 '(3 4 5))", "15");
    eq("(apply max '(3 1 4 1 5 9))", "9");
    eq("(apply list '())", "'()");
}

#[test]
fn apply_is_a_value() {
    truthy("(procedure? apply)");
    eq("(map (lambda (xs) (apply + xs)) '((1 2) (3 4) (5 6)))", "'(3 7 11)");
    // apply applied via apply:
    eq("(apply apply (list + '(1 2 3)))", "6");
}

#[test]
fn eval_basic_and_value() {
    truthy("(procedure? eval)");
    eq("(eval '(* 7 3) (environment '(scheme base)))", "21");
    eq("(map (lambda (e) (eval e (environment '(scheme base)))) '((+ 1 1) (* 2 3)))", "'(2 6)");
    // apply + eval together:
    eq("(apply eval (list '(+ 2 2) (environment '(scheme base))))", "4");
}

#[test]
fn load_is_a_procedure() {
    truthy("(procedure? load)");
}

#[test]
fn arity_and_type_errors_are_catchable() {
    truthy("(guard (e (#t #t)) (apply +))");          // too few args
    truthy("(guard (e (#t #t)) (apply + 1 2 3))");    // last arg not a list
    truthy("(guard (e (#t #t)) (eval))");             // eval arity
}
