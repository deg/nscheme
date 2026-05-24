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

;; Compare two test results: structurally equal for most data, but
;; for numbers fall through to `=` (R7RS mathematical equality)
;; rather than `equal?`'s bit-comparison so that small float
;; round-off — inevitable when the corpus hard-codes 15-digit
;; literals — doesn't cause spurious failures. Lists and vectors
;; recurse element-by-element.
;; Approximate-equal numbers when *either* side is inexact: the
;; corpus often pins float literals to 15 significant digits, which
;; doesn't bit-match an f64 computed from full-precision libm. Allow
;; a small relative tolerance (and an absolute floor for values near
;; zero). Exact-only number comparisons stay strict.
(define $float-tolerance 1e-6)

(define ($numbers-approx-equal? a b)
  (cond
   ((and (exact? a) (exact? b)) (= a b))
   ((and (real? a) (real? b) (nan? a) (nan? b)) #t)
   ((or (and (real? a) (nan? a)) (and (real? b) (nan? b))) #f)
   ((and (real? a) (real? b))
    (let ((diff (abs (- a b)))
          (mag (max (abs a) (abs b))))
      (or (= a b)
          (< diff $float-tolerance)
          (< (/ diff (max mag 1.0)) $float-tolerance))))
   (else
    ;; Complex: compare real and imaginary parts independently.
    (and ($numbers-approx-equal? (real-part a) (real-part b))
         ($numbers-approx-equal? (imag-part a) (imag-part b))))))

(define ($test-equal? a b)
  (cond
   ((and (number? a) (number? b)) ($numbers-approx-equal? a b))
   ((and (pair? a) (pair? b))
    (and ($test-equal? (car a) (car b))
         ($test-equal? (cdr a) (cdr b))))
   ((and (vector? a) (vector? b))
    (and (= (vector-length a) (vector-length b))
         (let loop ((i 0))
           (cond
            ((= i (vector-length a)) #t)
            (($test-equal? (vector-ref a i) (vector-ref b i)) (loop (+ i 1)))
            (else #f)))))
   (else (equal? a b))))

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
        (($test-equal? (cdr outcome) expected-val)
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
