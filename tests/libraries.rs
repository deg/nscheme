//! R7RS library / module system tests.

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
fn import_scheme_base_is_no_op() {
    // (scheme base) is the built-in library — already in env.
    let v = run("(import (scheme base)) (+ 1 2)").unwrap();
    assert!(equal(&v, &Value::Int(3)));
}

#[test]
fn import_unknown_library_errors() {
    let err = run("(import (nope))").unwrap_err();
    assert!(matches!(err, EvalError::MalformedForm { .. }));
}

#[test]
fn define_library_then_import() {
    let src = "
        (define-library (test math)
          (export square)
          (begin
            (define (square x) (* x x))))
        (import (test math))
        (square 7)
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(49)));
}

#[test]
fn library_does_not_leak_non_exported_bindings() {
    let src = "
        (define-library (test internal)
          (export public-fn)
          (begin
            (define (private-helper x) (* x 100))
            (define (public-fn x) (private-helper x))))
        (import (test internal))
        (public-fn 5)
    ";
    // public-fn works (and internally uses private-helper).
    assert!(equal(&run(src).unwrap(), &Value::Int(500)));

    // But private-helper is not visible.
    let src = "
        (define-library (test internal2)
          (export public-fn)
          (begin
            (define (private-helper x) (* x 100))
            (define (public-fn x) (private-helper x))))
        (import (test internal2))
        (private-helper 5)
    ";
    let err = run(src).unwrap_err();
    assert!(matches!(
        err,
        EvalError::Runtime(nscheme::value::RuntimeError::Undefined(name)) if name == "private-helper"
    ));
}

#[test]
fn cond_expand_picks_matching_branch() {
    let src = "
        (cond-expand
          (nscheme 'is-nscheme)
          (else 'is-other))
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("is-nscheme"))
    ));
}

#[test]
fn cond_expand_else_when_nothing_matches() {
    let src = "
        (cond-expand
          (nonexistent-feature 'no)
          (else 'yes))
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("yes"))
    ));
}

#[test]
fn cond_expand_and_or_not() {
    let src = "
        (cond-expand
          ((and r7rs nscheme) 'both)
          (else 'no))
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("both"))
    ));

    let src = "
        (cond-expand
          ((and r7rs (not nscheme)) 'never)
          (else 'else-branch))
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("else-branch"))
    ));
}

#[test]
fn cond_expand_library_clause() {
    let src = "
        (cond-expand
          ((library (scheme base)) 'have-base)
          (else 'missing))
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("have-base"))
    ));
}

#[test]
fn features_lists_implementation_features() {
    let v = run("(features)").unwrap();
    // Result should be a list containing at least 'r7rs and 'nscheme.
    let s = format!("{v}");
    assert!(s.contains("r7rs"), "features doesn't contain r7rs: {s}");
    assert!(
        s.contains("nscheme"),
        "features doesn't contain nscheme: {s}"
    );
}
