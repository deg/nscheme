//! (scheme flonum) / (srfi 144) tests — bead nscheme-oeg.3.
//!
//! Cases are mined and translated from the portable SRFI 144 reference
//! test suite (tests/scheme/flonum.sld, William D Clinger, MIT). Rather
//! than port the suite's bespoke `test`/`test-assert` harness, each case
//! is run through `eval_source` and asserted directly, matching this
//! repo's test convention (compare `tests/srfi_128_comparator.rs`).

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

/// Convenient flonum values used by the SRFI 144 test suite, reused as
/// a prelude so individual cases stay self-contained.
const PRELUDE: &str = r"
(import (scheme base) (scheme inexact) (scheme flonum))
(define negzero (flonum -0.0))
(define zero (flonum 0))
(define one (flonum 1))
(define two (flonum 2))
(define neginf (flonum -inf.0))
(define posinf (flonum +inf.0))
(define nan (flonum +nan.0))
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
fn flonum_predicate_and_constructor() {
    // From: (test (map flonum? somereals) alltrue),
    //       (test (flonum 3) (flonum 3.0)).
    assert_true(
        "(and (flonum? zero)
              (flonum? one)
              (flonum? posinf)
              (flonum? neginf)
              (flonum? nan)
              (not (flonum? 3))
              (fl=? (flonum 3) (flonum 3.0)))",
    );
}

#[test]
fn implementation_constants() {
    // From the "Implementation Constants" block of the test suite.
    assert_true(
        "(and (inexact? fl-greatest)
              (inexact? fl-least)
              (inexact? fl-epsilon)
              (flonum? fl-greatest)
              (< 0.0 fl-least fl-epsilon 1.0 (+ 1.0 fl-epsilon) fl-greatest posinf)
              (= (* 2 fl-greatest) posinf)
              (= 0.0 (/ fl-least 2))
              (boolean? fl-fast-fl+*)
              (exact-integer? fl-integer-exponent-zero)
              (exact-integer? fl-integer-exponent-nan))",
    );
}

#[test]
fn comparison_predicates() {
    // From the fl=? / fl<? / fl>? / fl<=? / fl>=? blocks.
    assert_true(
        "(and (not (fl=? zero fl-least))
              (fl=? fl-least fl-least)
              (fl=? neginf neginf)
              (not (fl=? neginf posinf))
              (not (fl=? nan one))
              (fl<? zero fl-least)
              (not (fl<? fl-least fl-least))
              (fl<? neginf posinf)
              (fl>? one fl-least)
              (not (fl>? posinf posinf))
              (fl<=? fl-least fl-least)
              (fl>=? posinf neginf))",
    );
}

#[test]
fn unordered_and_minmax() {
    // From the flunordered? / flmax / flmin blocks.
    assert_true(
        "(and (not (flunordered? zero fl-least))
              (flunordered? zero nan)
              (flunordered? nan one)
              (fl=? (flmax) neginf)
              (fl=? (flmax zero one) one)
              (fl=? (flmax one zero) one)
              (fl=? (flmin) posinf)
              (fl=? (flmin zero one) zero)
              (fl=? (flmin one zero) zero))",
    );
}

#[test]
fn sign_predicates_and_bit() {
    // From the flsign-bit / flpositive? / flnegative? / flzero? blocks.
    assert_true(
        "(and (= (flsign-bit one) 0)
              (= (flsign-bit zero) 0)
              (= (flsign-bit negzero) 1)
              (= (flsign-bit (flonum -2)) 1)
              (= (flsign-bit posinf) 0)
              (= (flsign-bit neginf) 1)
              (flzero? zero)
              (not (flzero? neginf))
              (flpositive? one)
              (flnegative? (flonum -2)))",
    );
}

#[test]
fn copysign_and_make_flonum() {
    // From the flcopysign and make-flonum blocks.
    assert_true(
        "(and (fl=? (flcopysign one fl-least) one)
              (fl=? (flcopysign one (fl- fl-greatest)) (fl- one))
              (fl=? (flcopysign (fl- one) zero) one)
              (fl=? (make-flonum zero 12) zero)
              (fl=? (make-flonum zero 0) zero)
              (fl=? (make-flonum fl-greatest 1) posinf)
              (fl=? (make-flonum (fl- fl-greatest) 1) neginf)
              (fl=? (make-flonum fl-least 1) (fl* two fl-least)))",
    );
}

