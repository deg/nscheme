;;; (srfi 111) — Boxes.
;;;
;;; SRFI 111 (John Cowan, 2015) defines a box as a single-slot mutable
;;; container. The whole library is the canonical record definition from
;;; the SRFI document; there is no separate reference .scm to vendor.
(define-library (srfi 111)
  (export box box? unbox set-box!)
  (import (scheme base))
  (begin
    (define-record-type box-type
      (box value)
      box?
      (value unbox set-box!))))
