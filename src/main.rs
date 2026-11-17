//! nscheme command-line frontend.
//!
//! This is the only binary in the project; everything else is a
//! library (see [`lib.rs`](../lib.rs)). The frontend is deliberately
//! thin: parse the arguments, install the base library into a fresh
//! [`Env`], and then either drive the REPL or evaluate the source
//! the user provided. No language logic lives here.
//!
//! ## Usage
//!
//!   nscheme            — start the interactive REPL
//!   nscheme FILE       — evaluate FILE and exit (no REPL)
//!   nscheme -i FILE... — evaluate FILE(s), then stay in the REPL
//!   nscheme -e EXPR    — evaluate EXPR and print the result, then exit
//!
//! ## Read alongside
//!
//! - [`crate::lib`] — module map of the library.
//! - The REPL loop in [`run_repl`] uses [`rustyline`] for the line
//!   editor (history, multi-line prompts when parens aren't
//!   balanced).

use std::path::Path;
use std::process::ExitCode;

use nscheme::builtins::install_base;
use nscheme::env::{Env, EnvRef};
use nscheme::eval::{EvalError, eval_source};
use nscheme::lex::{LexError, tokenize};
use nscheme::value::Value;

use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = match build_base_env() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("nscheme: failed to initialize base library: {e}");
            return ExitCode::from(2);
        }
    };

    match args.as_slice() {
        [] => run_repl(&env),
        [flag, expr] if flag == "-e" => run_expr(&env, expr),
        [flag] if flag == "-h" || flag == "--help" => {
            print_usage();
            ExitCode::SUCCESS
        }
        [flag, rest @ ..] if flag == "-i" || flag == "--interactive" => {
            run_files_then_repl(&env, rest)
        }
        [path] => run_file(&env, Path::new(path)),
        _ => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn build_base_env() -> Result<EnvRef, EvalError> {
    let env = Env::new_global();
    install_base(&env)?;
    Ok(env)
}

fn print_usage() {
    eprintln!("nscheme {} — R7RS Scheme interpreter", nscheme::VERSION);
    eprintln!();
    eprintln!("Usage:");
    eprintln!("    nscheme              start the interactive REPL");
    eprintln!("    nscheme FILE         evaluate FILE and exit");
    eprintln!("    nscheme -i FILE...   evaluate FILE(s), then stay in the REPL");
    eprintln!("    nscheme -e EXPR      evaluate EXPR, print result, exit");
    eprintln!("    nscheme -h, --help   show this message");
}

/// Print an evaluation error to stderr, with its source location and
/// call trace (bead nscheme-tn3). The formatting lives in the library so
/// the browser wrapper reports errors the same way.
fn report_error(source: &str, e: &EvalError) {
    eprintln!("{}", nscheme::diagnostic::format_error(source, e));
}

/// The process status for a Scheme `(exit code)`. Like a Unix exit
/// status, only the low 8 bits survive.
fn exit_status(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code.rem_euclid(256)).unwrap_or(1))
}

fn run_expr(env: &EnvRef, source: &str) -> ExitCode {
    match eval_source(source, env.clone()) {
        Ok(v) => {
            print_value(&v);
            ExitCode::SUCCESS
        }
        Err(EvalError::Exit(code)) => exit_status(code),
        Err(e) => {
            report_error(source, &e);
            ExitCode::from(1)
        }
    }
}

fn run_file(env: &EnvRef, path: &Path) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("nscheme: cannot read {}: {e}", path.display());
            return ExitCode::from(2);
        }
    };
    match eval_source(&source, env.clone()) {
        Ok(_) => ExitCode::SUCCESS,
        Err(EvalError::Exit(code)) => exit_status(code),
        Err(e) => {
            report_error(&source, &e);
            ExitCode::from(1)
        }
    }
}

/// `nscheme -i FILE...` — evaluate each FILE into `env` (so its
/// definitions stay live), then hand that same env to the REPL. A file
/// that fails to read or evaluate is reported and skipped; the REPL
/// still starts so the session isn't lost. A file that calls `exit`
/// ends the process without starting the REPL. With no files, this is
/// just the plain REPL.
fn run_files_then_repl(env: &EnvRef, paths: &[String]) -> ExitCode {
    for path in paths {
        let p = Path::new(path);
        match std::fs::read_to_string(p) {
            Ok(source) => match eval_source(&source, env.clone()) {
                Ok(_) => {}
                Err(EvalError::Exit(code)) => return exit_status(code),
                Err(e) => eprintln!("error loading {}: {e}", p.display()),
            },
            Err(e) => eprintln!("nscheme: cannot read {}: {e}", p.display()),
        }
    }
    run_repl(env)
}

