;;; (scheme fixnum) — R7RS-large (Tangerine Edition) name for SRFI 143.
;;;
;;; The Tangerine Edition adopts SRFI 143 under the name (scheme fixnum);
;;; this library is a thin re-export of our vendored (srfi 143).
(define-library (scheme fixnum)
  (import (srfi 143))
  (export fx-width fx-greatest fx-least
          fixnum? fx=? fx<? fx>? fx<=? fx>=?
          fxzero? fxpositive? fxnegative?
          fxodd? fxeven? fxmax fxmin
          fx+ fx- fxneg fx* fxquotient fxremainder
          fxabs fxsquare fxsqrt
          fx+/carry fx-/carry fx*/carry
          fxnot fxand fxior fxxor fxarithmetic-shift
          fxarithmetic-shift-left fxarithmetic-shift-right
          fxbit-count fxlength fxif fxbit-set? fxcopy-bit
          fxfirst-set-bit fxbit-field
          fxbit-field-rotate fxbit-field-reverse))
