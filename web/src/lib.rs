//! nscheme in a web browser: the WebAssembly wrapper (beads nscheme-yd0.1,
//! nscheme-yd0.2.2).
//!
//! A thin layer over the `nscheme` library, compiled to
//! `wasm32-unknown-unknown` and exposed to JavaScript by `wasm-bindgen`.
//! JavaScript creates an [`Interpreter`], calls [`Interpreter::run`] or
//! [`Interpreter::run_streaming`] with program text, and gets back a
//! [`RunResult`]: the value of its last expression, its error report, or
//! its `exit` status, and (from `run`) everything it printed.
//! `web/build.sh` builds it. The demo page `web/www/index.html` runs it
//! in a Web Worker (`web/www/worker.js`) so a long run never freezes the
//! page and Stop can end it. The page is a REPL: each entry is
//! evaluated in the same `Interpreter`, until Reset makes a new one and
//! calls [`forget_user_libraries`]. See ADRs 0012 and 0013.
//!
//! The library does the browser-specific work itself (a compiled-in copy
//! of `lib/`, a wasm-safe clock, `exit` as a result instead of a process
//! exit). This crate only collects console output and packages results.
//!
//! **Rust note**: this is a separate crate, not a feature of `nscheme`,
//! because the glue `#[wasm_bindgen]` generates uses `unsafe`, which
//! the main crate forbids.
//!
//! What a browser can't do yet: read from stdin (`read-line` on the
//! console sees end-of-file), or touch files (file ports fail with a
//! `file-error?` condition). Both are later beads of the epic
//! nscheme-yd0.

use std::cell::RefCell;
use std::rc::Rc;

use nscheme::env::{Env, EnvRef};
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::Value;
use wasm_bindgen::prelude::*;

/// One Scheme top-level environment. Definitions made by one
/// [`run`](Interpreter::run) stay visible to the next, as in a REPL;
/// a new `Interpreter` starts with none. Libraries are not per
/// interpreter: the library registry is per thread, so a library
/// loaded by `import` (or defined with `define-library`) stays loaded
/// for every later `Interpreter` on the page.
#[wasm_bindgen]
pub struct Interpreter {
    env: EnvRef,
}

#[wasm_bindgen]
impl Interpreter {
    /// A fresh environment with the whole base library installed.
    ///
    /// # Errors
    ///
    /// If the base library fails to install, which would be a bug.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Interpreter, JsError> {
        // A Rust panic otherwise reaches JavaScript as a bare
        // "unreachable executed"; this logs the panic message to the
        // browser console first.
        console_error_panic_hook::set_once();
        let env = Env::new_global();
        nscheme::builtins::install_base(&env).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { env })
    }

    /// Evaluate every top-level form in `source`, collecting what the
    /// program writes to the console into [`RunResult::output`].
    pub fn run(&self, source: &str) -> RunResult {
        let output = Rc::new(RefCell::new(String::new()));
        let collected = Rc::clone(&output);
        let mut result = self.evaluate(source, move |text| {
            collected.borrow_mut().push_str(text);
        });
        result.output = output.take();
        result
    }

    /// Evaluate `source`, calling `on_output(text)` for each write the
    /// program makes to the console, as it makes it, rather than
    /// collecting the output. [`RunResult::output`] is then empty.
    ///
    /// Batching is the caller's job: the demo page's Worker (`worker.js`)
    /// keeps the pending text on the JavaScript side, so it can still
    /// send the last of it if the run traps (a trapped wasm instance can
    /// run nothing more). A call into JS per write is cheap; it is
    /// `postMessage` per write that would flood the page.
    #[wasm_bindgen(js_name = runStreaming)]
    pub fn run_streaming(&self, source: &str, on_output: &js_sys::Function) -> RunResult {
        let on_output = on_output.clone();
        self.evaluate(source, move |text| {
            // A throwing callback can't stop the run from here; the
            // program carries on and that text is lost.
            let _ = on_output.call1(&JsValue::NULL, &JsValue::from_str(text));
        })
    }
}

impl Interpreter {
    /// The run itself: evaluate `source` with console output (stdout and
    /// stderr interleaved, in the order written) sent to `write`, and
    /// package the outcome. `output` is left empty for the caller.
    fn evaluate(&self, source: &str, write: impl FnMut(&str) + 'static) -> RunResult {
        // State the library keeps per thread outlives a run: an `exit`
        // (or an error that skips the raise machinery) from inside a
        // `parameterize` leaves the current ports rebound, and a
        // backtrace may be left over.
        nscheme::io::reset_current_ports();
        let _ = nscheme::eval::take_backtrace();

        let write = RefCell::new(write);
        nscheme::io::set_console_sink(Some(Box::new(move |_stream, text| {
            (write.borrow_mut())(text);
        })));
        let outcome = eval_source(source, self.env.clone());
        nscheme::io::set_console_sink(None);

        let empty = RunResult::new(String::new());
        match outcome {
            Ok(Value::Unspecified) => empty,
            Ok(v) => RunResult {
                value: Some(format!("{v:?}")),
                ..empty
            },
            Err(EvalError::Exit(code)) => RunResult {
                exit_code: Some(code),
                ..empty
            },
            Err(e) => RunResult {
                error: Some(nscheme::diagnostic::format_error(source, &e)),
                ..empty
            },
        }
    }
}

