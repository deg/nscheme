//! Integration with chibi-scheme's r7rs-tests.scm conformance corpus.
//!
//! See `tests/r7rs-corpus/README.md` for background. The corpus
//! exercises corners of R7RS-small that nscheme v1 does not yet
//! implement (complex numbers, `eval`, `case-lambda`, `(scheme time)`,
//! `(scheme process-context)`, full `read`, …) so this test does NOT
//! assert "all pass." It records a baseline pass/fail count and
//! prints a short summary; the floor that must be cleared is the
//! `BASELINE_MIN_PASSES` constant below.
//!
//! Run with output:
//!
//! ```text
//! cargo test --test r7rs_chibi -- --nocapture
//! ```

use std::path::PathBuf;
use std::time::Instant;

use nscheme::builtins::install_base;
use nscheme::env::{Env, EnvRef};
use nscheme::eval::{eval, eval_source};
use nscheme::parse::parse_program;
use nscheme::value::{Symbol, Value};

/// Resolve a path under `tests/r7rs-corpus/`.
fn corpus_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("r7rs-corpus");
    p.push(name);
    p
}

/// Read a thread-local Scheme integer from the env.
fn read_int(env: &EnvRef, name: &str) -> Option<i64> {
    match env.lookup(&Symbol::intern(name))? {
        Value::Int(n) => Some(n),
        _ => None,
    }
}

#[test]
fn chibi_r7rs_tests_baseline() {
    // The chibi corpus pushes some primitives (notably `equal?` /
    // `write` on long shared lists) through deep host-level
    // recursion. cargo test threads default to a 2 MiB stack on
    // macOS — too small for some of these. We re-run the body on
    // a thread with a generous explicit stack so cargo test
    // matches `cargo run`'s behavior.
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(chibi_baseline_inner)
        .expect("spawn")
        .join()
        .expect("worker thread");
}

fn chibi_baseline_inner() {
    // Pass-count floor. As nscheme grows toward full R7RS-small the
    // baseline should be ratcheted up. Lowering it requires
    // justification (regression triage in the failing bead).
    const BASELINE_MIN_PASSES: i64 = 1150;

    let env = Env::new_global();
    install_base(&env).expect("install_base");

    // 1) Load the (chibi test) shim so the corpus's import resolves.
    let shim = std::fs::read_to_string(corpus_path("chibi-test-shim.scm")).expect("read shim");
    eval_source(&shim, env.clone()).expect("load shim");

    // 2) Parse the corpus into individual top-level datums and
    //    evaluate each one under a Rust-level catch, so a single
    //    unsupported form (e.g. one referencing `eval` or
    //    `make-rectangular`) doesn't abort the entire run.
    let corpus = std::fs::read_to_string(corpus_path("chibi-r7rs-tests.scm")).expect("read corpus");
    let datums = parse_program(&corpus).expect("parse corpus");

    // Hardcoded skip-list for top-level forms known to hang or run
    // unreasonably slowly under nscheme v1's evaluator. Each entry
    // documents what's there. Lifting an entry off this list requires
    // either fixing the underlying interpreter gap or proving the
    // form now terminates.
    // No forms are statically skipped at this point.
    let skip: std::collections::HashSet<usize> = std::collections::HashSet::new();

    let total_datums = datums.len();
    let start = Instant::now();
    let mut top_level_errors = 0_usize;
    let mut first_errors: Vec<String> = Vec::new();
    for (i, d) in datums.into_iter().enumerate() {
        if skip.contains(&i) {
            continue;
        }
        let t = Instant::now();
        if let Err(e) = eval(d, env.clone()) {
            top_level_errors += 1;
            if first_errors.len() < 5 {
                first_errors.push(format!("[#{i}] {e}"));
            }
        }
        let dt = t.elapsed();
        if dt > std::time::Duration::from_millis(50) {
            eprintln!("  slow datum #{i}: {dt:.2?}");
        }
    }
    let elapsed = start.elapsed();

    // 3) Read the shim's counters.
    let passes = read_int(&env, "$passes").unwrap_or(0);
    let fails = read_int(&env, "$fails").unwrap_or(0);

    println!();
    println!("=== chibi r7rs-tests.scm baseline ===");
    println!("  total datums:     {total_datums}");
    println!("  evaluated:        {}", total_datums - skip.len());
    println!("  passes:           {passes}");
    println!("  failures:         {fails}");
    println!("  top-level errors: {top_level_errors}");
    println!("  duration:         {elapsed:.2?}");
    if !first_errors.is_empty() {
        println!();
        println!("First top-level errors (truncated to 5):");
        for (i, e) in first_errors.iter().enumerate() {
            println!("  {}. {e}", i + 1);
        }
    }
    println!();

    assert!(
        passes >= BASELINE_MIN_PASSES,
        "Chibi conformance regressed: only {passes} passes (baseline {BASELINE_MIN_PASSES}). \
         Triage which tests stopped passing — see bead nscheme-i0p."
    );
}
