//! R7RS-small conformance test suite.
//!
//! Each test is named after the relevant section of the R7RS-small
//! report (<https://small.r7rs.org/attachment/r7rs.pdf>). Tests are
//! formulated as `(source, expected)` pairs; `expected` is the
//! canonical R7RS-printed form of the expected value.
//!
//! The full conformance corpus from chibi-scheme / other implementations
//! is *not* vendored here for license reasons; this is an in-house
//! suite derived from the spec text. As more R7RS forms are implemented,
//! tests are added here rather than to module-local test files.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::eval_source;

/// Run `source` and compare the printed result against `expected`.
/// Comparison is via `format!("{value:?}")` which matches R7RS `write`.
fn check(source: &str, expected: &str) {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    let result = match eval_source(source, env) {
        Ok(v) => format!("{v:?}"),
        Err(e) => panic!("evaluation error: {e}\n source: {source}"),
    };
    assert_eq!(result, expected, "\n source: {source}");
}

// ---------------------------------------------------------------------
// §4 Expressions
// ---------------------------------------------------------------------

#[test]
fn r7rs_4_1_2_literal_expressions() {
    check("(quote a)", "a");
    check("(quote #(a b c))", "#(a b c)");
    check("'a", "a");
    check("'()", "()");
    check("'(+ 1 2)", "(+ 1 2)");
    check("'(quote a)", "(quote a)");
}

#[test]
fn r7rs_4_1_3_procedure_calls() {
    check("(+ 3 4)", "7");
    check("((if #f + *) 3 4)", "12");
}

#[test]
fn r7rs_4_1_4_procedures() {
    check("((lambda (x) (+ x x)) 4)", "8");
    check(
        "(define reverse-subtract (lambda (x y) (- y x)))
         (reverse-subtract 7 10)",
        "3",
    );
}

#[test]
fn r7rs_4_1_5_conditionals() {
    check("(if (> 3 2) 'yes 'no)", "yes");
    check("(if (> 2 3) 'yes 'no)", "no");
    check("(if (> 3 2) (- 3 2) (+ 3 2))", "1");
}

#[test]
fn r7rs_4_1_6_assignments() {
    check(
        "(define x 2)
         (+ x 1)
         (set! x 4)
         (+ x 1)",
        "5",
    );
}

#[test]
fn r7rs_4_2_1_conditional_cond() {
    check("(cond ((> 3 2) 'greater) ((< 3 2) 'less))", "greater");
    check(
        "(cond ((> 3 3) 'greater) ((< 3 3) 'less) (else 'equal))",
        "equal",
    );
    check("(cond ((assv 'b '((a 1) (b 2))) => cdr) (else #f))", "(2)");
}

