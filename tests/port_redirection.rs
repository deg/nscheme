//! Current ports as parameters + redirection (bead nscheme-oge).
//!
//! current-input/output/error-port are now `parameterize`-able parameters,
//! and the default-port forms of the I/O primitives read them — so
//! with-output-to-string / with-input-from-string / with-output-to-file /
//! with-input-from-file work.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Value, equal};

fn run(src: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&format!("(import (scheme base) (scheme file))\n{src}"), env)
}
fn eq(expr: &str, expected: &str) {
    let v = run(&format!("(equal? {expr} {expected})")).unwrap();
    assert!(equal(&v, &Value::Bool(true)), "{expr} should equal {expected}");
}
fn truthy(expr: &str) {
    assert!(equal(&run(expr).unwrap(), &Value::Bool(true)), "{expr} should be #t");
}

#[test]
fn current_ports_are_parameters() {
    truthy("(procedure? current-output-port)");
    truthy("(output-port? (current-output-port))");
    truthy("(input-port? (current-input-port))");
}

#[test]
fn with_output_to_string_captures_default_output() {
    eq(
        "(with-output-to-string (lambda () (display \"hi \") (write 42)))",
        "\"hi 42\"",
    );
}

#[test]
fn parameterize_current_output_port_redirects() {
    eq(
        "(let ((s (open-output-string)))
           (parameterize ((current-output-port s)) (display \"x\") (write '(1 2)))
           (get-output-string s))",
        "\"x(1 2)\"",
    );
}

#[test]
fn redirection_is_dynamically_scoped() {
    // After the parameterize extent, output goes back to the original port.
    eq(
        "(let ((s (open-output-string)))
           (parameterize ((current-output-port s)) (display \"in\"))
           (with-output-to-string (lambda () (display \"out\"))))",
        "\"out\"",
    );
}

#[test]
fn nested_with_output_to_string() {
    eq(
        "(with-output-to-string
           (lambda ()
             (display \"a\")
             (display (with-output-to-string (lambda () (display \"INNER\"))))
             (display \"b\")))",
        "\"aINNERb\"",
    );
}

#[test]
fn with_input_from_string_feeds_the_readers() {
    eq("(with-input-from-string \"(a b c) 99\" (lambda () (read)))", "'(a b c)");
    eq("(with-input-from-string \"(a b c) 99\" (lambda () (read) (read)))", "99");
    eq(
        "(with-input-from-string \"l1\\nl2\" (lambda () (read-line)))",
        "\"l1\"",
    );
    eq(
        "(with-input-from-string \"xyz\" (lambda () (list (read-char) (peek-char) (read-char))))",
        "'(#\\x #\\y #\\y)",
    );
}

#[test]
fn file_redirection_round_trips() {
    let path = std::env::temp_dir().join("nscheme-oge-roundtrip.scm");
    let p = path.display().to_string();
    let src = format!(
        "(with-output-to-file \"{p}\" (lambda () (display \"hello\") (newline) (write '(x y z))))
         (with-input-from-file \"{p}\" (lambda () (cons (read-line) (read))))"
    );
    let v = run(&src).unwrap();
    let _ = std::fs::remove_file(&path);
    let expected = run("(cons \"hello\" '(x y z))").unwrap();
    assert!(equal(&v, &expected), "got {v:?}");
}

#[test]
fn char_ready_validates_the_port() {
    truthy("(char-ready? (open-input-string \"x\"))");
    truthy("(char-ready?)"); // defaults to current-input-port
    // A non-input port is an error.
    assert!(run("(char-ready? (open-output-string))").is_err());
}
