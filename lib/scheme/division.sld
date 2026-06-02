;;; (scheme division) — R7RS-large (Tangerine Edition) name for SRFI 141.
;;;
;;; The Tangerine Edition adopts SRFI 141 (Integer division) under the
;;; name (scheme division); this library is a thin re-export of our
;;; vendored (srfi 141).
(define-library (scheme division)
  (import (srfi 141))
  (export ceiling/ ceiling-quotient ceiling-remainder
          floor/ floor-quotient floor-remainder
          truncate/ truncate-quotient truncate-remainder
          round/ round-quotient round-remainder
          euclidean/ euclidean-quotient euclidean-remainder
          balanced/ balanced-quotient balanced-remainder))
