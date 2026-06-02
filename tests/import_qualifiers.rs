//! Import-set qualifier tests (bead nscheme-e0s): only / except /
//! rename / prefix, per R7RS §5.6.1, against both an on-disk library
//! and a built-in library.

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::library::set_search_path;
use nscheme::value::{RuntimeError, Value, equal};

fn fixture_lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/tests/fixtures/lib", env!("CARGO_MANIFEST_DIR")))
}

fn run(source: &str) -> Result<Value, EvalError> {
    set_search_path(vec![fixture_lib_dir()]);
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

fn is_undefined(err: &EvalError, name: &str) -> bool {
    matches!(err, EvalError::Runtime(RuntimeError::Undefined(n)) if n == name)
}

// (loadtest b) exports: b-value, bump-b!, read-b

#[test]
fn only_brings_listed_names_and_excludes_the_rest() {
    let v = run("(import (only (loadtest b) read-b)) (read-b)").unwrap();
    assert!(equal(&v, &Value::Int(42)));
    // b-value was not among the `only` list, so it stays unbound.
    let err = run("(import (only (loadtest b) read-b)) b-value").unwrap_err();
    assert!(is_undefined(&err, "b-value"));
}

#[test]
fn only_unknown_name_errors() {
    let err = run("(import (only (loadtest b) nonexistent))").unwrap_err();
    assert!(matches!(err, EvalError::MalformedForm { .. }));
}

#[test]
fn except_omits_listed_names() {
    let v = run("(import (except (loadtest b) bump-b! read-b)) b-value").unwrap();
    assert!(equal(&v, &Value::Int(42)));
    // read-b was excepted out.
    let err = run("(import (except (loadtest b) read-b)) (read-b)").unwrap_err();
    assert!(is_undefined(&err, "read-b"));
}

#[test]
fn rename_maps_old_name_to_new() {
    let v = run("(import (rename (loadtest b) (read-b get-b))) (get-b)").unwrap();
    assert!(equal(&v, &Value::Int(42)));
    // The original name is gone after rename.
    let err = run("(import (rename (loadtest b) (read-b get-b))) (read-b)").unwrap_err();
    assert!(is_undefined(&err, "read-b"));
}

#[test]
fn prefix_prepends_to_every_name() {
    let v = run("(import (prefix (loadtest b) b:)) (b:read-b)").unwrap();
    assert!(equal(&v, &Value::Int(42)));
}

#[test]
fn qualifiers_nest() {
    // prefix applied to the result of only.
    let v = run("(import (prefix (only (loadtest b) read-b) my-)) (my-read-b)").unwrap();
    assert!(equal(&v, &Value::Int(42)));
}

#[test]
fn only_on_builtin_library() {
    let v = run("(import (only (scheme base) car)) (car '(1 2))").unwrap();
    assert!(equal(&v, &Value::Int(1)));
}

#[test]
fn rename_on_builtin_library() {
    let v = run("(import (rename (scheme base) (car head))) (head '(7 8))").unwrap();
    assert!(equal(&v, &Value::Int(7)));
}