fn run_repl(env: &EnvRef) -> ExitCode {
    println!(
        "nscheme {} — R7RS Scheme interpreter. Type (exit) or press Ctrl-D to quit.",
        nscheme::VERSION
    );
    let mut rl = match rustyline::Editor::<(), DefaultHistory>::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("nscheme: failed to start line editor: {e}");
            return ExitCode::from(2);
        }
    };
    // Best-effort history file in the user's home directory.
    let history_path = history_path();
    if let Some(p) = &history_path {
        let _ = rl.load_history(p);
    }

    let mut status = ExitCode::SUCCESS;
    loop {
        let mut buffer = String::new();
        // Read a complete datum: keep prompting until parens balance and
        // the input parses.
        let result = read_complete_datum(&mut rl, &mut buffer);
        match result {
            ReadOutcome::Quit => break,
            ReadOutcome::Empty => {}
            ReadOutcome::Input(source) => match eval_source(&source, env.clone()) {
                Ok(Value::Unspecified) => {}
                Ok(v) => print_value(&v),
                // `(exit)` anywhere in the input ends the session (and
                // still saves the history).
                Err(EvalError::Exit(code)) => {
                    status = exit_status(code);
                    break;
                }
                Err(e) => report_error(&source, &e),
            },
        }
    }

    if let Some(p) = &history_path {
        let _ = rl.save_history(p);
    }
    status
}

/// Print a value using R7RS `write` semantics: strings get quoted,
/// characters use `#\name`, etc. The empty unspecified value is silent
/// (handled by the caller).
fn print_value(v: &Value) {
    println!("{v:?}");
}

enum ReadOutcome {
    Input(String),
    Empty,
    Quit,
}

fn read_complete_datum(
    rl: &mut rustyline::Editor<(), DefaultHistory>,
    buffer: &mut String,
) -> ReadOutcome {
    buffer.clear();
    loop {
        let prompt = if buffer.is_empty() { "> " } else { "… " };
        let line = match rl.readline(prompt) {
            Ok(l) => l,
            Err(ReadlineError::Eof | ReadlineError::Interrupted) => {
                return ReadOutcome::Quit;
            }
            Err(e) => {
                eprintln!("readline error: {e}");
                return ReadOutcome::Quit;
            }
        };
        // `,quit` is a REPL command, not Scheme. (`(exit)` is handled
        // by evaluating it, so `(exit 3)` sets the status too.)
        let trimmed = line.trim();
        if buffer.is_empty() && trimmed == ",quit" {
            return ReadOutcome::Quit;
        }
        if buffer.is_empty() && trimmed.is_empty() {
            return ReadOutcome::Empty;
        }
        let _ = rl.add_history_entry(line.as_str());
        buffer.push_str(&line);
        buffer.push('\n');

        // Try to tokenize: if it succeeds and the parens balance, we're
        // done. If tokenization fails with "unterminated…", keep
        // reading. Otherwise surface the error now.
        match tokenize(buffer) {
            Ok(tokens) => {
                if parens_balanced(&tokens) {
                    return ReadOutcome::Input(std::mem::take(buffer));
                }
            }
            Err(
                LexError::UnterminatedString { .. }
                | LexError::UnterminatedBlockComment { .. }
                | LexError::UnterminatedQuotedIdentifier { .. },
            ) => {
                // Need more input — loop again with the same buffer.
            }
            Err(e) => {
                eprintln!("lex error: {e}");
                return ReadOutcome::Empty;
            }
        }
    }
}

fn parens_balanced(tokens: &[nscheme::lex::Token]) -> bool {
    use nscheme::lex::TokenKind;
    let mut depth: i64 = 0;
    for t in tokens {
        match t.kind {
            TokenKind::LParen | TokenKind::VectorStart | TokenKind::BytevectorStart => depth += 1,
            TokenKind::RParen => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            // Unbalanced — let the parser surface a proper error.
            return true;
        }
    }
    depth == 0
}

fn history_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| {
        let mut p = std::path::PathBuf::from(home);
        p.push(".nscheme_history");
        p
    })
}
