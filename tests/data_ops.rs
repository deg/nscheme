//! Integration tests for the string / char / symbol / vector /
//! bytevector primitives (R7RS-small base library).

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Symbol, Value, equal};

fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

// ---- characters --------------------------------------------------

#[test]
fn char_integer_round_trip() {
    assert!(equal(
        &run(r"(char->integer #\A)").unwrap(),
        &Value::Int(65)
    ));
    assert!(equal(
        &run("(integer->char 65)").unwrap(),
        &Value::Char('A')
    ));
    assert!(equal(
        &run("(integer->char 955)").unwrap(),
        &Value::Char('λ')
    ));
}

#[test]
fn char_predicates() {
    assert!(equal(
        &run(r"(char-alphabetic? #\a)").unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(
        &run(r"(char-numeric? #\5)").unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(
        &run(r"(char-whitespace? #\space)").unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(
        &run(r"(char-upper-case? #\A)").unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(
        &run(r"(char-lower-case? #\a)").unwrap(),
        &Value::Bool(true)
    ));
}

#[test]
fn char_case_conversion() {
    assert!(equal(
        &run(r"(char-upcase #\a)").unwrap(),
        &Value::Char('A')
    ));
    assert!(equal(
        &run(r"(char-downcase #\A)").unwrap(),
        &Value::Char('a')
    ));
}

#[test]
fn char_comparison_chain() {
    assert!(equal(
        &run(r"(char<? #\a #\b #\c)").unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(
        &run(r"(char<? #\a #\a)").unwrap(),
        &Value::Bool(false)
    ));
}

// ---- strings -----------------------------------------------------

#[test]
fn string_length_handles_unicode() {
    // Unicode chars count as one position each (not bytes).
    assert!(equal(
        &run(r#"(string-length "hello")"#).unwrap(),
        &Value::Int(5)
    ));
    assert!(equal(
        &run(r#"(string-length "λμν")"#).unwrap(),
        &Value::Int(3)
    ));
}

#[test]
fn string_ref_indexes_chars() {
    assert!(equal(
        &run(r#"(string-ref "hello" 1)"#).unwrap(),
        &Value::Char('e')
    ));
}

#[test]
fn substring_extracts_range() {
    let v = run(r#"(substring "hello world" 6 11)"#).unwrap();
    assert!(equal(&v, &Value::string("world")));
}

#[test]
fn string_append_concatenates() {
    let v = run(r#"(string-append "foo" "bar" "baz")"#).unwrap();
    assert!(equal(&v, &Value::string("foobarbaz")));
}

#[test]
fn string_list_round_trip() {
    let v = run(r#"(list->string (string->list "abc"))"#).unwrap();
    assert!(equal(&v, &Value::string("abc")));
}

#[test]
fn string_comparison() {
    assert!(equal(
        &run(r#"(string<? "apple" "banana" "cherry")"#).unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(
        &run(r#"(string=? "x" "x" "x")"#).unwrap(),
        &Value::Bool(true)
    ));
}

#[test]
fn string_to_number_parses() {
    assert!(equal(
        &run(r#"(string->number "42")"#).unwrap(),
        &Value::Int(42)
    ));
    assert!(equal(
        &run(r#"(string->number "2.5")"#).unwrap(),
        &Value::Float(2.5)
    ));
    // R7RS: returns #f on parse failure.
    assert!(equal(
        &run(r#"(string->number "not-a-number")"#).unwrap(),
        &Value::Bool(false)
    ));
}

#[test]
fn number_to_string_renders() {
    assert!(equal(
        &run("(number->string 42)").unwrap(),
        &Value::string("42")
    ));
}

// ---- symbols -----------------------------------------------------

#[test]
fn symbol_string_round_trip() {
    let v = run(r"(symbol->string 'foo)").unwrap();
    assert!(equal(&v, &Value::string("foo")));
    let v = run(r#"(string->symbol "bar")"#).unwrap();
    assert!(equal(&v, &Value::Symbol(Symbol::intern("bar"))));
}

#[test]
fn symbol_equal() {
    assert!(equal(
        &run(r"(symbol=? 'foo 'foo)").unwrap(),
        &Value::Bool(true)
    ));
    assert!(equal(
        &run(r"(symbol=? 'foo 'bar)").unwrap(),
        &Value::Bool(false)
    ));
}

// ---- vectors -----------------------------------------------------

#[test]
fn vector_creation_and_access() {
    assert!(equal(
        &run("(vector-length (vector 1 2 3))").unwrap(),
        &Value::Int(3)
    ));
    assert!(equal(
        &run("(vector-ref (vector 'a 'b 'c) 1)").unwrap(),
        &Value::Symbol(Symbol::intern("b"))
    ));
}

#[test]
fn make_vector_with_fill() {
    let v = run("(make-vector 3 0)").unwrap();
    let expected = Value::vector(vec![Value::Int(0), Value::Int(0), Value::Int(0)]);
    assert!(equal(&v, &expected));
}

#[test]
fn vector_set_mutates() {
    let v = run("(define v (vector 1 2 3)) (vector-set! v 1 99) v").unwrap();
    let expected = Value::vector(vec![Value::Int(1), Value::Int(99), Value::Int(3)]);
    assert!(equal(&v, &expected));
}

#[test]
fn vector_list_round_trip() {
    let v = run("(vector->list (list->vector '(a b c)))").unwrap();
    let expected = Value::list_from([
        Value::Symbol(Symbol::intern("a")),
        Value::Symbol(Symbol::intern("b")),
        Value::Symbol(Symbol::intern("c")),
    ]);
    assert!(equal(&v, &expected));
}

#[test]
fn vector_fill() {
    let v = run("(define v (vector 1 2 3)) (vector-fill! v 'x) v").unwrap();
    let x = Value::Symbol(Symbol::intern("x"));
    let expected = Value::vector(vec![x.clone(), x.clone(), x]);
    assert!(equal(&v, &expected));
}

// ---- bytevectors -------------------------------------------------

#[test]
fn bytevector_creation() {
    let v = run("(bytevector 1 2 3)").unwrap();
    assert!(equal(&v, &Value::bytevector(vec![1, 2, 3])));
}

#[test]
fn make_bytevector_with_fill() {
    let v = run("(make-bytevector 4 7)").unwrap();
    assert!(equal(&v, &Value::bytevector(vec![7, 7, 7, 7])));
}

#[test]
fn bytevector_ref_and_set() {
    let v = run("(define b (make-bytevector 3 0))
         (bytevector-u8-set! b 1 99)
         (bytevector-u8-ref b 1)")
    .unwrap();
    assert!(equal(&v, &Value::Int(99)));
}

#[test]
fn utf8_string_round_trip() {
    let v = run(r#"(utf8->string (string->utf8 "héllo"))"#).unwrap();
    assert!(equal(&v, &Value::string("héllo")));
}