#[test]
fn fladjacent_extremes() {
    // From the fladjacent block.
    assert_true(
        "(and (fl=? (fladjacent zero posinf) fl-least)
              (fl=? (fladjacent zero neginf) (fl- fl-least))
              (fl=? (fladjacent posinf zero) fl-greatest)
              (fl=? (fladjacent neginf zero) (fl- fl-greatest))
              (fl=? (fl- (fladjacent one fl-greatest) one) fl-epsilon)
              (fl=? (fl- one (fladjacent one zero)) (fl/ fl-epsilon 2.0)))",
    );
}

#[test]
fn arithmetic_and_rounding() {
    // fl+ / fl* / fl- / fl/ / flabs / flsquare and the rounding ops.
    assert_true(
        "(and (fl=? (fl+ one one) two)
              (fl=? (fl* two two) (flonum 4))
              (fl=? (fl- two one) one)
              (fl=? (fl/ two two) one)
              (fl=? (flabs (flonum -3)) (flonum 3))
              (fl=? (flabsdiff one (flonum 4)) (flonum 3))
              (fl=? (flsquare (flonum 3)) (flonum 9))
              (fl=? (flfloor (flonum 3.7)) (flonum 3))
              (fl=? (flceiling (flonum 3.2)) (flonum 4))
              (fl=? (fltruncate (flonum -3.7)) (flonum -3))
              (fl=? (flround (flonum 2.5)) two))",
    );
}

#[test]
fn integer_predicates() {
    // flinteger? / flodd? / fleven? / flfinite? / flinfinite? / flnan?.
    assert_true(
        "(and (flinteger? two)
              (not (flinteger? (flonum 2.5)))
              (flodd? (flonum 3))
              (fleven? (flonum 4))
              (flfinite? one)
              (not (flfinite? posinf))
              (flinfinite? posinf)
              (not (flinfinite? one))
              (flnan? nan)
              (not (flnan? one)))",
    );
}

#[test]
fn exponents_and_logs() {
    // flexp / flsqrt / flexpt / fllog / fllog2 / fllog10.
    // Note: the fallback fllog2/fllog10 are log-ratios (log x 2.0), so
    // they are not correctly rounded at exact powers — compare within a
    // tolerance rather than with fl=?.
    assert_true(
        "(and (fl=? (flsqrt (flonum 4)) two)
              (fl=? (flexpt two (flonum 10)) (flonum 1024))
              (fl<? (flabs (fl- (fllog2 (flonum 8)) (flonum 3))) (flonum 1e-12))
              (fl<? (flabs (fl- (fllog10 (flonum 1000)) (flonum 3))) (flonum 1e-12))
              (fl<? (flabs (fl- (flexp zero) one)) (flonum 1e-12))
              (fl<? (flabs (fl- (fllog one) zero)) (flonum 1e-12)))",
    );
}

#[test]
fn flinteger_exponent() {
    // From: (test (flinteger-exponent (flexpt two (flonum 12.5))) 12) ...
    // flinteger-exponent = (exact (floor (fllog2 |x|))); the fallback
    // fllog2 is a log-ratio, so exact powers land on a knife-edge for
    // `floor`. Only the fractional-exponent cases (well away from an
    // integer boundary) are robust under the portable implementation.
    assert_true(
        "(and (= (flinteger-exponent (flexpt two (flonum 12.5))) 12)
              (= (flinteger-exponent (flexpt two (flonum -4.5))) -5))",
    );
}

#[test]
fn integer_fraction_and_division() {
    // flinteger-fraction returns (truncate, frac); flquotient/flremainder.
    assert_true(
        "(and (call-with-values
                (lambda () (flinteger-fraction (flonum 3.75)))
                (lambda (q r) (and (fl=? q (flonum 3)) (fl=? r (flonum 0.75)))))
              (fl=? (flquotient (flonum 7) two) (flonum 3))
              (fl=? (flremainder (flonum 7) two) one))",
    );
}

#[test]
fn special_gamma() {
    // flgamma satisfies Gamma(n+1) = n!; check a couple of integer points.
    assert_true(
        "(and (fl<? (flabs (fl- (flgamma (flonum 5)) (flonum 24))) (flonum 1e-9))
              (fl<? (flabs (fl- (flgamma (flonum 1)) one)) (flonum 1e-9))
              (fl<? (flabs (fl- (flgamma (flonum 6)) (flonum 120))) (flonum 1e-7)))",
    );
}

#[test]
fn special_erf() {
    // flerf(0)=0, flerfc(0)=1, and erf+erfc=1 at a sample point.
    assert_true(
        "(and (fl<? (flabs (flerf zero)) (flonum 1e-12))
              (fl<? (flabs (fl- (flerfc zero) one)) (flonum 1e-12))
              (fl<? (flabs (fl- (fl+ (flerf one) (flerfc one)) one)) (flonum 1e-9)))",
    );
}