#[test]
fn r7rs_4_2_1_case() {
    check(
        "(case (* 2 3)
           ((2 3 5 7) 'prime)
           ((1 4 6 8 9) 'composite))",
        "composite",
    );
    check(
        "(case (car '(c d))
           ((a) 'a)
           ((b) 'b)
           (else 'other))",
        "other",
    );
}

#[test]
fn r7rs_4_2_1_and_or() {
    check("(and (= 2 2) (> 2 1))", "#t");
    check("(and (= 2 2) (< 2 1))", "#f");
    check("(and 1 2 'c '(f g))", "(f g)");
    check("(and)", "#t");
    check("(or (= 2 2) (> 2 1))", "#t");
    check("(or #f #f #f)", "#f");
    check("(or (memq 'b '(a b c)) (/ 3 0))", "(b c)");
}

#[test]
fn r7rs_4_2_1_when_unless() {
    check("(when (= 1 1) 'ok)", "ok");
    check("(unless (= 1 2) 'ok)", "ok");
}

#[test]
fn r7rs_4_2_2_let_family() {
    check("(let ((x 2) (y 3)) (* x y))", "6");
    check(
        "(let ((x 2) (y 3))
           (let ((x 7) (z (+ x y)))
             (* z x)))",
        "35",
    );
    check(
        "(let ((x 2) (y 3))
           (let* ((x 7) (z (+ x y)))
             (* z x)))",
        "70",
    );
    check(
        "(letrec ((even? (lambda (n) (if (zero? n) #t (odd? (- n 1)))))
                  (odd?  (lambda (n) (if (zero? n) #f (even? (- n 1))))))
           (even? 88))",
        "#t",
    );
}

#[test]
fn r7rs_4_2_3_sequencing_begin() {
    check(
        "(define x 0)
         (begin (set! x 5) (+ x 1))",
        "6",
    );
}

#[test]
fn r7rs_4_2_4_iteration_do() {
    check(
        "(do ((vec (make-vector 5))
              (i 0 (+ i 1)))
             ((= i 5) vec)
           (vector-set! vec i i))",
        "#(0 1 2 3 4)",
    );
    check(
        "(let loop ((numbers '(3 -2 1 6 -5))
                    (nonneg '())
                    (neg '()))
           (cond ((null? numbers) (list nonneg neg))
                 ((>= (car numbers) 0)
                  (loop (cdr numbers) (cons (car numbers) nonneg) neg))
                 (else
                  (loop (cdr numbers) nonneg (cons (car numbers) neg)))))",
        "((6 1 3) (-5 -2))",
    );
}

#[test]
fn r7rs_4_2_6_quasiquotation() {
    check("`(list ,(+ 1 2) 4)", "(list 3 4)");
    check(
        "(let ((name 'a)) `(list ,name ',name))",
        "(list a (quote a))",
    );
    check("`(1 2 ,@(list 3 4) 5)", "(1 2 3 4 5)");
}

// ---------------------------------------------------------------------
// §6 Standard procedures
// ---------------------------------------------------------------------

#[test]
fn r7rs_6_1_equivalence_predicates() {
    check("(eqv? 'a 'a)", "#t");
    check("(eqv? 'a 'b)", "#f");
    check("(eqv? 2 2)", "#t");
    check("(eqv? '() '())", "#t");
    check("(eq? 'a 'a)", "#t");
    check("(eq? (list 'a) (list 'a))", "#f");
    check("(equal? 'a 'a)", "#t");
    check("(equal? '(a) '(a))", "#t");
    check("(equal? \"abc\" \"abc\")", "#t");
}

#[test]
fn r7rs_6_2_arithmetic() {
    check("(+ 3 4)", "7");
    check("(+ 3)", "3");
    check("(+)", "0");
    check("(* 4)", "4");
    check("(*)", "1");
    check("(- 3 4 5)", "-6");
    check("(- 3)", "-3");
    check("(/ 3 4 5)", "3/20");
    check("(/ 3)", "1/3");
    check("(abs -7)", "7");
    check("(modulo 13 4)", "1");
    check("(modulo -13 4)", "3");
    check("(remainder 13 4)", "1");
    check("(remainder -13 4)", "-1");
    check("(quotient 13 4)", "3");
    check("(quotient -13 4)", "-3");
}

#[test]
fn r7rs_6_2_numeric_predicates() {
    check("(zero? 0)", "#t");
    check("(zero? 1)", "#f");
    check("(positive? 1)", "#t");
    check("(positive? -1)", "#f");
    check("(negative? -1)", "#t");
    check("(integer? 3)", "#t");
    check("(integer? 3.0)", "#t");
    check("(exact? 3)", "#t");
    check("(exact? 3.0)", "#f");
}

#[test]
fn r7rs_6_3_booleans() {
    check("(not #t)", "#f");
    check("(not 3)", "#f");
    check("(not (list 3))", "#f");
    check("(not #f)", "#t");
    check("(not '())", "#f");
    check("(boolean? #f)", "#t");
    check("(boolean? 0)", "#f");
}

#[test]
fn r7rs_6_4_pairs_and_lists() {
    check("(pair? '(a . b))", "#t");
    check("(pair? '(a b c))", "#t");
    check("(pair? '())", "#f");
    check("(cons 'a '())", "(a)");
    check("(cons '(a) '(b c d))", "((a) b c d)");
    check("(car '(a b c))", "a");
    check("(car '((a) b c d))", "(a)");
    check("(cdr '((a) b c d))", "(b c d)");
    check("(list? '(a b c))", "#t");
    check("(list? '())", "#t");
    check("(list? '(a . b))", "#f"); // improper list
}

#[test]
fn r7rs_6_4_list_ops() {
    check("(length '(a b c))", "3");
    check("(length '(a (b) (c d e)))", "3");
    check("(length '())", "0");
    check("(append '(x) '(y))", "(x y)");
    check("(append '(a) '(b c d))", "(a b c d)");
    check("(append)", "()");
    check("(reverse '(a b c))", "(c b a)");
    check("(reverse '(a (b c) d (e (f))))", "((e (f)) d (b c) a)");
    check("(list-ref '(a b c d) 2)", "c");
    check("(memq 'a '(a b c))", "(a b c)");
    check("(memq 'b '(a b c))", "(b c)");
    check("(memq 'a '(b c d))", "#f");
    check("(member (list 'a) '(b (a) c))", "((a) c)");
    check("(assq 'b '((a 1) (b 2) (c 3)))", "(b 2)");
}

#[test]
fn r7rs_6_5_symbols() {
    check("(symbol? 'foo)", "#t");
    check("(symbol? (car '(a b)))", "#t");
    check("(symbol? \"bar\")", "#f");
    check("(symbol? 'nil)", "#t");
    check("(symbol? #f)", "#f");
    check("(symbol->string 'flying-fish)", "\"flying-fish\"");
    check("(string->symbol \"mISSISSIppi\")", "mISSISSIppi");
}

#[test]
fn r7rs_6_6_characters() {
    check(r"(char? #\a)", "#t");
    check(r"(char? 1)", "#f");
    check(r"(char->integer #\A)", "65");
    check(r"(integer->char 97)", "#\\a");
    check(r"(char<? #\A #\B)", "#t");
    check(r"(char-upcase #\a)", "#\\A");
}

#[test]
fn r7rs_6_7_strings() {
    check("(string? \"hi\")", "#t");
    check("(string-length \"abcdef\")", "6");
    check("(string-ref \"abcdef\" 0)", "#\\a");
    check("(substring \"abcdef\" 1 4)", "\"bcd\"");
    check(
        "(string-append \"hello\" \", \" \"world\")",
        "\"hello, world\"",
    );
    check("(string->list \"ab\")", "(#\\a #\\b)");
    check(r"(list->string '(#\h #\i))", "\"hi\"");
}

#[test]
fn r7rs_6_8_vectors() {
    check("(vector 'a 'b 'c)", "#(a b c)");
    check("(make-vector 3 0)", "#(0 0 0)");
    check("(vector-length #(1 2 3))", "3");
    check("(vector-ref #(a b c) 0)", "a");
    check("(vector->list #(a b c))", "(a b c)");
    check("(list->vector '(1 2 3))", "#(1 2 3)");
}

#[test]
fn r7rs_6_10_control_features() {
    check(
        "(call-with-current-continuation
           (lambda (exit)
             (for-each (lambda (x)
                         (if (negative? x) (exit x)))
                       '(54 0 37 -3 245 19))
             #t))",
        "-3",
    );
    check("(apply + '(1 2 3))", "6");
    check("(apply + 1 2 '(3 4 5))", "15");
}

#[test]
fn r7rs_6_11_exceptions() {
    check(
        "(guard (e ((symbol? e) (list 'symbol e))
                   ((number? e) (list 'number e)))
           (raise 'oops))",
        "(symbol oops)",
    );
    check(
        "(guard (e ((symbol? e) (list 'symbol e))
                   ((number? e) (list 'number e)))
           (raise 99))",
        "(number 99)",
    );
    check(
        "(guard (e (else 'caught))
           (+ 1 2))",
        "3",
    );
}

// ---------------------------------------------------------------------
// §3.5 Tail position — one canonical fact
// ---------------------------------------------------------------------

#[test]
fn r7rs_3_5_proper_tail_calls() {
    // The classic R7RS tail-call test from the report itself.
    check(
        "(define (loop n)
           (if (= n 0) 'done (loop (- n 1))))
         (loop 200000)",
        "done",
    );
}

// ---------------------------------------------------------------------
// §4.3 syntax-rules
// ---------------------------------------------------------------------

#[test]
fn r7rs_4_3_2_syntax_rules() {
    // Canonical or-via-macro from R7RS itself.
    check(
        "(define-syntax my-or
           (syntax-rules ()
             ((my-or) #f)
             ((my-or e) e)
             ((my-or e1 e2 ...) (let ((t e1)) (if t t (my-or e2 ...))))))
         (my-or #f #f 7 #f)",
        "7",
    );
}