/// Forget every library except the standard ones, so the next `import`
/// of a user's library (a saved file, or an inline `define-library`)
/// reloads it. The playground's Reset calls this along with making a new
/// [`Interpreter`]. See `nscheme::library::forget_user_libraries`.
#[wasm_bindgen(js_name = forgetUserLibraries)]
pub fn forget_user_libraries() {
    nscheme::library::forget_user_libraries();
}

/// What one [`Interpreter::run`] produced. At most one of `value`,
/// `error` and `exit_code` is set.
#[wasm_bindgen]
#[derive(Debug)]
pub struct RunResult {
    output: String,
    value: Option<String>,
    error: Option<String>,
    exit_code: Option<i32>,
}

impl RunResult {
    fn new(output: String) -> Self {
        Self {
            output,
            value: None,
            error: None,
            exit_code: None,
        }
    }
}

#[wasm_bindgen]
impl RunResult {
    /// Everything written to the console ports, stdout and stderr
    /// interleaved.
    #[wasm_bindgen(getter)]
    pub fn output(&self) -> String {
        self.output.clone()
    }

    /// The last expression's value in `write` form, unless it was
    /// unspecified or the run failed or exited.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> Option<String> {
        self.value.clone()
    }

    /// The error report (message, `line:col` and caret, call trace),
    /// as the CLI prints it, if the run failed.
    #[wasm_bindgen(getter)]
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    /// The status passed to `exit` / `emergency-exit`, if the program
    /// called one.
    #[wasm_bindgen(getter, js_name = exitCode)]
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
}

// The wrapper's logic runs natively too, so `cargo test` covers it; the
// browser itself is covered by `web/smoke.mjs`.
#[cfg(test)]
mod tests {
    use super::*;

    /// The page's two outputs come from different places: the console
    /// sink and the last expression's value. If either were missing,
    /// `(display …)` or a bare expression would show nothing.
    #[test]
    fn collects_console_output_and_the_last_value() {
        let r = interp().run(r#"(display "hello") (newline) (+ 1 2)"#);
        assert_eq!(r.output(), "hello\n");
        assert_eq!(r.value().as_deref(), Some("3"));
        assert_eq!(r.error(), None);
    }

    /// Writes to `current-error-port` would otherwise go to the
    /// browser's invisible stderr and be lost.
    #[test]
    fn stderr_joins_the_transcript() {
        let r = interp().run(r#"(display "a") (display "b" (current-error-port))"#);
        assert_eq!(r.output(), "ab");
    }

    /// The browser must show the same located report as the CLI (shared
    /// via `diagnostic::format_error`), not a bare message.
    #[test]
    fn reports_errors_with_their_location() {
        let r = interp().run("(define x 1)\n(car x)");
        let err = r.error().expect("an error");
        assert!(err.contains("expected pair"), "{err}");
        assert!(err.contains("line 2:1"), "{err}");
    }

    /// In the browser, `exit` is a result. Output before it is kept,
    /// nothing after it runs, and it is not reported as an error.
    #[test]
    fn exit_ends_the_run_with_its_status() {
        let r = interp().run(r#"(display "before") (exit 3) (display "after")"#);
        assert_eq!(r.output(), "before");
        assert_eq!(r.exit_code(), Some(3));
        assert_eq!(r.error(), None);
    }

    /// One `Interpreter` is one top-level environment, which tier 2's
    /// REPL will rely on. The page makes a new one per Run.
    #[test]
    fn definitions_persist_across_runs() {
        let i = interp();
        i.run("(define (sq x) (* x x))");
        assert_eq!(i.run("(sq 7)").value().as_deref(), Some("49"));
    }

    /// The page reuses one wasm instance for every run, so a program
    /// that closes the console port must not break the runs after it.
    #[test]
    fn closing_the_console_port_does_not_break_later_runs() {
        let i = interp();
        i.run("(close-port (current-output-port))");
        let r = i.run(r#"(display "still here")"#);
        assert_eq!(r.error(), None);
        assert_eq!(r.output(), "still here");
    }

    /// Without `reset_current_ports`, an exit inside
    /// `with-output-to-string` would leave the string port current, and
    /// every later run's output would vanish (verified by removing the
    /// reset).
    #[test]
    fn an_exit_does_not_leave_output_redirected() {
        // `exit` leaves the eval loop without unwinding
        // `with-output-to-string`'s parameterize (an uncaught `raise`
        // would restore it), so its string port stays current unless
        // `run` resets it.
        let i = interp();
        let r = i.run("(with-output-to-string (lambda () (exit 1)))");
        assert_eq!(r.exit_code(), Some(1));
        assert_eq!(i.run(r#"(display "visible")"#).output(), "visible");
    }

    /// A fresh interpreter with the base library installed.
    fn interp() -> Interpreter {
        Interpreter::new().expect("base library installs")
    }
}
