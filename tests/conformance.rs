//! R7RS-large conformance tests: run each library's *actual* SRFI
//! reference test suite through the (srfi 64) harness and assert zero
//! failures.
//!
//! The vendored suites under tests/r7rs-large-corpus/ are the upstream
//! reference suites with ONLY their non-portable preamble (Chicken
//! `(use …)`, R6RS `import`, relative `(load …)`) replaced by portable
//! R7RS imports — every `test`/`test-assert`/`test-equal` assertion is
//! verbatim. This is real conformance coverage (hundreds of assertions
//! per library), not the hand-mined representative subset in the
//! srfi_*_*.rs files.

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::{Env, EnvRef};
use nscheme::eval::eval_source;
use nscheme::library::set_search_path;
use nscheme::value::Value;

fn lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/lib", env!("CARGO_MANIFEST_DIR")))
}

fn int_of(env: &EnvRef, expr: &str) -> i64 {
    match eval_source(expr, env.clone()) {
        Ok(Value::Int(n)) => n,
        other => panic!("expected integer from `{expr}`, got {other:?}"),
    }
}

/// Run the named vendored reference suite; return (pass, fail) counts.
fn run_suite(file: &str) -> (i64, i64) {
    set_search_path(vec![lib_dir()]);
    let path = PathBuf::from(format!(
        "{}/tests/r7rs-large-corpus/{file}",
        env!("CARGO_MANIFEST_DIR")
    ));
    let src = std::fs::read_to_string(&path).expect("read suite");
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&src, env.clone()).unwrap_or_else(|e| panic!("suite {file} errored: {e:?}"));
    let pass = int_of(&env, "(test-runner-pass-count (test-runner-get))");
    let fail = int_of(&env, "(test-runner-fail-count (test-runner-get))");
    (pass, fail)
}

/// Assert a suite ran a positive number of assertions with no failures.
fn assert_suite_clean(file: &str, min_pass: i64) {
    let (pass, fail) = run_suite(file);
    assert_eq!(fail, 0, "{file}: {fail} reference-suite assertions FAILED");
    assert!(
        pass >= min_pass,
        "{file}: only {pass} assertions ran (expected >= {min_pass})"
    );
}

#[test]
fn srfi_128_comparator_reference_suite() {
    // SRFI 128 reference suite (John Cowan). ~144 assertions.
    assert_suite_clean("srfi-128-test.scm", 140);
}

#[test]
fn srfi_151_bitwise_reference_suite() {
    // SRFI 151 reference suite (John Cowan). ~246 assertions.
    assert_suite_clean("srfi-151-test.scm", 240);
}

#[test]
fn srfi_113_set_reference_suite() {
    // SRFI 113 reference suite (John Cowan). ~282 assertions.
    assert_suite_clean("srfi-113-test.scm", 280);
}

#[test]
fn srfi_143_fixnum_reference_suite() {
    // SRFI 143 reference suite (John Cowan). ~141 assertions.
    assert_suite_clean("srfi-143-test.scm", 140);
}

#[test]
fn srfi_117_list_queue_reference_suite() {
    // SRFI 117 reference suite (Alex Shinn). ~34 assertions.
    assert_suite_clean("srfi-117-test.scm", 30);
}

#[test]
fn srfi_127_lseq_reference_suite() {
    // SRFI 127 reference suite (John Cowan). ~109 assertions.
    assert_suite_clean("srfi-127-test.scm", 100);
}

#[test]
fn srfi_146_mapping_reference_suite() {
    // SRFI 146 reference suite (Marc Nieper-Wißkirchen). ~97 assertions,
    // including the delete operations that the hygiene fix unblocked.
    assert_suite_clean("srfi-146-test.scm", 90);
}

#[test]
fn srfi_125_hash_table_reference_suite() {
    // SRFI 125 reference suite (William D Clinger). It keeps its own
    // pass/fail state in `ultimate-exit-status` (0 = all passed) rather
    // than via (srfi 64), so we read that after running it.
    set_search_path(vec![lib_dir()]);
    let path = PathBuf::from(format!(
        "{}/tests/r7rs-large-corpus/srfi-125-test.scm",
        env!("CARGO_MANIFEST_DIR")
    ));
    let src = std::fs::read_to_string(&path).expect("read suite");
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&src, env.clone()).expect("SRFI 125 reference suite ran");
    assert_eq!(
        int_of(&env, "ultimate-exit-status"),
        0,
        "SRFI 125 reference suite had a failed assertion"
    );
}

#[test]
fn srfi_115_regex_reference_suite() {
    // SRFI 115 reference suite (Alex Shinn). ~66 assertions.
    assert_suite_clean("srfi-115-test.scm", 60);
}

#[test]
fn srfi_14_charset_reference_suite() {
    // SRFI 14 reference suite (Olin Shivers). Its own `test` macro
    // raises on the first failed form, so running to completion without
    // error means every assertion passed. ~140 assertions.
    set_search_path(vec![lib_dir()]);
    let path = PathBuf::from(format!(
        "{}/tests/r7rs-large-corpus/srfi-14-test.scm",
        env!("CARGO_MANIFEST_DIR")
    ));
    let src = std::fs::read_to_string(&path).expect("read suite");
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&src, env).expect("SRFI 14 reference suite must run with no failed assertion");
}
