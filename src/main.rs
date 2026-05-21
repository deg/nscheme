//! nscheme command-line frontend.
//!
//! Usage:
//!   nscheme            — start the interactive REPL
//!   nscheme FILE       — evaluate FILE and exit (no REPL)
//!   nscheme -e EXPR    — evaluate EXPR and print the result, then exit

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
    eprintln!(
        "nscheme {} — R7RS-small Scheme interpreter",
        nscheme::VERSION
    );
    eprintln!();
    eprintln!("Usage:");
    eprintln!("    nscheme              start the interactive REPL");
    eprintln!("    nscheme FILE         evaluate FILE and exit");
    eprintln!("    nscheme -e EXPR      evaluate EXPR, print result, exit");
    eprintln!("    nscheme -h, --help   show this message");
}

fn run_expr(env: &EnvRef, source: &str) -> ExitCode {
    match eval_source(source, env.clone()) {
        Ok(v) => {
            print_value(&v);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
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
    if let Err(e) = eval_source(&source, env.clone()) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run_repl(env: &EnvRef) -> ExitCode {
    println!(
        "nscheme {} — R7RS-small interpreter. Type (exit) or press Ctrl-D to quit.",
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
                Err(e) => eprintln!("error: {e}"),
            },
        }
    }

    if let Some(p) = &history_path {
        let _ = rl.save_history(p);
    }
    ExitCode::SUCCESS
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
        // Allow (exit) as a clean exit even though we don't have a real
        // (exit) procedure yet.
        let trimmed = line.trim();
        if buffer.is_empty() && (trimmed == "(exit)" || trimmed == ",quit") {
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
