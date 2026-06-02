//! (scheme box) / (srfi 111) tests — bead nscheme-lul.2.
//! Cases follow the SRFI 111 specification examples.

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::library::set_search_path;
use nscheme::value::{Value, equal};

fn lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/lib", env!("CARGO_MANIFEST_DIR")))
}

fn run(expr: &str) -> Result<Value, EvalError> {
    set_search_path(vec![lib_dir()]);
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&format!("(import (scheme base) (scheme box))\n{expr}"), env)
}

fn assert_true(expr: &str) {
    match run(expr) {
        Ok(Value::Bool(true)) => {}
        Ok(other) => panic!("expected #t from `{expr}`, got {other}"),
        Err(e) => panic!("error evaluating `{expr}`: {e:?}"),
    }
}

#[test]
fn box_predicate() {
    assert_true("(box? (box 1))");
    assert_true("(not (box? 1))");
    assert_true("(not (box? '(1)))");
}

#[test]
fn unbox_returns_contents() {
    assert!(equal(&run("(unbox (box 10))").unwrap(), &Value::Int(10)));
    assert!(equal(
        &run("(unbox (box (+ 2 3)))").unwrap(),
        &Value::Int(5)
    ));
}

#[test]
fn set_box_mutates_contents() {
    assert_true("(let ((b (box 1))) (set-box! b 42) (= (unbox b) 42))");
}

#[test]
fn boxes_hold_arbitrary_values() {
    assert_true("(equal? (unbox (box '(a b c))) '(a b c))");
    assert_true("(let ((inner (box 5))) (= 5 (unbox (unbox (box inner)))))");
}
