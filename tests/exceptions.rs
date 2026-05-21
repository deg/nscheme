//! Exception-handling tests (R7RS §6.11).

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
fn raise_without_handler_bubbles_out() {
    let err = run("(raise 'oops)").unwrap_err();
    match err {
        EvalError::Raised(v) => {
            assert!(equal(&v, &Value::Symbol(Symbol::intern("oops"))));
        }
        other => panic!("expected EvalError::Raised, got {other:?}"),
    }
}

#[test]
fn with_exception_handler_catches_raise() {
    // R7RS: for `raise`, the handler is called; its return value is
    // re-raised. So with a single handler that ignores the value, we
    // still get an unhandled raise — unless the handler invokes a
    // continuation to escape. The escape pattern via call/cc is the
    // idiomatic way.
    let src = "
        (call/cc
          (lambda (k)
            (with-exception-handler
              (lambda (v) (k (list 'caught v)))
              (lambda () (raise 'boom)))))
    ";
    let v = run(src).unwrap();
    let expected = Value::list_from([
        Value::Symbol(Symbol::intern("caught")),
        Value::Symbol(Symbol::intern("boom")),
    ]);
    assert!(equal(&v, &expected));
}

#[test]
fn raise_continuable_uses_handler_result() {
    // raise-continuable: the handler's return value substitutes.
    let src = "
        (with-exception-handler
          (lambda (v) (+ v 1000))
          (lambda () (+ 1 (raise-continuable 42))))
    ";
    // The (raise-continuable 42) inside +1 becomes 1042 (handler
    // returns 1042), so the outer + gives 1 + 1042 = 1043.
    assert!(equal(&run(src).unwrap(), &Value::Int(1043)));
}

#[test]
fn guard_handles_matching_clause() {
    let src = "
        (guard (e ((eq? e 'small) 'caught-small)
                  ((eq? e 'big)   'caught-big))
          (raise 'big))
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("caught-big"))
    ));
}

#[test]
fn guard_no_match_re_raises() {
    let src = "
        (guard (e ((eq? e 'expected) 'caught))
          (raise 'unexpected))
    ";
    let err = run(src).unwrap_err();
    assert!(matches!(err, EvalError::Raised(_)));
}

#[test]
fn guard_else_clause() {
    let src = "
        (guard (e (else (list 'unknown e)))
          (raise 'mystery))
    ";
    let v = run(src).unwrap();
    let expected = Value::list_from([
        Value::Symbol(Symbol::intern("unknown")),
        Value::Symbol(Symbol::intern("mystery")),
    ]);
    assert!(equal(&v, &expected));
}

#[test]
fn guard_normal_return_value() {
    // No raise; body returns its value normally.
    let src = "(guard (e (else 'caught)) (+ 1 2 3))";
    assert!(equal(&run(src).unwrap(), &Value::Int(6)));
}

#[test]
fn error_constructs_and_raises() {
    let src = "
        (guard (e (else (list (error-object-message e)
                              (error-object-irritants e))))
          (error \"bad input\" 'foo 42))
    ";
    let v = run(src).unwrap();
    let expected = Value::list_from([
        Value::string("bad input"),
        Value::list_from([Value::Symbol(Symbol::intern("foo")), Value::Int(42)]),
    ]);
    assert!(equal(&v, &expected));
}

#[test]
fn error_object_predicate() {
    let src = "
        (guard (e (else (error-object? e)))
          (error \"x\"))
    ";
    assert!(equal(&run(src).unwrap(), &Value::Bool(true)));
}
