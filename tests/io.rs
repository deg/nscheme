//! Integration tests for the I/O primitives.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Value, equal};

fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

#[test]
fn open_input_string_and_read_char() {
    let v = run("(define p (open-input-string \"abc\"))
         (list (read-char p) (read-char p) (read-char p) (read-char p))")
    .unwrap();
    // a, b, c, then eof-object
    let items = vec![
        Value::Char('a'),
        Value::Char('b'),
        Value::Char('c'),
        Value::Eof,
    ];
    assert!(equal(&v, &Value::list_from(items)));
}

#[test]
fn peek_char_does_not_advance() {
    let v = run("(define p (open-input-string \"xy\"))
         (list (peek-char p) (read-char p) (read-char p))")
    .unwrap();
    let items = vec![Value::Char('x'), Value::Char('x'), Value::Char('y')];
    assert!(equal(&v, &Value::list_from(items)));
}

#[test]
fn read_line_from_string_port() {
    let v = run("(define p (open-input-string \"one\\ntwo\\nthree\"))
         (list (read-line p) (read-line p) (read-line p) (read-line p))")
    .unwrap();
    let items = vec![
        Value::string("one"),
        Value::string("two"),
        Value::string("three"),
        Value::Eof,
    ];
    assert!(equal(&v, &Value::list_from(items)));
}

#[test]
fn open_output_string_collects_writes() {
    let v = run("(define p (open-output-string))
         (write-string \"hello, \" p)
         (write-string \"world\" p)
         (get-output-string p)")
    .unwrap();
    assert!(equal(&v, &Value::string("hello, world")));
}

#[test]
fn write_uses_write_semantics_strings_get_quoted() {
    // write of a string adds quotes; display does not.
    let v = run("(define p (open-output-string))
         (write \"hi\" p)
         (display \" \" p)
         (display \"hi\" p)
         (get-output-string p)")
    .unwrap();
    assert!(equal(&v, &Value::string("\"hi\" hi")));
}

#[test]
fn newline_writes_newline() {
    let v = run("(define p (open-output-string))
         (display \"a\" p)
         (newline p)
         (display \"b\" p)
         (get-output-string p)")
    .unwrap();
    assert!(equal(&v, &Value::string("a\nb")));
}

#[test]
fn eof_object_predicate() {
    assert!(equal(
        &run("(eof-object? (eof-object))").unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(
        &run("(eof-object? 'x)").unwrap(),
        &Value::Bool(false)
    ));
}

#[test]
fn port_type_predicates() {
    assert!(equal(
        &run("(input-port? (open-input-string \"\"))").unwrap(),
        &Value::Bool(true),
    ));
    assert!(equal(
        &run("(output-port? (open-output-string))").unwrap(),
        &Value::Bool(true),
    ));
    assert!(equal(
        &run("(input-port? (open-output-string))").unwrap(),
        &Value::Bool(false),
    ));
    assert!(equal(
        &run("(textual-port? (open-input-string \"\"))").unwrap(),
        &Value::Bool(true),
    ));
    assert!(equal(
        &run("(binary-port? (open-input-string \"\"))").unwrap(),
        &Value::Bool(false),
    ));
}

#[test]
fn current_ports_resolve_to_stdio() {
    // These return the canonical $stdin/$stdout values from the env;
    // we just check the result is a port.
    let v = run("(input-port? (current-input-port))").unwrap();
    assert!(equal(&v, &Value::Bool(true)));
    let v = run("(output-port? (current-output-port))").unwrap();
    assert!(equal(&v, &Value::Bool(true)));
    let v = run("(output-port? (current-error-port))").unwrap();
    assert!(equal(&v, &Value::Bool(true)));
}

#[test]
fn file_round_trip() {
    let path = std::env::temp_dir().join("nscheme_io_test.scm");
    let path_str = path.to_str().unwrap().replace('\\', "\\\\");
    let _ = std::fs::remove_file(&path);
    let src = format!(
        "(define out (open-output-file \"{path_str}\"))
         (display \"line1\\nline2\" out)
         (close-port out)
         (define in (open-input-file \"{path_str}\"))
         (define result (list (read-line in) (read-line in)))
         (close-port in)
         result"
    );
    let v = run(&src).unwrap();
    let _ = std::fs::remove_file(&path);
    let expected = Value::list_from([Value::string("line1"), Value::string("line2")]);
    assert!(equal(&v, &expected));
}

#[test]
fn file_exists_then_delete() {
    let path = std::env::temp_dir().join("nscheme_io_exists.tmp");
    let path_str = path.to_str().unwrap().replace('\\', "\\\\");
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, "x").expect("write tmp");
    let v = run(&format!("(file-exists? \"{path_str}\")")).unwrap();
    assert!(equal(&v, &Value::Bool(true)));
    let _ = run(&format!("(delete-file \"{path_str}\")")).unwrap();
    let v = run(&format!("(file-exists? \"{path_str}\")")).unwrap();
    assert!(equal(&v, &Value::Bool(false)));
}
