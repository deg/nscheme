;; Minimal (chibi test) compatibility shim for the chibi-scheme
;; r7rs-tests.scm corpus.
;;
;; Loaded directly into the test harness's global env (not wrapped
;; in `define-library`) so that the macro templates' free
;; identifiers — `$passes`, `$fails`, `$failures`, `$section`, plus
;; the standard `set!`, `cons`, `equal?`, etc. — all resolve to the
;; same global bindings the test harness later reads. The
;; corresponding `(import (chibi test))` in the corpus is a no-op
;; (the import system recognises the library name and skips it
;; because the bindings are already in place).

(define $passes 0)
(define $fails 0)
(define $failures '())  ; list of (section description) lists
(define $section '())   ; stack of test-begin labels

(define (test-begin . args)
  (if (pair? args)
      (set! $section (cons (car args) $section))))

(define (test-end . args)
  (if (pair? $section)
      (set! $section (cdr $section))))

;; chibi's `(chibi test)` allows an optional label as the first arg
;; to test / test-assert / test-error. We accept either form.

;; Result comparison, reproducing chibi's `(chibi test)` `test-equal?`
;; FAITHFULLY (see chibi-scheme lib/chibi/test.scm): an inexact
;; *expected* value matches a result within chibi's default epsilon
;; (`current-test-epsilon` = 1e-5) via the same `approx-equal?` formula;
;; complex values compare part-wise; everything else uses `equal?`.
;;
;; This is NOT a relaxation we invented — chibi's own test oracle is
;; approximate for inexact reals, so "passes the chibi corpus" means the
;; same thing here as it does in chibi (bead nscheme-bc7). The corpus
;; pins a few transcendental results to 15-digit literals (e.g.
;; `(exp 3)` -> 20.0855369231877) that differ from libm in the last
;; ULP; chibi's epsilon is exactly what absorbs those.
(define $test-epsilon 1e-5)  ; chibi current-test-epsilon default

(define ($approx-equal? a b epsilon)
  (cond
   ((> (abs a) (abs b)) ($approx-equal? b a epsilon))
   ((zero? a) (< (abs b) epsilon))
   (else (< (abs (/ (- a b) b)) epsilon))))

;; `(expect actual)` argument order, like chibi's comparator.
(define ($test-equal? expect res)
  (or (equal? expect res)
      (if (real? expect)
          ;; An inexact expected value accepts a result within epsilon.
          (and (inexact? expect)
               (real? res)
               ($approx-equal? expect res $test-epsilon))
          (and (complex? res)
               (complex? expect)
               ($test-equal? (real-part expect) (real-part res))
               ($test-equal? (imag-part expect) (imag-part res))))))

(define-syntax test
  (syntax-rules ()
    ((_ expected expr)
     (let* ((expected-val expected)
            (outcome (guard (e (else (cons 'err e)))
                      (cons 'ok expr))))
       (cond
        ((eq? (car outcome) 'err)
         (set! $fails (+ $fails 1))
         (set! $failures
               (cons (list 'raised 'expr (cdr outcome))
                     $failures)))
        (($test-equal? expected-val (cdr outcome))
         (set! $passes (+ $passes 1)))
        (else
         (set! $fails (+ $fails 1))
         (set! $failures
               (cons (list 'mismatch 'expr
                           'expected expected-val
                           'got (cdr outcome))
                     $failures))))))
    ((_ label expected expr)
     (test expected expr))))

(define-syntax test-assert
  (syntax-rules ()
    ((_ expr)
     (let ((outcome (guard (e (else (cons 'err e)))
                      (cons 'ok expr))))
       (cond
        ((eq? (car outcome) 'err)
         (set! $fails (+ $fails 1))
         (set! $failures
               (cons (list 'assert-raised 'expr (cdr outcome))
                     $failures)))
        ((cdr outcome)
         (set! $passes (+ $passes 1)))
        (else
         (set! $fails (+ $fails 1))
         (set! $failures
               (cons (list 'assert-false 'expr) $failures))))))
    ((_ label expr) (test-assert expr))))

(define-syntax test-error
  (syntax-rules ()
    ((_ expr)
     (let ((outcome (guard (e (else (cons 'err e)))
                      (cons 'ok expr))))
       (cond
        ((eq? (car outcome) 'err)
         (set! $passes (+ $passes 1)))
        (else
         (set! $fails (+ $fails 1))
         (set! $failures
               (cons (list 'expected-error 'expr 'got (cdr outcome))
                     $failures))))))
    ((_ label expr) (test-error expr))))

(define-syntax test-read-error
  (syntax-rules ()
    ((_ src) (test-error (read (open-input-string src))))))

;; test-values: like `test` but the producer returns multiple
;; values that should equal the values produced by the expected
;; expression. We compare via values->list (so single values and
;; packets compare uniformly).
(define-syntax test-values
  (syntax-rules ()
    ((_ expected expr)
     (let ((got (guard (e (else (cons 'err e)))
                  (cons 'ok (values->list expr))))
           (want (values->list expected)))
       (cond
        ((eq? (car got) 'err)
         (set! $fails (+ $fails 1))
         (set! $failures
               (cons (list 'raised 'expr (cdr got)) $failures)))
        (($test-equal? (cdr got) want)
         (set! $passes (+ $passes 1)))
        (else
         (set! $fails (+ $fails 1))
         (set! $failures
               (cons (list 'values-mismatch 'expr
                           'expected want
                           'got (cdr got))
                     $failures))))))))
