//! (scheme list) / (srfi 1) tests — bead nscheme-lul.3.
//!
//! Cases are mined and translated from the SRFI 1 specification's worked
//! examples (Olin Shivers, MIT). The SRFI 1 repo ships no SRFI-64 test
//! suite, so these assertions are drawn from the `=>` example results in
//! the SRFI document and the library's documented semantics. Each case is
//! run through `eval_source`, matching this repo's test convention.

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

const PRELUDE: &str = "(import (scheme base) (scheme list))";

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

/// Assert that `expr` evaluates `equal?` to `expected` (also Scheme source).
fn assert_equal(expr: &str, expected: &str) {
    assert_true(&format!("(equal? {expr} {expected})"));
}

#[test]
fn constructors() {
    // (iota 5)            => (0 1 2 3 4)
    assert_equal("(iota 5)", "'(0 1 2 3 4)");
    // (iota 5 0 2)        => (0 2 4 6 8)
    assert_equal("(iota 5 0 2)", "'(0 2 4 6 8)");
    // (cons* 1 2 3 4)     => (1 2 3 . 4)
    assert_equal("(cons* 1 2 3 4)", "'(1 2 3 . 4)");
    // (list-tabulate 4 values) => (0 1 2 3)
    assert_equal("(list-tabulate 4 (lambda (i) i))", "'(0 1 2 3)");
    // (make-list 3 'c)    => (c c c)
    assert_equal("(make-list 3 'c)", "'(c c c)");
    // (xcons '(b c) 'a)   => (a b c)
    assert_equal("(xcons '(b c) 'a)", "'(a b c)");
}

#[test]
fn selectors() {
    assert_equal("(take '(a b c d e) 2)", "'(a b)");
    assert_equal("(drop '(a b c d e) 2)", "'(c d e)");
    assert_equal("(take-right '(a b c d e) 2)", "'(d e)");
    assert_equal("(drop-right '(a b c d e) 2)", "'(a b c)");
    assert_true("(eq? 'c (last '(a b c)))");
    assert_true("(eq? 'c (third '(a b c d e)))");
}

#[test]
fn fold_and_reduce() {
    // (fold + 0 '(1 2 3 4 5)) => 15
    assert_true("(= 15 (fold + 0 '(1 2 3 4 5)))");
    // (fold cons '() '(a b c)) => (c b a)
    assert_equal("(fold cons '() '(a b c))", "'(c b a)");
    // (fold-right cons '() '(a b c)) => (a b c)
    assert_equal("(fold-right cons '() '(a b c))", "'(a b c)");
    // (reduce + 0 '(1 2 3 4 5)) => 15
    assert_true("(= 15 (reduce + 0 '(1 2 3 4 5)))");
}

#[test]
fn mapping() {
    // (map + '(1 2 3) '(10 20 30)) => (11 22 33)  (extended unequal-length map)
    assert_equal("(map + '(1 2 3) '(10 20 30))", "'(11 22 33)");
    // (append-map (lambda (x) (list x x)) '(1 2)) => (1 1 2 2)
    assert_equal("(append-map (lambda (x) (list x x)) '(1 2))", "'(1 1 2 2)");
    // (filter-map (lambda (x) (and (even? x) (* x x))) '(1 2 3 4)) => (4 16)
    assert_equal(
        "(filter-map (lambda (x) (and (even? x) (* x x))) '(1 2 3 4))",
        "'(4 16)",
    );
}

#[test]
fn filtering_and_partition() {
    // (filter even? '(0 7 8 8 43 -4)) => (0 8 8 -4)
    assert_equal("(filter even? '(0 7 8 8 43 -4))", "'(0 8 8 -4)");
    // (remove even? '(0 7 8 8 43 -4)) => (7 43)
    assert_equal("(remove even? '(0 7 8 8 43 -4))", "'(7 43)");
    // (partition odd? '(1 2 3 4 5)) => (1 3 5) and (2 4)
    assert_equal(
        "(call-with-values (lambda () (partition odd? '(1 2 3 4 5))) list)",
        "'((1 3 5) (2 4))",
    );
}

#[test]
fn searching() {
    // (find even? '(3 1 4 1 5 9)) => 4
    assert_true("(= 4 (find even? '(3 1 4 1 5 9)))");
    // (any odd? '(2 4 6 9)) => #t
    assert_true("(any odd? '(2 4 6 9))");
    // (every even? '(2 4 6)) => #t
    assert_true("(every even? '(2 4 6))");
    // (list-index even? '(3 1 4 1 5 9)) => 2
    assert_true("(= 2 (list-index even? '(3 1 4 1 5 9)))");
    // (take-while even? '(2 18 3 10 22 9)) => (2 18)
    assert_equal("(take-while even? '(2 18 3 10 22 9))", "'(2 18)");
    // (count even? '(3 1 4 1 5 9 2 6)) => 3
    assert_true("(= 3 (count even? '(3 1 4 1 5 9 2 6)))");
}

#[test]
fn deletion() {
    // (delete 5 '(1 5 2 5 3)) => (1 2 3)
    assert_equal("(delete 5 '(1 5 2 5 3))", "'(1 2 3)");
    // (delete-duplicates '(a b a c a b c z)) => (a b c z)
    assert_equal("(delete-duplicates '(a b a c a b c z))", "'(a b c z)");
}

