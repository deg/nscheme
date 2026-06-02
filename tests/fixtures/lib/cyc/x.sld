;; Fixture: mutually-recursive libraries (cyc x) <-> (cyc y). Importing
;; either must fail cleanly with a circular-dependency error rather than
;; overflowing the stack (loader cycle guard, bead nscheme-9q5).
(define-library (cyc x)
  (export x-val)
  (import (scheme base) (cyc y))
  (begin (define x-val 1)))
