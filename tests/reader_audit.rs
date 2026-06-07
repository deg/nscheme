//! Reader / lexical-syntax audit (bead nscheme-9gy).
//!
//! Confirms every R7RS §7.1.1 lexical form the chibi corpus does not
//! separately pin down, and documents the extension policy: we accept
//! ONLY the two standard `#!` directives (`fold-case` / `no-fold-case`)
//! and reject non-standard ones (`#!r6rs`, `#:kw`, …) with a clear error.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Value, equal};

fn run(src: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(src, env)
}

fn eq(expr: &str, expected: &str) {
    let v = run(&format!("(equal? {expr} {expected})")).unwrap();
    assert!(equal(&v, &Value::Bool(true)), "{expr} should equal {expected}");
}
fn truthy(expr: &str) {
    assert!(equal(&run(expr).unwrap(), &Value::Bool(true)), "{expr} should be #t");
}

#[test]
fn block_comments_nest() {
    eq("(+ 1 #| a comment |# 2)", "3");
    eq("(+ 1 #| outer #| nested |# still outer |# 2)", "3");
    eq("(list 1 #| x |# 2 #| y |# 3)", "'(1 2 3)");
}

#[test]
fn datum_comments() {
    eq("(+ 1 #;(this is dropped) 2)", "3");
    eq("(list 1 #;2 3)", "'(1 3)");
}

#[test]
fn datum_labels_including_cycles() {
    truthy("(let ((x '#0=(a . #0#))) (eq? x (cdr x)))");
    // Forward reference inside the same datum.
    truthy("(let ((v '#1=(1 2 . #1#))) (and (= (car v) 1) (eq? v (cddr v))))");
}

#[test]
fn all_named_characters() {
    for (name, code) in [
        ("alarm", 7),
        ("backspace", 8),
        ("delete", 127),
        ("escape", 27),
        ("newline", 10),
        ("null", 0),
        ("return", 13),
        ("space", 32),
        ("tab", 9),
    ] {
        eq(&format!("(char->integer #\\{name})"), &code.to_string());
    }
}

#[test]
fn hex_scalar_characters() {
    eq("(char->integer #\\x41)", "65");
    eq("(char->integer #\\x3bb)", "955"); // λ
    eq("(char->integer #\\x1f600)", "128512"); // 😀
}

#[test]
fn fold_case_directives() {
    eq("(begin #!fold-case (symbol->string 'ABC))", "\"abc\"");
    eq("(begin #!no-fold-case (symbol->string 'ABC))", "\"ABC\"");
}

#[test]
fn boolean_long_and_short_forms() {
    truthy("(eq? #t #true)");
    truthy("(eq? #f #false)");
}

#[test]
fn vector_and_bytevector_literals() {
    eq("#(1 2 3)", "(vector 1 2 3)");
    eq("#u8(1 2 255)", "(bytevector 1 2 255)");
}

#[test]
fn unknown_directives_are_rejected() {
    // Non-standard `#!` directives error with a directive-specific message.
    let err = run("#!r6rs").unwrap_err();
    assert!(format!("{err}").contains("#!"), "error was: {err}");
    // Keyword syntax is not part of R7RS.
    assert!(run("'#:kw").is_err());
}
