//! What the browser build relies on, tested natively (bead nscheme-yd0.1,
//! ADR 0012): the compiled-in copy of `lib/`, `exit` as an uncatchable
//! result instead of a process exit, the console sink, and the clock.
//! The wasm build itself is covered by `web/smoke.mjs`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::io::{ConsoleStream, set_console_sink};
use nscheme::library::set_search_path;
use nscheme::value::Value;

// ---------------------------------------------------------------------
// Compiled-in libraries
// ---------------------------------------------------------------------

/// The browser has no filesystem, so every library must load from the
/// table `build.rs` compiles in. An empty search path turns off disk
/// lookup to force that. Walking `lib/` covers a newly added library
/// automatically. Caught a real bug: the table was sorted in path
/// order but binary-searched by string, so `(srfi 146)` and the
/// libraries that import it were not found.
#[test]
fn every_library_loads_from_the_compiled_in_copy() {
    let mut names = Vec::new();
    library_names(
        &PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/lib")),
        &mut Vec::new(),
        &mut names,
    );
    assert!(names.len() > 40, "found only {} libraries", names.len());
    set_search_path(vec![]);
    for name in names {
        if let Err(e) = run(&format!("(import {name})")) {
            panic!("(import {name}) from the compiled-in copy: {e}");
        }
    }
}

/// `cond-expand`'s `(library …)` test has its own lookup
/// (`library_findable`). If it missed the compiled-in table, a browser
/// program would take the "library missing" branch while `import` of
/// the same library worked.
#[test]
fn cond_expand_sees_compiled_in_libraries() {
    set_search_path(vec![]);
    let v = run("(cond-expand ((library (scheme sort)) 'yes) (else 'no))").unwrap();
    assert_eq!(format!("{v:?}"), "yes");
}

/// The compiled-in copy is a last resort: a library on disk must win, or
/// editing `lib/` (and every test fixture that shadows a library) would
/// silently stop taking effect until a rebuild.
#[test]
fn a_library_on_disk_wins_over_the_compiled_in_copy() {
    // The fixture dir has its own (scheme list) whose `iota` is a marker.
    let fixtures = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/shadow-lib"
    ));
    set_search_path(vec![fixtures]);
    let v = run("(import (scheme list)) (iota 3)").unwrap();
    assert_eq!(format!("{v:?}"), "from-disk");
}

// ---------------------------------------------------------------------
// exit
// ---------------------------------------------------------------------

/// `exit` now hands its status to the host instead of ending the
/// process. This pins the R7RS status mapping (absent or `#t` is 0, `#f`
/// or a non-integer is 1). Before this change, a test calling `exit`
/// would have killed the test runner.
#[test]
fn exit_returns_its_status_to_the_host() {
    for (src, code) in [
        ("(exit)", 0),
        ("(exit #t)", 0),
        ("(exit #f)", 1),
        ("(exit 42)", 42),
        ("(emergency-exit 9)", 9),
        ("(exit 'odd)", 1),
    ] {
        match run(src) {
            Err(EvalError::Exit(c)) => assert_eq!(c, code, "{src}"),
            other => panic!("{src}: expected Exit({code}), got {other:?}"),
        }
    }
}

/// Every other primitive error is turned into a catchable `raise`, so
/// `exit` must be intercepted before that conversion, and must also
/// survive Rust code that runs Scheme callbacks itself. Caught a real
/// leak: `force` wrapped the exit of a promise body as an ordinary
/// error, which `guard` then caught.
#[test]
fn exit_cannot_be_caught() {
    for src in [
        "(guard (e (#t 'caught)) (exit 4))",
        "(with-exception-handler (lambda (e) 'caught) (lambda () (exit 4)))",
        "(call/cc (lambda (k) (with-exception-handler k (lambda () (exit 4)))))",
        // From inside callbacks that primitives and special forms run
        // themselves, bare and under a guard.
        "(map (lambda (x) (exit 4)) '(1))",
        "(guard (e (#t 'caught)) (for-each (lambda (x) (exit 4)) '(1)))",
        "(guard (e (#t 'caught)) (vector-map (lambda (x) (exit 4)) #(1)))",
        "(guard (e (#t 'caught)) (string-for-each (lambda (c) (exit 4)) \"a\"))",
        "(guard (e (#t 'caught)) (apply exit '(4)))",
        "(guard (e (#t 'caught)) (dynamic-wind (lambda () #f) (lambda () (exit 4)) (lambda () #f)))",
        "(guard (e (#t 'caught)) (call-with-values (lambda () (exit 4)) list))",
        "(guard (e (#t 'caught)) (eval '(exit 4) (interaction-environment)))",
        "(guard (e (#t 'caught)) (with-exception-handler (lambda (e) (exit 4)) (lambda () (raise 'x))))",
        "(guard (e (#t 'caught)) (force (delay (exit 4))))",
    ] {
        assert!(
            matches!(run(src), Err(EvalError::Exit(4))),
            "{src} caught exit"
        );
    }
}

