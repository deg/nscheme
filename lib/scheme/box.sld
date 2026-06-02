;;; (scheme box) — R7RS-large (Red Edition) name for SRFI 111.
(define-library (scheme box)
  (import (srfi 111))
  (export box box? unbox set-box!))
