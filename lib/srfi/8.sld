;;; (srfi 8) — receive: binding to multiple values.
(define-library (srfi 8)
  (export receive)
  (import (scheme base))
  (begin
    (define-syntax receive
      (syntax-rules ()
        ((receive formals expression body ...)
         (call-with-values (lambda () expression)
           (lambda formals body ...)))))))