/// Nothing after the `exit` may run, neither the rest of the procedure
/// nor later top-level forms. Otherwise a browser program would carry on
/// printing after it had asked to stop.
#[test]
fn exit_stops_the_program_where_it_is() {
    let out = Rc::new(RefCell::new(String::new()));
    let sink = Rc::clone(&out);
    set_console_sink(Some(Box::new(move |_, s| sink.borrow_mut().push_str(s))));
    let r =
        run("(define (f) (display \"in\") (exit 2) (display \"after\")) (f) (display \"later\")");
    set_console_sink(None);
    assert!(matches!(r, Err(EvalError::Exit(2))));
    assert_eq!(*out.borrow(), "in");
}

/// The loader evaluates a library file with a nested `eval_source`. The
/// exit must pass through it, not turn into an "error loading library".
#[test]
fn exit_from_a_loaded_library_reaches_the_host() {
    // tests/fixtures/shadow-lib/quits.sld calls (exit 6) in its body.
    set_search_path(vec![PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/shadow-lib"
    ))]);
    assert!(matches!(run("(import (quits))"), Err(EvalError::Exit(6))));
}

/// The CLI used to get its status straight from `process::exit`. Now
/// `main.rs` must map `EvalError::Exit` itself. If it didn't, `-e
/// '(exit 3)'` would print an error and exit 1. The status keeps only
/// its low 8 bits, as `process::exit` did on Unix.
#[test]
fn cli_exits_with_the_programs_status() {
    assert_eq!(cli(&["-e", "(exit 3)"], ""), 3);
    assert_eq!(cli(&["-e", "(guard (e (#t 0)) (exit 4))"], ""), 4);
    assert_eq!(cli(&["-e", "(exit 300)"], ""), 300 % 256);
    assert_eq!(cli(&["-e", "(car 1)"], ""), 1);
}

/// The REPL banner says "Type (exit) … to quit". Without the REPL
/// handling `EvalError::Exit`, `(exit 7)` would be reported as an error
/// and the REPL would read on to end-of-input and exit 0.
#[test]
fn cli_repl_quits_on_exit() {
    assert_eq!(cli(&[], "(display 1)\n(exit 7)\n(display 2)\n"), 7);
    assert_eq!(cli(&[], "(begin (display 1) (exit))\n(car 1)\n"), 0);
    assert_eq!(cli(&[], ",quit\n"), 0);
}

// ---------------------------------------------------------------------
// Console sink and clock
// ---------------------------------------------------------------------

/// The browser shows the program's output only through the sink. This
/// checks that both console streams reach it, in order and labelled,
/// and that output redirected to a string port does not.
#[test]
fn console_sink_receives_both_streams_in_order() {
    let got = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&got);
    set_console_sink(Some(Box::new(move |stream, s| {
        sink.borrow_mut().push((stream, s.to_string()));
    })));
    run("(display \"out\") (display \"err\" (current-error-port)) \
         (with-output-to-string (lambda () (display \"captured\")))")
    .unwrap();
    set_console_sink(None);
    assert_eq!(
        *got.borrow(),
        vec![
            (ConsoleStream::Out, "out".to_string()),
            (ConsoleStream::Err, "err".to_string()),
        ]
    );
}

/// `current-second` moved from `std::time` (which panics in a browser)
/// to `web-time`. Natively the two must agree on the epoch and units,
/// so a wrong conversion (milliseconds, or the wrong epoch) fails here.
/// The JS clock itself is checked by `web/smoke.mjs`.
#[test]
fn clock_reads_wall_time() {
    let Value::Float(t) = run("(current-second)").unwrap() else {
        panic!("current-second is not a float");
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    assert!((t - now).abs() < 60.0, "{t} vs {now}");
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Evaluate `source` in a fresh environment with the base library.
fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

/// Every library name under `lib/`: `lib/srfi/146/hash.sld` becomes
/// `(srfi 146 hash)`.
fn library_names(dir: &Path, prefix: &mut Vec<String>, out: &mut Vec<String>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read lib dir")
        .map(|e| e.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            prefix.push(stem);
            library_names(&path, prefix, out);
            prefix.pop();
        } else if path.extension().is_some_and(|e| e == "sld") {
            prefix.push(stem);
            out.push(format!("({})", prefix.join(" ")));
            prefix.pop();
        }
    }
}

/// Run the CLI binary with `args`, feeding `stdin`; return its status code.
fn cli(args: &[&str], stdin: &str) -> i32 {
    use std::io::Write as _;
    let mut child = Command::new(env!("CARGO_BIN_EXE_nscheme"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn nscheme");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait().expect("wait").code().expect("exit code")
}
