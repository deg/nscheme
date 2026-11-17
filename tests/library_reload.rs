//! Forgetting user libraries (bead nscheme-yd0.2.3). The library
//! registry is per thread and outlives every environment, so in the
//! browser playground an edited library would never reload. Its Reset
//! calls `library::forget_user_libraries`, which drops every library
//! except the standard ones; those stay cached because reloading them
//! costs up to a second.

use std::path::PathBuf;
use std::rc::Rc;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::files::{MemoryFileSystem, set_file_system};
use nscheme::library::{forget_user_libraries, library_exists, set_search_path};
use nscheme::value::Value;

/// The case that motivated it: a saved library is edited. Without the
/// forget, a fresh environment's import silently gets the old version;
/// with it, the new one.
#[test]
fn an_edited_saved_library_reloads_after_the_forget() {
    let fs = in_memory();
    fs.insert("lib/mine/greet.sld", greet_library("hello"));
    assert_eq!(greeting(), r#""hello""#);
    fs.insert("lib/mine/greet.sld", greet_library("bonjour"));
    assert_eq!(
        greeting(),
        r#""hello""#,
        "the registry still has the old one"
    );
    forget_user_libraries();
    assert_eq!(greeting(), r#""bonjour""#);
}

/// A library defined inline in an entry is a user library too: after a
/// Reset, its old definition must not linger in the registry.
#[test]
fn an_inline_library_is_forgotten() {
    in_memory();
    run("(define-library (scratch pad) (export v) (import (scheme base)) (begin (define v 1)))")
        .unwrap();
    assert!(library_exists(&name(&["scratch", "pad"])));
    forget_user_libraries();
    assert!(!library_exists(&name(&["scratch", "pad"])));
}

/// The standard libraries must survive, or every Reset would pay to
/// reload them. Checked both for one served from the compiled-in table
/// (the browser's case) and for its dependency loaded along the way.
#[test]
fn standard_libraries_stay_registered() {
    in_memory();
    run("(import (scheme mapping))").unwrap();
    forget_user_libraries();
    assert!(library_exists(&name(&["scheme", "mapping"])));
    assert!(library_exists(&name(&["srfi", "146"])));
}

/// In a development checkout the standard libraries are found on disk
/// in the crate's own lib/ before the compiled-in copy; they must count
/// as standard there too.
#[test]
fn standard_libraries_found_on_disk_stay_registered() {
    set_search_path(vec![PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/lib"
    ))]);
    run("(import (scheme list))").unwrap();
    forget_user_libraries();
    assert!(library_exists(&name(&["scheme", "list"])));
}

/// A saved file that shadows a standard library's name is the user's,
/// so it is forgotten and reloads like any other.
#[test]
fn a_saved_file_shadowing_a_standard_name_is_forgotten() {
    let fs = in_memory();
    fs.insert(
        "lib/scheme/box.sld",
        "(define-library (scheme box) (export box) (import (scheme base)) \
         (begin (define (box x) 'mine)))",
    );
    run("(import (scheme box))").unwrap();
    forget_user_libraries();
    assert!(!library_exists(&name(&["scheme", "box"])));
}

/// A user library that re-registers a standard library's name (here
/// inline, after the real one was imported) is the user's from then on:
/// it must be forgotten, or Reset would leave the user's version in
/// place of the standard one.
#[test]
fn a_user_library_that_replaces_a_standard_one_is_forgotten() {
    in_memory();
    run("(import (scheme box))").unwrap();
    run(
        "(define-library (scheme box) (export box) (import (scheme base)) \
         (begin (define (box x) 'mine)))",
    )
    .unwrap();
    forget_user_libraries();
    assert!(!library_exists(&name(&["scheme", "box"])));
}

/// Forgetting one library (what the playground does when its file is
/// saved from the Files panel) reloads just that one.
#[test]
fn forgetting_one_library_reloads_only_it() {
    let fs = in_memory();
    fs.insert("lib/mine/greet.sld", greet_library("hello"));
    run("(define-library (scratch other) (export o) (import (scheme base)) (begin (define o 1)))")
        .unwrap();
    assert_eq!(greeting(), r#""hello""#);
    fs.insert("lib/mine/greet.sld", greet_library("hi"));
    assert!(nscheme::library::forget_library(&name(&["mine", "greet"])));
    assert_eq!(greeting(), r#""hi""#);
    assert!(
        library_exists(&name(&["scratch", "other"])),
        "only the one library is forgotten"
    );
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Install an in-memory filesystem and look for libraries only in its
/// `lib/` (standard libraries then come from the compiled-in table, as
/// in a browser). Returns a handle for editing files.
fn in_memory() -> Rc<MemoryFileSystem> {
    let fs = Rc::new(MemoryFileSystem::new());
    set_file_system(Some(Box::new(Rc::clone(&fs))));
    set_search_path(vec![PathBuf::from("lib")]);
    fs
}

/// A `(mine greet)` library whose `greeting` is `word`.
fn greet_library(word: &str) -> String {
    format!(
        "(define-library (mine greet) (export greeting) (import (scheme base)) \
         (begin (define greeting \"{word}\")))"
    )
}

/// Import `(mine greet)` into a fresh environment and read `greeting`.
fn greeting() -> String {
    format!("{:?}", run("(import (mine greet)) greeting").unwrap())
}

fn name(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).to_string()).collect()
}

/// Evaluate `source` in a fresh environment with the base library.
fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}
