//! Tests for the `load` special form (bead nscheme-1f9).
//!
//! `(load FILE)` reads FILE and evaluates its forms in the *current*
//! environment, so its definitions persist — the behavior that lets you
//! `nscheme -i FILE` (or `(load …)` at the REPL) and then call what the
//! file defined.

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Value, equal};

/// Absolute path to a committed fixture file, as a Scheme string literal.
fn fixture(name: &str) -> String {
    let p = PathBuf::from(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ));
    // Embed as a string literal; fixture paths contain no quotes/backslashes.
    format!("\"{}\"", p.display())
}

fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

#[test]
fn load_defines_persist_in_current_env() {
    // After loading the fixture, both its definitions must be callable.
    let src = format!(
        "(load {}) (list loaded-greeting (loaded-double 21))",
        fixture("load-target.scm")
    );
    let v = run(&src).unwrap();
    let expected = Value::list_from(vec![Value::string("hello from load"), Value::Int(42)]);
    assert!(equal(&v, &expected), "got {v:?}");
}

#[test]
fn load_evaluates_its_filename_argument() {
    // The operand is evaluated, so a computed path works.
    let dir = PathBuf::from(format!("{}/tests/fixtures/", env!("CARGO_MANIFEST_DIR")));
    let src = format!(
        "(load (string-append \"{}\" \"load-target.scm\")) (loaded-double 5)",
        dir.display()
    );
    assert!(equal(&run(&src).unwrap(), &Value::Int(10)));
}

#[test]
fn load_missing_file_errors_cleanly() {
    let err = run("(load \"/no/such/file-xyz.scm\")").unwrap_err();
    match err {
        EvalError::MalformedForm { form, .. } => assert_eq!(form, "load"),
        other => panic!("expected a malformed `load` error, got {other:?}"),
    }
}

#[test]
fn load_non_string_argument_errors() {
    let err = run("(load 42)").unwrap_err();
    match err {
        EvalError::MalformedForm { form, message } => {
            assert_eq!(form, "load");
            assert!(message.contains("string"), "message was: {message}");
        }
        other => panic!("expected a malformed `load` error, got {other:?}"),
    }
}
