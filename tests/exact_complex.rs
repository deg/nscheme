//! Exact-complex arithmetic (bead nscheme-5mn).
//!
//! Complex numbers with exact Cartesian parts now stay exact through
//! `+ - * /`, instead of falling through to inexact f64. Inexactness is
//! contagious: one inexact operand forces an inexact result.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Value, equal};

fn run(src: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&format!("(import (scheme base))\n{src}"), env)
}

/// Assert `expr` is `equal?` to `expected` (also Scheme source).
fn eq(expr: &str, expected: &str) {
    let v = run(&format!("(equal? {expr} {expected})")).unwrap();
    assert!(equal(&v, &Value::Bool(true)), "{expr} should equal {expected}");
}

/// Assert `expr` evaluates to #t.
fn truthy(expr: &str) {
    assert!(equal(&run(expr).unwrap(), &Value::Bool(true)), "{expr} should be #t");
}

#[test]
fn products_and_sums_stay_exact() {
    eq("(* 1+2i 1-2i)", "5"); // collapses to an exact real
    eq("(* 1+2i 1+2i)", "-3+4i");
    eq("(+ 3+4i 1-1i)", "4+3i");
    eq("(- 3+4i 1-1i)", "2+5i");
    eq("(/ 1 1+2i)", "1/5-2/5i");
    truthy("(exact? (* 1+2i 1-2i))");
    truthy("(exact? (/ 1 1+2i))");
}

#[test]
fn exactness_predicates() {
    truthy("(exact? 1+2i)");
    truthy("(not (exact? 1.0+2.0i))");
    truthy("(inexact? 1.0+2.0i)");
    truthy("(not (inexact? 1+2i))");
}

#[test]
fn inexactness_is_contagious() {
    truthy("(inexact? (+ 1.0 1+2i))");
    truthy("(inexact? (* 1+2i 2.0))");
    eq("(+ 1.0 1+2i)", "2.0+2.0i");
}

#[test]
fn conversions_preserve_both_parts() {
    eq("(exact->inexact 1+2i)", "1.0+2.0i");
    eq("(inexact->exact 1.0+2.0i)", "1+2i");
    eq("(exact 1.5+2.5i)", "3/2+5/2i");
    eq("(inexact 1+2i)", "1.0+2.0i");
    truthy("(exact? (inexact->exact 1.0+2.0i))");
}

#[test]
fn normalization_keeps_equality() {
    // (* z 1) must round-trip through rational_to_value so Int vs Rational
    // representation doesn't break eqv?.
    truthy("(eqv? (* 1+2i 1) 1+2i)");
    truthy("(equal? (+ 0 3+4i) 3+4i)");
}
