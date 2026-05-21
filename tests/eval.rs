//! End-to-end integration tests for the lex → parse → eval pipeline.
//!
//! These tests bypass each module's individual surface and run small
//! programs as strings, catching mismatches between contracts that
//! per-module unit tests might miss.

use std::rc::Rc;

use nscheme::env::{Env, EnvRef};
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Arity, PrimitiveFn, Procedure, RuntimeError, Symbol, Value, equal};

fn intern_def(env: &EnvRef, name: &'static str, arity: Arity, body: PrimitiveFn) {
    let p = Procedure::Primitive { name, arity, body };
    env.define(Symbol::intern(name), Value::Procedure(Rc::new(p)));
}

fn base_env() -> EnvRef {
    let env = Env::new_global();
    intern_def(&env, "+", Arity::AtLeast(0), |args| {
        let mut acc: i64 = 0;
        for a in args {
            match a {
                Value::Int(n) => {
                    acc = acc
                        .checked_add(*n)
                        .ok_or_else(|| RuntimeError::Other("integer overflow in +".into()))?;
                }
                other => {
                    return Err(RuntimeError::Type {
                        expected: "integer".into(),
                        got: other.type_name().into(),
                    });
                }
            }
        }
        Ok(Value::Int(acc))
    });
    intern_def(&env, "-", Arity::AtLeast(1), |args| {
        let first = match &args[0] {
            Value::Int(n) => *n,
            other => {
                return Err(RuntimeError::Type {
                    expected: "integer".into(),
                    got: other.type_name().into(),
                });
            }
        };
        if args.len() == 1 {
            return Ok(Value::Int(-first));
        }
        let mut acc = first;
        for a in &args[1..] {
            match a {
                Value::Int(n) => {
                    acc = acc
                        .checked_sub(*n)
                        .ok_or_else(|| RuntimeError::Other("integer overflow in -".into()))?
                }
                other => {
                    return Err(RuntimeError::Type {
                        expected: "integer".into(),
                        got: other.type_name().into(),
                    });
                }
            }
        }
        Ok(Value::Int(acc))
    });
    intern_def(&env, "=", Arity::AtLeast(2), |args| {
        let first = match &args[0] {
            Value::Int(n) => *n,
            other => {
                return Err(RuntimeError::Type {
                    expected: "integer".into(),
                    got: other.type_name().into(),
                });
            }
        };
        for a in &args[1..] {
            match a {
                Value::Int(n) if *n == first => {}
                Value::Int(_) => return Ok(Value::Bool(false)),
                other => {
                    return Err(RuntimeError::Type {
                        expected: "integer".into(),
                        got: other.type_name().into(),
                    });
                }
            }
        }
        Ok(Value::Bool(true))
    });
    intern_def(&env, "list", Arity::AtLeast(0), |args| {
        Ok(Value::list_from(args.iter().cloned()))
    });
    env
}

fn run(source: &str) -> Result<Value, EvalError> {
    eval_source(source, base_env())
}

#[test]
fn arithmetic_through_full_pipeline() {
    assert!(equal(&run("(+ 1 2 3 4)").unwrap(), &Value::Int(10)));
    assert!(equal(&run("(- 10 1 2 3)").unwrap(), &Value::Int(4)));
    assert!(equal(&run("(+ (- 10 4) (- 5 2))").unwrap(), &Value::Int(9)));
}

#[test]
fn lexical_scope_works_end_to_end() {
    let v = run("(define x 10) (define (add-x y) (+ x y)) (define x 99) (add-x 5)").unwrap();
    // R7RS: `(define x 99)` rebinds at top level, but the closure `add-x`
    // captured the *binding*, so the lookup sees the new 99. So result = 104.
    assert!(equal(&v, &Value::Int(104)));
}

#[test]
fn closures_remember_their_scope() {
    let v = run("(define (make-adder n) (lambda (x) (+ x n)))
         (define add5 (make-adder 5))
         (add5 7)")
    .unwrap();
    assert!(equal(&v, &Value::Int(12)));
}

#[test]
fn mutual_recursion() {
    // Mutual recursion needs closures to share the env where both
    // functions are defined.
    let v = run("(define (even? n) (if (= n 0) #t (odd? (- n 1))))
         (define (odd?  n) (if (= n 0) #f (even? (- n 1))))
         (even? 10)")
    .unwrap();
    assert!(equal(&v, &Value::Bool(true)));
}

#[test]
fn deep_tail_recursion_through_pipeline() {
    let v = run("(define (loop n) (if (= n 0) 'done (loop (- n 1))))
         (loop 100000)")
    .unwrap();
    assert!(equal(&v, &Value::Symbol(Symbol::intern("done"))));
}

#[test]
fn quoted_data_round_trips() {
    let v = run("'(1 (2 3) \"hi\")").unwrap();
    let inner = Value::list_from([Value::Int(2), Value::Int(3)]);
    let expected = Value::list_from([Value::Int(1), inner, Value::string("hi")]);
    assert!(equal(&v, &expected));
}
