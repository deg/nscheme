;;; (srfi 27) — Sources of random bits (minimal, deterministic LCG).
;;; Enough for test suites that need varied data; not cryptographic.
(define-library (srfi 27)
  (export random-integer random-real
          default-random-source make-random-source
          random-source-randomize! random-source-state-ref
          random-source-state-set! random-source-pseudo-randomize!
          random-source-make-integers random-source-make-reals)
  (import (scheme base))
  (begin
    (define %state 123456789)
    (define (random-integer n)
      (if (and (integer? n) (positive? n))
          (begin
            (set! %state (modulo (+ (* %state 1103515245) 12345) 2147483648))
            (modulo %state n))
          (error "random-integer: needs a positive integer" n)))
    (define (random-real)
      (/ (+ 1.0 (random-integer 1000000000)) 1000000001.0))
    (define default-random-source 'srfi-27-default-source)
    (define (make-random-source) 'srfi-27-source)
    (define (random-source-randomize! s) (if #f #f))
    (define (random-source-pseudo-randomize! s i j) (if #f #f))
    (define (random-source-state-ref s) %state)
    (define (random-source-state-set! s st) (set! %state st))
    (define (random-source-make-integers s) random-integer)
    (define (random-source-make-reals s . o) random-real)))
