;;; (scheme flonum) — R7RS-large (Tangerine Edition) name for SRFI 144.
;;;
;;; The Tangerine Edition adopts SRFI 144 under the name (scheme flonum);
;;; this library is a thin re-export of our vendored (srfi 144).
(define-library (scheme flonum)
  (import (srfi 144))
  (export

   ;; Mathematical Constants

   fl-e
   fl-1/e
   fl-e-2
   fl-e-pi/4
   fl-log2-e
   fl-log10-e
   fl-log-2
   fl-1/log-2
   fl-log-3
   fl-log-pi
   fl-log-10
   fl-1/log-10
   fl-pi
   fl-1/pi
   fl-2pi
   fl-pi/2
   fl-pi/4
   fl-2/sqrt-pi
   fl-pi-squared
   fl-degree
   fl-2/pi
   fl-sqrt-2
   fl-sqrt-3
   fl-sqrt-5
   fl-sqrt-10
   fl-1/sqrt-2
   fl-cbrt-2
   fl-cbrt-3
   fl-4thrt-2
   fl-phi
   fl-log-phi
   fl-1/log-phi
   fl-euler
   fl-e-euler
   fl-sin-1
   fl-cos-1
   fl-gamma-1/2
   fl-gamma-1/3
   fl-gamma-2/3

   ;; Implementation Constants

   fl-greatest
   fl-least
   fl-epsilon
   fl-fast-fl+*
   fl-integer-exponent-zero
   fl-integer-exponent-nan

   ;; Constructors

   flonum
   fladjacent
   flcopysign
   make-flonum

   ;; Accessors

   flinteger-fraction
   flexponent
   flinteger-exponent
   flnormalized-fraction-exponent
   flsign-bit

   ;; Predicates

   flonum?
   fl=?
   fl<?
   fl>?
   fl<=?
   fl>=?
   flunordered?
   flmax
   flmin
   flinteger?
   flzero?
   flpositive?
   flnegative?
   flodd?
   fleven?
   flfinite?
   flinfinite?
   flnan?
   flnormalized?
   fldenormalized?

   ;; Arithmetic

   fl+
   fl*
   fl+*
   fl-
   fl/
   flabs
   flabsdiff
   flposdiff
   flsgn
   flnumerator
   fldenominator
   flfloor
   flceiling
   flround
   fltruncate

   ;; Exponents and logarithms

   flexp
   flexp2
   flexp-1
   flsquare
   flsqrt
   flcbrt
   flhypot
   flexpt
   fllog
   fllog1+
   fllog2
   fllog10
   make-fllog-base

   ;; Trigonometric functions

   flsin
   flcos
   fltan
   flasin
   flacos
   flatan
   flsinh
   flcosh
   fltanh
   flasinh
   flacosh
   flatanh

   ;; Integer division

   flquotient
   flremainder
   flremquo

   ;; Special functions

   flgamma
   flloggamma
   flfirst-bessel
   flsecond-bessel
   flerf
   flerfc
   ))
