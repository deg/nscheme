//! (scheme bitwise) / (srfi 151) tests — bead nscheme-oeg.1.
//!
//! Cases are mined and translated from the SRFI 151 reference test
//! suite (chibi-test.scm, John Cowan, MIT). Rather than port the test
//! harness, each `(test EXPECTED EXPR)` becomes an equality check and
//! each `(test-assert EXPR)` becomes an `assert_true`, matching this
//! repo's test convention. Cases are chosen for small, self-evident
//! result values; the impl is portable over generic integer
//! arithmetic, so a few cases (e.g. `bit-field` with a large `end`)
//! build wide intermediate masks that rely on nscheme's bignums.

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::library::set_search_path;
use nscheme::value::Value;

/// The library lives in the repo's real `lib/` tree.
fn lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/lib", env!("CARGO_MANIFEST_DIR")))
}

const PRELUDE: &str = r"
(import (scheme base) (scheme bitwise))
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

#[test]
fn core_logical_ops() {
    // bitwise-not, -and, -ior, -xor, -eqv on small integers.
    assert_true(
        "(and (= -1 (bitwise-not 0))
              (= 0 (bitwise-not -1))
              (= -11 (bitwise-not 10))
              (= 6 (bitwise-and 14 6))
              (= 10 (bitwise-and 11 26))
              (= 14 (bitwise-ior 10 12))
              (= 11 (bitwise-ior 3 10))
              (= 6 (bitwise-xor 10 12))
              (= 9 (bitwise-xor 3 10))
              (= -42 (bitwise-eqv 37 12)))",
    );
}

#[test]
fn derived_logical_ops() {
    assert_true(
        "(and (= -1 (bitwise-nand 0 0))
              (= -124 (bitwise-nand -1 123))
              (= -11 (bitwise-nand 11 26))
              (= -28 (bitwise-nor 11 26))
              (= 16 (bitwise-andc1 11 26))
              (= 1 (bitwise-andc2 11 26))
              (= -2 (bitwise-orc1 11 26)))",
    );
}

#[test]
fn shift_count_and_length() {
    assert_true(
        "(and (= 2 (arithmetic-shift 1 1))
              (= 0 (arithmetic-shift 1 -1))
              (= 4 (arithmetic-shift 1 2))
              (= 16 (arithmetic-shift 1 4))
              (= 32 (arithmetic-shift 8 2))
              (= 4 (arithmetic-shift 8 -1))
              (= -4 (arithmetic-shift -1 2))
              (= 2 (bit-count 12))
              (= 0 (integer-length 0))
              (= 1 (integer-length 1))
              (= 0 (integer-length -1))
              (= 3 (integer-length 7))
              (= 3 (integer-length -7))
              (= 4 (integer-length 8)))",
    );
}

#[test]
fn bitwise_if_op() {
    assert_true(
        "(and (= 9 (bitwise-if 3 1 8))
              (= 0 (bitwise-if 3 8 1))
              (= 3 (bitwise-if 1 1 2))
              (= #b00110011 (bitwise-if #b00111100 #b11110000 #b00001111)))",
    );
}

#[test]
fn single_bit_ops() {
    // bit-set? takes (index n).
    assert_true(
        "(and (bit-set? 0 1)
              (not (bit-set? 1 1))
              (not (bit-set? 1 8))
              (bit-set? 3 10)
              (bit-set? 2 6)
              (not (bit-set? 0 6))
              (= 1 (copy-bit 0 0 #t))
              (= #x106 (copy-bit 8 6 #t))
              (= 6 (copy-bit 8 6 #f))
              (= -2 (copy-bit 0 -1 #f)))",
    );
}

#[test]
fn any_every_and_first_set() {
    assert_true(
        "(and (any-bit-set? 3 6)
              (not (any-bit-set? 8 6))
              (every-bit-set? 4 6)
              (not (every-bit-set? 7 6))
              (= -1 (first-set-bit 0))
              (= 0 (first-set-bit 1))
              (= 0 (first-set-bit 3))
              (= 2 (first-set-bit 4))
              (= 1 (first-set-bit 6))
              (= 1 (first-set-bit -2))
              (= 2 (first-set-bit -28)))",
    );
}

#[test]
fn bit_field_ops() {
    assert_true(
        "(and (= 0 (bit-field 6 0 1))
              (= 3 (bit-field 6 1 3))
              (= 1 (bit-field 6 2 999))
              (= #b1010 (bit-field #b1101101010 0 4))
              (= #b101101 (bit-field #b1101101010 3 9))
              (= #b110 (bit-field-rotate #b110 0 0 10))
              (= #b1010 (bit-field-rotate #b110 1 2 4))
              (= 6 (bit-field-reverse 6 1 3)))",
    );
}

#[test]
fn bit_swap_op() {
    assert_true(
        "(and (= #b1011 (bit-swap 1 2 #b1101))
              (= #b1011 (bit-swap 2 1 #b1101))
              (= #b1110 (bit-swap 0 1 #b1101))
              (= 1 (bit-swap 0 2 4)))",
    );
}

#[test]
fn bits_list_and_vector_conversions() {
    assert_true(
        "(and (equal? '(#t #f #t #f #t #t #t) (bits->list #b1110101))
              (equal? '(#f #t #f #t) (bits->list #b111010 4))
              (equal? '(#t #t) (bits->list 3))
              (equal? '(#f #t #t #f) (bits->list 6 4))
              (= #b1110101 (list->bits '(#t #f #t #f #t #t #t)))
              (= 6 (list->bits '(#f #t #t)))
              (= 12 (list->bits '(#f #f #t #t)))
              (= 6 (vector->bits '#(#f #t #t)))
              (equal? '#(#t #t) (bits->vector 3))
              (= 6 (bits #f #t #t)))",
    );
}

#[test]
fn fold_and_generator() {
    // bitwise-fold collects the bit booleans low-to-high.
    assert_true("(equal? '(#t #f #t) (reverse (bitwise-fold cons '() 5)))");
    // make-bitwise-generator yields bits low-to-high.
    assert_true(
        "(let ((g (make-bitwise-generator 6)))
           (let* ((b0 (g)) (b1 (g)) (b2 (g)))
             (and (not b0) b1 b2)))",
    );
    // bitwise-unfold rebuilds an integer from a bit predicate.
    assert_true(
        "(= 5 (bitwise-unfold (lambda (i) (= i 3))
                              (lambda (i) (odd? (bit-field 5 i (+ i 1))))
                              (lambda (i) (+ i 1))
                              0))",
    );
}
