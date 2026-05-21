//! `syntax-rules` macro tests covering the four amateur-failure-mode
//! cases the advisor called out, plus general usage.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Symbol, Value, equal};

fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

// ---- failure mode A: hygiene (swap! capture) ---------------------

#[test]
fn swap_macro_does_not_capture_user_identifier() {
    let src = "
        (define-syntax swap!
          (syntax-rules ()
            ((_ a b) (let ((tmp a)) (set! a b) (set! b tmp)))))
        (define tmp 1)
        (define x 2)
        (swap! tmp x)
        (list tmp x)
    ";
    // After swap! the user's `tmp` should hold the old `x` (2)
    // and `x` should hold the old `tmp` (1). A non-hygienic macro
    // would expand the inner `tmp` to capture the user's `tmp`
    // binding and the result would be wrong.
    let v = run(src).unwrap();
    let expected = Value::list_from([Value::Int(2), Value::Int(1)]);
    assert!(equal(&v, &expected));
}

// ---- failure mode B: ellipsis depth ------------------------------

#[test]
fn my_let_star_via_syntax_rules() {
    let src = "
        (define-syntax my-let*
          (syntax-rules ()
            ((_ () body) body)
            ((_ ((v e) rest ...) body)
             (let ((v e)) (my-let* (rest ...) body)))))
        (my-let* ((a 1) (b (+ a 1)) (c (+ b 1))) c)
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(3)));
}

#[test]
fn ellipsis_empty_case() {
    // The pattern's `rest ...` must match zero elements as well.
    let src = "
        (define-syntax fold-empty
          (syntax-rules ()
            ((_ () done) done)
            ((_ (x rest ...) done) (cons x (fold-empty (rest ...) done)))))
        (fold-empty () 'finished)
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("finished"))
    ));
}

// ---- failure mode C: literals don't bind --------------------------

#[test]
fn literals_match_themselves_not_arbitrary_input() {
    let src = "
        (define-syntax tif
          (syntax-rules (then else)
            ((_ a then b else c) (if a b c))))
        (tif #t then 'yes else 'no)
    ";
    assert!(equal(
        &run(src).unwrap(),
        &Value::Symbol(Symbol::intern("yes"))
    ));
}

#[test]
fn literals_fail_to_match_non_literal_inputs() {
    // The 'random isn't `then`, so the only clause shouldn't match.
    let src = "
        (define-syntax tif
          (syntax-rules (then else)
            ((_ a then b else c) (if a b c))))
        (tif #t 'random 'yes 'random 'no)
    ";
    // Expansion fails → MalformedForm error.
    let err = run(src).unwrap_err();
    assert!(
        matches!(err, EvalError::MalformedForm { .. }),
        "expected MalformedForm, got {err:?}"
    );
}

// ---- failure mode D: `_` is wildcard ------------------------------

#[test]
fn underscore_in_head_does_not_have_to_match_macro_name() {
    // The `_` in `(_ x)` is purely a wildcard, NOT the macro name.
    let src = "
        (define-syntax double
          (syntax-rules ()
            ((_ x) (+ x x))))
        (double 5)
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(10)));
}

// ---- general use ---------------------------------------------------

#[test]
fn nested_macro_expansion() {
    // A macro expanding to a call to another macro must work — this is
    // exactly why the spec requires re-evaluating the expansion.
    let src = "
        (define-syntax inc
          (syntax-rules ()
            ((_ x) (+ x 1))))
        (define-syntax inc2
          (syntax-rules ()
            ((_ x) (inc (inc x)))))
        (inc2 5)
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(7)));
}

#[test]
fn let_syntax_local_macros() {
    let src = "
        (let-syntax ((m (syntax-rules () ((_ x) (* x 10)))))
          (m 4))
    ";
    assert!(equal(&run(src).unwrap(), &Value::Int(40)));
}

#[test]
fn macro_with_multiple_clauses() {
    // Picks first matching clause.
    let src = "
        (define-syntax kind
          (syntax-rules ()
            ((_ ()) 'empty)
            ((_ (x)) 'single)
            ((_ (x y ...)) 'many)))
        (list (kind ()) (kind (1)) (kind (1 2 3)))
    ";
    let expected = Value::list_from([
        Value::Symbol(Symbol::intern("empty")),
        Value::Symbol(Symbol::intern("single")),
        Value::Symbol(Symbol::intern("many")),
    ]);
    assert!(equal(&run(src).unwrap(), &expected));
}
