//! R7RS §3.5 tail-position verification.
//!
//! Every form that R7RS lists as introducing a tail position must run
//! a deep self-recursion without growing the Rust call stack. The
//! evaluator's step-loop architecture (see `docs/0001`) makes this
//! work *structurally* — these tests are the proof that the
//! architecture was implemented correctly for each form.
//!
//! The depth (100k or 250k) is chosen to be deep enough that any
//! per-call frame push would overflow Rust's default 8 MiB stack
//! while still completing in well under a second on a debug build.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Symbol, Value, equal};

fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}

fn done() -> Value {
    Value::Symbol(Symbol::intern("done"))
}

#[test]
fn tail_in_lambda_body() {
    let src = "(define (loop n) (if (= n 0) 'done (loop (- n 1))))
               (loop 100000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_if_consequent() {
    // The conseq of if is in tail position.
    let src = "(define (loop n)
                 (if (= n 0)
                     'done
                     (if #t (loop (- n 1)) 'never)))
               (loop 100000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_if_alternate() {
    // The alt of if is in tail position.
    let src = "(define (loop n)
                 (if (= n 0)
                     'done
                     (if #f 'never (loop (- n 1)))))
               (loop 100000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_begin_last() {
    let src = "(define (loop n)
                 (begin
                   (+ 1 2)
                   (if (= n 0) 'done (loop (- n 1)))))
               (loop 100000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_let_body() {
    let src = "(define (loop n)
                 (let ((m n))
                   (if (= m 0) 'done (loop (- m 1)))))
               (loop 100000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_let_star_body() {
    let src = "(define (loop n)
                 (let* ((a n) (b a))
                   (if (= b 0) 'done (loop (- b 1)))))
               (loop 100000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_letrec_body() {
    let src = "(define (loop n)
                 (letrec ((helper (lambda (x) x)))
                   (if (= n 0) 'done (loop (- (helper n) 1)))))
               (loop 50000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn named_let_tail_recursion() {
    let src = "(let loop ((n 100000))
                 (if (= n 0) 'done (loop (- n 1))))";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_cond_clause_body() {
    let src = "(define (loop n)
                 (cond ((= n 0) 'done)
                       (else (loop (- n 1)))))
               (loop 100000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_when_body() {
    // When the test is true, the last body expression is tail.
    let src = "(define (loop n)
                 (cond ((= n 0) 'done)
                       (else (when #t (loop (- n 1))))))
               (loop 100000)";
    // The when-body's result is the if's-without-alt return; that's
    // Unspecified, not 'done. Adjust expectation: loop will keep
    // recursing until n=0, where 'done is returned via cond.
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_and_last() {
    // The last expression of `and` (when reached) is in tail position.
    let src = "(define (loop n)
                 (and (> n -1) (if (= n 0) 'done (loop (- n 1)))))
               (loop 100000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_or_last() {
    // (or #f recur-expr) — recur-expr is in tail position.
    let src = "(define (loop n)
                 (or #f (if (= n 0) 'done (loop (- n 1)))))
               (loop 100000)";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn tail_in_do_loop() {
    // do desugars to a letrec loop; the body's recursive call is tail.
    let src = "(do ((i 0 (+ i 1))) ((= i 100000) 'done))";
    assert!(equal(&run(src).unwrap(), &done()));
}

#[test]
fn mutual_recursion_does_not_overflow() {
    // Mutual recursion crosses two closures' tail calls. Each call
    // must clean up before the next.
    let src = "(define (even? n) (if (= n 0) #t (odd? (- n 1))))
               (define (odd?  n) (if (= n 0) #f (even? (- n 1))))
               (even? 100000)";
    assert!(equal(&run(src).unwrap(), &Value::Bool(true)));
}

#[test]
fn case_arm_body_tail() {
    let src = "(define (loop n)
                 (case (modulo n 2)
                   ((0) (if (= n 0) 'done (loop (- n 1))))
                   ((1) (loop (- n 1)))))
               (loop 50000)";
    assert!(equal(&run(src).unwrap(), &done()));
}
