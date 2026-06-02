//! Filesystem library-loader tests (bead nscheme-9q5).
//!
//! These point `NSCHEME_LIB_PATH` at a committed fixture directory and
//! import libraries that live only on disk. Every loader test uses the
//! *same* path value, so the process-global env write is benign even
//! though Rust runs tests in parallel threads (the library registry
//! itself is thread-local, so each test thread loads independently).

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::library::set_search_path;
use nscheme::value::{Value, equal};

/// Absolute path to the committed fixture lib directory.
fn fixture_lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/tests/fixtures/lib", env!("CARGO_MANIFEST_DIR")))
}

/// Point the loader at the fixture dir, then run `source`. The search
/// path override is thread-local, so parallel tests don't interfere.
fn run_with_fixtures(source: &str) -> Result<Value, EvalError> {
    set_search_path(vec![fixture_lib_dir()]);
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

#[test]
fn loads_library_from_disk() {
    // (loadtest b) lives only on disk; importing it must find, load,
    // and register it.
    let v = run_with_fixtures("(import (loadtest b)) (read-b)").unwrap();
    assert!(equal(&v, &Value::Int(42)));
}

#[test]
fn recursive_load_through_dependent_library() {
    // (loadtest a) imports (scheme base) and (loadtest b). Loading a
    // must recurse to load b, and a-plus-b must read b's exported
    // value — exercising the loader, registry caching, and q1c shared
    // cells in one shot.
    let v = run_with_fixtures("(import (loadtest a)) (a-plus-b 8)").unwrap();
    assert!(equal(&v, &Value::Int(50)));
}

#[test]
fn loaded_library_mutable_state_is_shared() {
    // A mutator exported from an on-disk library, run after import,
    // must be visible to the importer reading the same binding.
    let src = "
        (import (loadtest b))
        (bump-b!)
        (bump-b!)
        b-value
    ";
    assert!(equal(&run_with_fixtures(src).unwrap(), &Value::Int(44)));
}

#[test]
fn unknown_library_not_on_path_errors() {
    // No file for (loadtest nope): the loader finds nothing and the
    // import reports an unknown library rather than silently passing.
    let err = run_with_fixtures("(import (loadtest nope))").unwrap_err();
    assert!(matches!(err, EvalError::MalformedForm { .. }));
}
