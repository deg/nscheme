;; Minimal (chibi test) compatibility shim for the chibi-scheme
;; r7rs-tests.scm corpus.
;;
;; Provides just enough of the (chibi test) surface for the corpus
;; to load and run. Pass/fail counters live as top-level mutable
;; variables that the test macros set! directly. (We can't route
;; counts through a closure-bound helper because library imports
;; bind a *copy* of the value, not a shared cell — see ADR 0003 on
;; nscheme v1's macro hygiene model. Inlining set! into each macro
;; ensures the mutation lands in the call-site env, where the
;; importer's $passes / $fails actually live.)

(define-library (chibi test)
  (export test-begin test-end test test-assert test-error test-read-error
          $passes $fails $failures $section)
  (import (scheme base))
  (begin

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

    ;; Each macro expands to inline guard + set!, so the
    ;; mutations target the call-site $passes / $fails.

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
            ((equal? (cdr outcome) expected-val)
             (set! $passes (+ $passes 1)))
            (else
             (set! $fails (+ $fails 1))
             (set! $failures
                   (cons (list 'mismatch 'expr
                               'expected expected-val
                               'got (cdr outcome))
                         $failures))))))))

    (define-syntax test-assert
      (syntax-rules ()
        ((_ expr)
         (let ((outcome (guard (e (else (cons 'err e)))
                          (cons 'ok expr))))
           (cond
            ((eq? (car outcome) 'err)
             (set! $fails (+ $fails 1)))
            ((cdr outcome)
             (set! $passes (+ $passes 1)))
            (else
             (set! $fails (+ $fails 1))))))))

    (define-syntax test-error
      (syntax-rules ()
        ((_ expr)
         (let ((outcome (guard (e (else (cons 'err e)))
                          (cons 'ok expr))))
           (if (eq? (car outcome) 'err)
               (set! $passes (+ $passes 1))
               (set! $fails (+ $fails 1)))))))

    (define-syntax test-read-error
      (syntax-rules ()
        ((_ src) (test-error (read (open-input-string src))))))
    ))