#[test]
fn misc_and_sets() {
    // (append-reverse '(3 2 1) '(4 5 6)) => (1 2 3 4 5 6)
    assert_equal("(append-reverse '(3 2 1) '(4 5 6))", "'(1 2 3 4 5 6)");
    // (concatenate '((a b) (c d) (e))) => (a b c d e)
    assert_equal("(concatenate '((a b) (c d) (e)))", "'(a b c d e)");
    // (zip '(1 2 3) '(a b c)) => ((1 a) (2 b) (3 c))
    assert_equal("(zip '(1 2 3) '(a b c))", "'((1 a) (2 b) (3 c))");
    // (lset-intersection eqv? '(a b c d e) '(a e i o u)) => (a e)
    assert_equal(
        "(lset-intersection eqv? '(a b c d e) '(a e i o u))",
        "'(a e)",
    );
    // lset-union membership (order is unspecified, so test by membership)
    assert_true(
        // SRFI-1 `every` returns the last truthy predicate result (here
        // a `memv` tail), not #t, so coerce to a boolean.
        "(let ((u (lset-union eqv? '(a b c d e) '(a e i o u))))
           (and (= 8 (length u))
                (if (every (lambda (x) (memv x u)) '(a b c d e i o u)) #t #f)))",
    );
}

// --- additional coverage for procedure families beyond the worked
// --- examples (SRFI 1 ships no upstream test suite). ---

#[test]
fn unfold_and_tabulate() {
    assert_true(
        "(equal? (unfold (lambda (x) (> x 5)) (lambda (x) (* x x)) (lambda (x) (+ x 1)) 1)
                         '(1 4 9 16 25))",
    );
    assert_true(
        "(equal? (unfold-right zero? (lambda (x) (* x x)) (lambda (x) (- x 1)) 5)
                         '(1 4 9 16 25))",
    );
    assert_true("(equal? (list-tabulate 5 (lambda (i) (* i i))) '(0 1 4 9 16))");
}

#[test]
fn span_break_partition_tails() {
    assert_true(
        "(equal? (call-with-values (lambda () (span even? '(2 4 6 1 3))) list)
                         '((2 4 6) (1 3)))",
    );
    assert_true(
        "(equal? (call-with-values (lambda () (break even? '(1 3 2 4))) list)
                         '((1 3) (2 4)))",
    );
    assert_true("(equal? (find-tail even? '(1 3 5 6 7)) '(6 7))");
    assert_true("(eq? (find-tail even? '(1 3 5)) #f)");
    assert_true("(equal? (take-while odd? '(1 3 5 2 4)) '(1 3 5))");
    assert_true("(equal? (drop-while odd? '(1 3 5 2 4)) '(2 4))");
}

#[test]
fn selectors_and_misc() {
    assert_true("(eq? 'd (fourth '(a b c d e)))");
    assert_true("(eq? 'e (fifth '(a b c d e)))");
    assert_true(
        "(call-with-values (lambda () (car+cdr '(1 2 3)))
                   (lambda (a d) (and (= a 1) (equal? d '(2 3)))))",
    );
    assert_true("(= 5 (length+ '(1 2 3 4 5)))");
    assert_true("(eq? #f (length+ (circular-list 1 2 3)))");
    assert_true("(equal? (concatenate '((1 2) (3) (4 5))) '(1 2 3 4 5))");
    assert_true("(equal? (append-reverse '(3 2 1) '(4 5)) '(1 2 3 4 5))");
}

#[test]
fn deletion_and_remove() {
    assert_true("(equal? (remove even? '(1 2 3 4 5)) '(1 3 5))");
    assert_true("(equal? (delete 3 '(1 2 3 4 3 5)) '(1 2 4 5))");
    assert_true("(equal? (delete-duplicates '(1 2 1 3 2 4)) '(1 2 3 4))");
    assert_true("(equal? (filter-map (lambda (x) (and (even? x) (* x x))) '(1 2 3 4)) '(4 16))");
}

#[test]
fn alist_and_set_operations() {
    assert_true("(equal? (assq 'b '((a . 1) (b . 2) (c . 3))) '(b . 2))");
    assert_true("(equal? (alist-copy '((a . 1) (b . 2))) '((a . 1) (b . 2)))");
    assert_true("(equal? (alist-delete 'b '((a . 1) (b . 2) (c . 3))) '((a . 1) (c . 3)))");
    assert_true("(if (lset<= eqv? '(1 2) '(1 2 3)) #t #f)");
    assert_true("(equal? (lset-adjoin eqv? '(1 2 3) 2 4) '(4 1 2 3))");
    assert_true(
        "(let ((d (lset-difference eqv? '(1 2 3 4 5) '(2 4))))
                   (and (= 3 (length d)) (if (every (lambda (x) (memv x d)) '(1 3 5)) #t #f)))",
    );
}

#[test]
fn folds_and_reductions() {
    assert_true("(= 15 (fold + 0 '(1 2 3 4 5)))");
    assert_true("(equal? (fold cons '() '(1 2 3)) '(3 2 1))");
    assert_true("(equal? (fold-right cons '() '(1 2 3)) '(1 2 3))");
    assert_true("(= 120 (reduce * 1 '(1 2 3 4 5)))");
    assert_true("(= 3 (count even? '(1 2 3 4 5 6)))");
    assert_true("(equal? (append-map (lambda (x) (list x x)) '(1 2)) '(1 1 2 2))");
}
