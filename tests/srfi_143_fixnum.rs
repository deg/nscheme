//! (scheme fixnum) / (srfi 143) tests — bead nscheme-oeg.2.
//!
//! Cases are mined and translated from the SRFI 143 reference test
//! suite (chibi-test.scm, John Cowan, MIT). Rather than port a test
//! harness, each `(test EXPECTED EXPR)` becomes an equality check on
//! `run("EXPR")` and each `(test-assert EXPR)` becomes `assert_true`,
//! matching this repo's test convention.

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::library::set_search_path;
use nscheme::value::{Value, equal};

/// The library lives in the repo's real `lib/` tree.
fn lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/lib", env!("CARGO_MANIFEST_DIR")))
}

const PRELUDE: &str = r"
(import (scheme base) (scheme fixnum))
";

fn run(expr: &str) -> Result<Value, EvalError> {
    set_search_path(vec![lib_dir()]);
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&format!("{PRELUDE}\n{expr}"), env)
}

/// Assert that a Scheme expression evaluates to #t.
fn assert_true(expr: &str) {
    match run(expr) {
        Ok(Value::Bool(true)) => {}
        Ok(other) => panic!("expected #t from `{expr}`, got {other}"),
        Err(e) => panic!("error evaluating `{expr}`: {e:?}"),
    }
}

/// Assert that a Scheme expression evaluates to an exact integer equal
/// to `expected`.
fn assert_int(expr: &str, expected: i64) {
    match run(expr) {
        Ok(v) if equal(&v, &Value::Int(expected)) => {}
        Ok(other) => panic!("expected {expected} from `{expr}`, got {other}"),
        Err(e) => panic!("error evaluating `{expr}`: {e:?}"),
    }
}

#[test]
fn predicates() {
    assert_true(
        "(and (fixnum? 32767)
              (not (fixnum? 1.1))
              (fxzero? 0)
              (not (fxzero? 1))
              (fxpositive? 1)
              (not (fxpositive? -1))
              (fxnegative? -1)
              (not (fxnegative? 0))
              (fxodd? 1)
              (fxodd? -1)
              (not (fxodd? 102))
              (fxeven? 0)
              (fxeven? -2)
              (fxeven? 102))",
    );
}

#[test]
fn variadic_comparisons() {
    assert_true(
        "(and (fx=? 1 1 1)
              (not (fx=? 1 2 2))
              (not (fx=? 1 1 2))
              (fx<? 1 2 3)
              (not (fx<? 1 1 2))
              (fx>? 3 2 1)
              (not (fx>? 2 1 1))
              (fx<=? 1 1 2)
              (not (fx<=? 1 2 1))
              (fx>=? 2 1 1)
              (not (fx>=? 1 2 1)))",
    );
}

#[test]
fn arithmetic() {
    assert_int("(fxmax 3 5 4)", 5);
    assert_int("(fxmin 3 5 4)", 3);
    assert_int("(fx+ 3 4)", 7);
    assert_int("(fx* 4 3)", 12);
    assert_int("(fx- 3 4)", -1);
    assert_int("(fxneg 3)", -3);
    assert_int("(fxabs -7)", 7);
    assert_int("(fxsquare 42)", 1764);
}

#[test]
fn quotient_remainder() {
    assert_int("(fxquotient 5 2)", 2);
    assert_int("(fxquotient -5 2)", -2);
    assert_int("(fxremainder 13 4)", 1);
    assert_int("(fxremainder -13 4)", -1);
    assert_int("(fxremainder 13 -4)", 1);
}

#[test]
fn sqrt_returns_root_and_remainder() {
    // (let*-values (((root rem) (fxsqrt 32))) (* root rem)) => 35
    assert_int("(let-values (((root rem) (fxsqrt 32))) (* root rem))", 35);
}

#[test]
fn bitwise_logical() {
    assert_int("(fxnot 0)", -1);
    assert_int("(fxnot -1)", 0);
    assert_int("(fxnot 10)", -11);
    assert_int("(fxnot -37)", 36);
    assert_int("(fxand 14 6)", 6);
    assert_int("(fxand 11 26)", 10);
    assert_int("(fxior 10 12)", 14);
    assert_int("(fxior 3 10)", 11);
    assert_int("(fxxor 10 12)", 6);
    assert_int("(fxxor 3 10)", 9);
}

#[test]
fn bit_count_and_length() {
    assert_int("(fxbit-count 12)", 2);
    assert_int("(fxlength 0)", 0);
    assert_int("(fxlength 1)", 1);
    assert_int("(fxlength -1)", 0);
    assert_int("(fxlength 7)", 3);
    assert_int("(fxlength 8)", 4);
    assert_int("(fxlength 128)", 8);
    assert_int("(fxlength 255)", 8);
    assert_int("(fxlength 256)", 9);
}

#[test]
fn first_set_bit() {
    assert_int("(fxfirst-set-bit 0)", -1);
    assert_int("(fxfirst-set-bit 1)", 0);
    assert_int("(fxfirst-set-bit 4)", 2);
    assert_int("(fxfirst-set-bit 6)", 1);
    assert_int("(fxfirst-set-bit -4)", 2);
    assert_int("(fxfirst-set-bit 40)", 3);
}

#[test]
fn if_and_copy_and_set() {
    assert_int("(fxif 3 1 8)", 9);
    assert_int("(fxif 3 8 1)", 0);
    assert_int("(fxif #b00111100 #b11110000 #b00001111)", 0b0011_0011);
    assert_int("(fxcopy-bit 0 0 #t)", 1);
    assert_int("(fxcopy-bit 8 6 #t)", 0x106);
    assert_int("(fxcopy-bit 8 6 #f)", 6);
    assert_int("(fxcopy-bit 2 #b1111 #f)", 0b1011);
    assert_true(
        "(and (fxbit-set? 0 1)
              (not (fxbit-set? 1 1))
              (fxbit-set? 3 10)
              (fxbit-set? 2 6)
              (not (fxbit-set? 0 6)))",
    );
}

#[test]
fn arithmetic_shift_and_bit_field() {
    assert_int("(fxarithmetic-shift 1 1)", 2);
    assert_int("(fxarithmetic-shift 1 -1)", 0);
    assert_int("(fxarithmetic-shift 8 2)", 32);
    assert_int("(fxarithmetic-shift 8 -1)", 4);
    assert_int("(fxarithmetic-shift -1 3)", -8);
    assert_int("(fxbit-field 6 1 3)", 3);
    assert_int("(fxbit-field #b1101101010 0 4)", 0b1010);
    assert_int("(fxbit-field #b1101101010 3 9)", 0b10_1101);
}

#[test]
fn bit_field_rotate_and_reverse() {
    assert_int("(fxbit-field-rotate #b110 1 1 2)", 0b110);
    assert_int("(fxbit-field-rotate #b110 1 2 4)", 0b1010);
    assert_int("(fxbit-field-rotate #b0111 -1 1 4)", 0b1011);
    assert_int("(fxbit-field-rotate #b110 0 0 10)", 0b110);
    assert_int("(fxbit-field-reverse 6 1 3)", 6);
    assert_int("(fxbit-field-reverse 6 1 4)", 12);
}

#[test]
fn limits_are_exact_integers() {
    assert_true(
        "(and (exact-integer? fx-width)
              (exact-integer? fx-greatest)
              (exact-integer? fx-least)
              (fx<? fx-least fx-greatest))",
    );
}
