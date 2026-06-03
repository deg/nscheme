;;; (srfi 144) — Flonums.
;;;
;;; Vendored from the SRFI 144 reference implementation by William D
;;; Clinger (2016/2017), MIT licence (text preserved below). Flattened
;;; into a single file: upstream `144.sld` pulls in five body files via
;;; (include "144.constants.scm" …), but nscheme resolves `include`
;;; relative to the process directory rather than the library file, so
;;; the body files are inlined here inside `begin` instead of included.
;;;
;;; Upstream's library form selects between (rnrs arithmetic flonums)
;;; and a portable fallback via cond-expand, and between a C FFI
;;; (Larceny only) and pure-Scheme stubs. nscheme has neither
;;; (rnrs arithmetic flonums) nor Larceny's FFI, so both else branches
;;; are taken: the R6RS-flonum layer is faked from generic arithmetic
;;; (144.r6rs.scm) and the C-library hooks resolve to error stubs. The
;;; cond-expand forms are preserved verbatim; nscheme evaluates them and
;;; chooses the same branches.
;;;
;;; SPDX-License-Identifier: MIT
;;; Copyright (C) William D Clinger (2016). All Rights Reserved.
;;;
;;; Permission is hereby granted, free of charge, to any person
;;; obtaining a copy of this software and associated documentation
;;; files (the "Software"), to deal in the Software without
;;; restriction, including without limitation the rights to use,
;;; copy, modify, merge, publish, distribute, sublicense, and/or
;;; sell copies of the Software, and to permit persons to whom the
;;; Software is furnished to do so, subject to the following
;;; conditions:
;;;
;;; The above copyright notice and this permission notice shall be
;;; included in all copies or substantial portions of the Software.
;;;
;;; THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
;;; EXPRESS OR IMPLIED. See the SRFI 144 document for the full text.

(define-library (srfi 144)

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

   ;; Exponents and logarithsm

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
   )

  (import (scheme base)
          (scheme inexact))

  ;; Use (rnrs arithmetic flonums) if that library is available.

  (cond-expand
   ((library (rnrs arithmetic flonums))
    (import (except (rnrs arithmetic flonums)
                    flmax flmin flnumerator fldenominator)
            (prefix (only (rnrs arithmetic flonums)
                          flnumerator fldenominator)
                    r6rs:)))
   (else
    (import (scheme complex))))

  ;; Use an FFI if one is available.

  (cond-expand
   ((and larceny i386 unix (or gnu-linux darwin))
    (import (rename (primitives r5rs:require)
                    (r5rs:require require))
            (primitives foreign-procedure)))
   (else))

  ;; The five upstream body files (144.constants.scm, 144.body0.scm,
  ;; 144.r6rs.scm, 144.body.scm, 144.special.scm) are inlined verbatim
  ;; below, in the load order the upstream .sld uses. They share one
  ;; `begin` so internal forward references (resolved lazily inside
  ;; lambda bodies) work as the originals do.

  (begin

;;; ==================================================================
;;; 144.constants.scm
;;; ==================================================================

;;; This is derived from the srfi-144-constants.scm file at
;;; https://github.com/scheme-requests-for-implementation/srfi-144

(define fl-e          2.7182818284590452353602874713526624977572) ; e
(define fl-1/e        0.3678794411714423215955237701614608674458) ; 1/e
(define fl-e-2        7.3890560989306502272304274605750078131803) ; e^2
(define fl-e-pi/4     2.1932800507380154565597696592787382234616) ; e^(pi/4)
(define fl-log2-e     1.44269504088896340735992468100189214)      ; log_2(e)
(define fl-log10-e    0.434294481903251827651128918916605082)     ; log_10(e)
(define fl-log-2      0.6931471805599453094172321214581765680755) ; ln(2)
(define fl-1/log-2    1.4426950408889634073599246810018921374266) ; 1/ln(2)
(define fl-log-3      1.0986122886681096913952452369225257046475) ; ln(3)
(define fl-log-pi     1.1447298858494001741434273513530587116473) ; ln(pi)
(define fl-log-10     2.3025850929940456840179914546843642076011) ; ln(10)
(define fl-1/log-10   0.4342944819032518276511289189166050822944) ; 1/ln(10)
(define fl-pi         3.1415926535897932384626433832795028841972) ; pi
(define fl-1/pi       0.3183098861837906715377675267450287240689) ; 1/pi
(define fl-2pi        6.2831853071795862319959269370883703231812) ; pi * 2
(define fl-pi/2       1.57079632679489661923132169163975144)      ; pi/2
(define fl-2/pi       0.636619772367581343075535053490057448)     ; 2/pi
(define fl-pi/4       0.785398163397448309615660845819875721)     ; pi/4
(define fl-2/sqrt-pi  1.12837916709551257389615890312154517)      ; 2/sqrt(pi)
(define fl-sqrt-pi    1.7724538509055160272981674833411451827975) ; sqrt(pi)
(define fl-pi-squared 9.8696044010893586188344909998761511353137) ; pi^2
(define fl-degree     0.0174532925199432957692369076848861271344) ; pi/180
(define fl-gamma-1/2  1.7724538509055160272981674833411451827975) ; gamma(1/2)
(define fl-gamma-1/3  2.6789385347077476336556929409746776441287) ; gamma(1/3)
(define fl-gamma-2/3  1.3541179394264004169452880281545137855193) ; gamma(2/3)
(define fl-sqrt-2     1.4142135623730950488016887242096980785697) ; sqrt(2)
(define fl-sqrt-3     1.7320508075688772935274463415058723669428) ; sqrt(3)
(define fl-sqrt-5     2.2360679774997896964091736687312762354406) ; sqrt(5)
(define fl-sqrt-10    3.1622776601683793319988935444327185337196) ; sqrt(10)
(define fl-cbrt-2     1.2599210498948731647672106072782283505703) ; cubert(2)
(define fl-cbrt-3     1.4422495703074083823216383107801095883919) ; cubert(3)
(define fl-4thrt-2    1.1892071150027210667174999705604759152930) ; fourthrt(2)
(define fl-1/sqrt-2   0.7071067811865475244008443621048490392848) ; 1/sqrt(2)
(define fl-phi        1.6180339887498948482045868343656381177203) ; phi
(define fl-log-phi    0.4812118250596034474977589134243684231352) ; ln(phi)
(define fl-1/log-phi  2.0780869212350275376013226061177957677422) ; 1/ln(phi)
(define fl-euler      0.5772156649015328606065120900824024310422) ; euler
(define fl-e-euler    1.7810724179901979852365041031071795491696) ; e^euler
(define fl-sin-1      0.8414709848078965066525023216302989996226) ; sin(1)
(define fl-cos-1      0.5403023058681397174009366074429766037323) ; cos(1)

;;; ==================================================================
;;; 144.body0.scm  (private but portable code)
;;; ==================================================================

(define FIXME 'FIXME)

(define precision-bits    ; IEEE double has 53 bits of precision
  (let loop ((bits 0)
             (x 1.0))
    (if (= x (+ x 1.0))
        bits
        (loop (+ bits 1)
              (* 2.0 x)))))

(define (check-flonum! name x)
  (if (not (flonum? x))
      (error (string-append "non-flonum argument passed to "
                            (symbol->string name))
             x)))

;;; Given a symbol naming a flonum procedure and a generic operation,
;;; returns a flonum procedure that restricts the generic operation
;;; to flonum arguments and result.

(define (flop1 name op)
  (lambda (x)
    (check-flonum! name x)
    (let ((result (op x)))
      (if (not (flonum? result))
          (error (string-append "non-flonum result from "
                                (symbol->string name))
                 result))
      result)))

(define (flop2 name op)
  (lambda (x y)
    (check-flonum! name x)
    (check-flonum! name y)
    (let ((result (op x y)))
      (if (not (flonum? result))
          (error (string-append "non-flonum result from "
                                (symbol->string name))
                 result))
      result)))

(define (flop3 name op)
  (lambda (x y z)
    (check-flonum! name x)
    (check-flonum! name y)
    (check-flonum! name z)
    (let ((result (op x y z)))
      (if (not (flonum? result))
          (error (string-append "non-flonum result from "
                                (symbol->string name))
                 result))
      result)))

;;; Given a flonum x and a list of flonum coefficients for a polynomial,
;;; in order of increasing degree, returns the value of the polynomial at x.

(define (polynomial-at x coefs)
  (if (null? coefs)
      0.0
      (fl+ (car coefs)
           (fl* x (polynomial-at x (cdr coefs))))))

;;; This uses Simpson's rule.

(define (definite-integral lower upper f . rest)
  (let* ((range (fl- upper lower))
         (kmax (if (or (null? rest)
                       (not (and (exact-integer? (car rest))
                                 (even? (car rest))
                                 (positive? (car rest)))))
                   1024 ; FIXME: must be even, should be power of 2
                   (car rest)))
         (n2 (inexact kmax))
         (h (fl/ range n2)))
    (define (loop k n sum)    ; n = (inexact k)
      (cond ((= k 0)
             (loop 1 1.0 (f lower)))
            ((= k kmax)
             (fl+ sum (f upper)))
            (else
             (let ((fn (f (+ lower (fl/ (fl* n range) n2)))))
               (loop (+ k 1)
                     (fl+ n 1.0)
                     (fl+ sum (fl* (if (even? k) 2.0 4.0) fn)))))))
    (fl/ (fl* h (loop 0 0.0 0.0))
         3.0)))

;;; Given x between x0 and x1, interpolates between f0 and f1.
;;; Can also extrapolate.

(define (interpolate x x0 x1 f0 f1)
  (let ((delta (fl- x1 x0)))
    (fl+ (fl* (fl/ (fl- x1 x) delta) f0)
         (fl* (fl/ (fl- x x0) delta) f1))))

(define (iota n)
  (do ((n (- n 1) (- n 1))
       (x '() (cons n x)))
      ((< n 0) x)))

;;; Given a exact non-negative integer, returns its factorial.

(define (fact x)
  (if (zero? x)
      1
      (* x (fact (- x 1)))))

;;; Given a non-negative integral flonum x, returns its factorial.

(define (factorial x)
  (if (flzero? x)
      1.0
      (fl* x (factorial (fl- x 1.0)))))

;;; ==================================================================
;;; 144.r6rs.scm  (fake (rnrs arithmetic flonums) over generic arith)
;;; ==================================================================

;;; Private.

(define (flop0-or-more name op)
  (lambda args
    (for-each (lambda (x) (check-flonum! name x)) args)
    (flonum (apply op args))))

(define (flop1-or-more name op)
  (lambda (x . args)
    (for-each (lambda (x) (check-flonum! name x)) (cons x args))
    (flonum (apply op x args))))

(define (flop2-or-more name op)
  (lambda (x y . args)
    (for-each (lambda (x) (check-flonum! name x)) (cons x (cons y args)))
    (flonum (apply op x y args))))

(define (flpred1 name op)
  (lambda (x)
    (check-flonum! name x)
    (op x)))

(define (flpred2-or-more name op)
  (lambda (x y . args)
    (for-each (lambda (x) (check-flonum! name x)) (cons x (cons y args)))
    (apply op x y args)))

;;; Exported.

(define (flonum? x)
  (and (number? x)
       (real? x)        ; implies (exact? (imag-part x))
       (inexact? x)))

(define fl=?  (flpred2-or-more 'fl=? =))
(define fl<?  (flpred2-or-more 'fl<? <))
(define fl>?  (flpred2-or-more 'fl>? >))
(define fl<=? (flpred2-or-more 'fl<=? <=))
(define fl>=? (flpred2-or-more 'fl>=? >=))

(define flinteger?  (flpred1 'flinteger?  integer?))
(define flzero?     (flpred1 'flzero?     zero?))
(define flpositive? (flpred1 'flpositive? positive?))
(define flnegative? (flpred1 'flnegative? negative?))
(define flodd?      (flpred1 'flodd?      odd?))
(define fleven?     (flpred1 'fleven?     even?))
(define flfinite?   (flpred1 'flfinite?   finite?))
(define flinfinite? (flpred1 'flinfinite? infinite?))
(define flnan?      (flpred1 'flnan?      nan?))

(define fl+        (flop0-or-more 'fl+ +))
(define fl*        (flop0-or-more 'fl* *))
(define fl-        (flop1-or-more 'fl- -))
(define fl/        (flop1-or-more 'fl/ /))

(define flabs      (flop1 'flabs      abs))

(define flfloor    (flop1 'flfloor    floor))
(define flceiling  (flop1 'flceiling  ceiling))
(define flround    (flop1 'flround    round))
(define fltruncate (flop1 'fltruncate truncate))

(define r6rs:flnumerator   numerator)
(define r6rs:fldenominator denominator)

(define flexp      (flop1 'flexp  exp))
(define flsqrt     (flop1 'flsqrt sqrt))
(define flexpt     (flop2 'flexpt expt))
(define fllog      (flop1 'fllog  log))
(define flsin      (flop1 'flsin  sin))
(define flcos      (flop1 'flcos  cos))
(define fltan      (flop1 'fltan tan))
(define flasin     (flop1 'flasin asin))
(define flacos     (flop1 'flacos acos))
(define flatan     (flop1-or-more 'flatan atan)) ; FIXME 1 or 2 arguments

;;; ==================================================================
;;; 144.body.scm
;;; ==================================================================

;; Implementation Constants

;; Upstream computes these three by load-time loops (doubling /
;; halving until overflow / underflow). Under the tree-walking
;; interpreter that costs ~18s per load, so they are inlined as the
;; exact IEEE-754 double values the loops converge to: the largest
;; finite double, the smallest positive subnormal, and machine epsilon
;; (bead nscheme-oeg.3.1).
(define fl-greatest 1.7976931348623157e308)
(define fl-least 5e-324)
(define fl-epsilon 2.220446049250313e-16)

(define fl-integer-exponent-zero                ; arbitrary
  (exact (- (log fl-least 2.0) 1.0)))

(define fl-integer-exponent-nan                 ; arbitrary
  (- fl-integer-exponent-zero 1))

;;; Constructors

; Implements post-finalization note 1
(define (flonum x)
  (if (real? x)
      (inexact x)
      +nan.0))

(define fladjacent
  (flop2 'fladjacent
         (lambda (x y)
           (define (loop y)
             (let* ((y3 (fl+ (fl* 0.999755859375 x) (fl* 0.000244140625 y))))
               (cond ((fl<? x y3 y)
                      (loop y3))
                     ((fl<? y y3 x)
                      (loop y3))
                     (else
                      (loop2 y)))))
           (define (loop2 y)
             (let* ((y2 (fl/ (fl+ x y) 2.0))
                    (y2 (if (flinfinite? y2)
                            (fl+ (fl* 0.5 x) (fl* 0.5 y))
                            y2)))
               (cond ((fl=? x y2)
                      y)
                     ((fl=? y y2)
                      y)
                     (else
                      (loop2 y2)))))
           (cond ((flinfinite? x)
                  (cond ((fl<? x y) (fl- fl-greatest))
                        ((fl>? x y) fl-greatest)
                        (else x)))
                 ((fl=? x y)
                  x)
                 ((flzero? x)
                  (if (flpositive? y)
                      fl-least
                      (fl- fl-least)))
                 ((fl<? x y)
                  (loop (flmin y
                               fl-greatest
                               (flmax (* 2.0 x)
                                      (* 0.5 x)))))
                 ((fl>? x y)
                  (loop (flmax y
                               (fl- fl-greatest)
                               (flmin (* 2.0 x)
                                      (* 0.5 x)))))
                 (else    ; x or y is a NaN
                  x)))))

(define flcopysign
  (flop2 'flcopysign
         (lambda (x y)
           (if (= (flsign-bit x) (flsign-bit y))
               x
               (fl- x)))))

(define (make-flonum x n)
  (let ((y (expt 2.0 n)))
    (cond ((or (not (flonum? x))
               (not (exact-integer? n)))
           (error "bad arguments to make-flonum" x n))
          ((finite? y)
           (* x y))
          (else
           (inexact (* (exact x) (expt 2 n)))))))

;;; Accessors

(define (flinteger-fraction x)
  (check-flonum! 'flinteger-fraction x)
  (let* ((result1 (fltruncate x))
         (result2 (fl- x result1)))
    (values result1 result2)))

(define (flexponent x)
  (floor (fllog2 (flabs x))))

(define (flinteger-exponent x)
  (exact (flexponent x)))

(define (flnormalized-fraction-exponent x)
  (define (return result1 result2)
    (cond ((fl<? result1 0.5)
           (values (fl* 2.0 result1) (- result2 1)))
          ((fl>=? result1 1.0)
           (values (fl* 0.5 result1) (+ result2 1)))
          (else
           (values result1 result2))))
  (check-flonum! 'flnormalized-fraction-exponent x)
  (cond ((flnan? x)    ; unspecified for NaN
         (values x 0))
        ((fl<? x 0.0)
         (call-with-values
          (lambda () (flnormalized-fraction-exponent (fl- x)))
          (lambda (y n)
            (values (fl- y) n))))
        ((fl=? x 0.0)    ; unspecified for 0.0
         (values 0.0 0))
        ((flinfinite? x)
         (values 0.5 (+ 3 (exact (round (fllog2 fl-greatest))))))
        ((flnormalized? x)
         (let* ((result2 (exact (flround (fllog2 x))))
                (result2 (if (integer? result2)
                             result2
                             (round result2)))
                (two^result2 (inexact (expt 2.0 result2))))
           (if (flinfinite? two^result2)
               (call-with-values
                (lambda () (flnormalized-fraction-exponent (fl/ x 4.0)))
                (lambda (y n)
                  (values y (+ n 2))))
               (return (fl/ x two^result2) result2))))
        (else
         (let* ((k (+ 2 precision-bits))
                (two^k (expt 2 k)))
           (call-with-values
            (lambda ()
              (flnormalized-fraction-exponent (fl* x (inexact two^k))))
            (lambda (y n)
              (return y (- n k))))))))

(define (flsign-bit x)
  (check-flonum! 'flsign-bit x)
  (cond ((fl<? x 0.0)
         1)
        ((eqv? x -0.0)
         1)
        (else
         0)))

;;; Predicates

(define (flunordered? x y)
  (or (flnan? x) (flnan? y)))

;;; incompatible with (rnrs arithmetic flonums) in zero-argument case

(define flmax
  (let ((flmax2 (flop2 'flmax max)))
    (lambda args
      (cond ((null? args)
             -inf.0)
            ((null? (cdr args))
             (car args))
            ((null? (cddr args))
             (flmax2 (car args) (cadr args)))
            (else
             (flmax2 (flmax2 (car args) (cadr args))
                     (apply flmax (cddr args))))))))

;;; incompatible with (rnrs arithmetic flonums) in zero-argument case

(define flmin
  (let ((flmin2 (flop2 'flmin min)))
    (lambda args
      (cond ((null? args)
             +inf.0)                 ; spec says fl-least, but that's wrong
            ((null? (cdr args))
             (car args))
            ((null? (cddr args))
             (flmin2 (car args) (cadr args)))
            (else
             (flmin2 (flmin2 (car args) (cadr args))
                     (apply flmin (cddr args))))))))

(define flnormalized?
  (lambda (x)
    (check-flonum! 'flnormalized? x)
    (let ((x (flabs x)))
      (and (flfinite? x)
           (fl<? (fl/ fl-greatest) x)))))

(define fldenormalized?
  (lambda (x)
    (check-flonum! 'fldenormalized? x)
    (let ((x (flabs x)))
      (and (flfinite? x)
           (fl<? 0.0 x)
           (fl<=? x (fl/ fl-greatest))))))

;;; Arithmetic

;;; Spec says "as if to infinite precision and rounded only once".

(define fl+*
  (flop3 'fl+*
         (lambda (x y z)
           (cond (c-functions-are-available
                  (fma x y z))
                 ((and (flfinite? x) (flfinite? y))
                  (if (flfinite? z)
                      (let ((x (exact x))
                            (y (exact y))
                            (z (exact z)))
                        (flonum (+ (* x y) z)))
                      z))
                 (else
                  (fl+ (fl* x y) z))))))

(define (flabsdiff x y)
  (flabs (fl- x y)))

(define (flposdiff x y)
  (let ((diff (fl- x y)))
    (if (flnegative? diff)
        0.0
        diff)))

(define (flsgn x)
  (flcopysign 1.0 x))

;;; (flnumerator +nan.0) and (fldenominator +nan.0) must be NaNs, which
;;; is not required by the R6RS specification of (rnrs arithmetic flonums).

(define flnumerator
  (flop1 'flnumerator
         (lambda (x)
           (cond ((flnan? x) x)
                 ;; SRFI 144: numerator of an infinity is that infinity.
                 ((flinfinite? x) x)
                 (else (r6rs:flnumerator x))))))

(define fldenominator
  (flop1 'fldenominator
         (lambda (x)
           (cond ((flnan? x) x)
                 ;; SRFI 144: denominator of an infinity is 1.0.
                 ((flinfinite? x) 1.0)
                 (else (r6rs:fldenominator x))))))

;;; Exponents and logarithms

(define flexp2 (flop1 'flexp2 (lambda (x) (flexpt 2.0 x))))

;;; e^x = \sum_n (z^n / (n!))

(define flexp-1
  (flop1 'flexp-1
         (let ((coefs (cons 0.0
                            (map fl/
                                 (map factorial
                                      '(1.0 2.0 3.0 4.0 5.0
                                        6.0 7.0 8.0 9.0 10.0
                                        11.0 12.0 13.0 14.0 15.0))))))
           (lambda (x)
             (cond ((fl<? (flabs x) 0.5)    ; FIXME
                    (polynomial-at x coefs))
                   (else
                    (fl- (flexp x) 1.0)))))))

(define flsquare (flop1 'flsquare (lambda (x) (fl* x x))))

(define flcbrt
  (flop1 'flcbrt
         (lambda (x)
           (cond ((flnegative? x)
                  (fl- (flcbrt (fl- x))))
                 (else
                  (flexpt x (fl/ 3.0)))))))

(define flhypot
  (flop2 'flhypot
         (lambda (x y)
           (cond ((flzero? x) (flabs y))
                 ((flzero? y) (flabs x))
                 ((or (flinfinite? x) (flinfinite? y)) +inf.0)
                 ((flnan? x) x)
                 ((flnan? y) y)
                 ((fl>? y x) (flhypot y x))
                 (else
                  (let* ((y/x (fl/ y x))
                         (root (flsqrt (fl+ 1.0 (fl* y/x y/x)))))
                    (fl* (flabs x) root)))))))

;;; Returns log(x+1), as in C99 log1p.

(define fllog1+
  (flop1 'fllog1+
         (lambda (x)
           (let ((u (fl+ 1.0 x)))
             (cond ((fl=? u 1.0)
                    x) ;; gets sign of zero result correct
                   ((fl=? u x)
                    (fllog u)) ;; large arguments and infinities
                   (else
                    (fl* (fllog u) (fl/ x (fl- u 1.0)))))))))

(define fllog2 (flop1 'fllog2 (lambda (x) (log x 2.0))))

(define fllog10 (flop1 'fllog10 (lambda (x) (log x 10.0))))

(define (make-fllog-base base)
  (check-flonum! 'make-fllog-base base)
  (if (fl>? base 1.0)
      (flop1 'procedure-created-by-make-fllog-base
             (lambda (x) (log x base)))
      (error "argument to make-fllog-base must be greater than 1.0" base)))

;;; Trigonometric functions

(define flsinh
  (flop1 'flsinh
         (lambda (x)
           (cond ((not (flfinite? x)) x)
                 ((fl<? (flabs x) 0.75)
                  (fl/ (fl- (flexp-1 x) (flexp-1 (fl- x))) 2.0))
                 (else
                  (fl/ (fl- (flexp x) (flexp (fl- x))) 2.0))))))

(define flcosh
  (flop1 'flcosh
         (lambda (x)
           (cond ((not (flfinite? x)) (flabs x))
                 ((fl<? (flabs x) 0.75)
                  (fl+ 1.0 (fl/ (fl+ (flexp-1 x) (flexp-1 (fl- x))) 2.0)))
                 (else
                  (fl/ (fl+ (flexp x) (flexp (fl- x))) 2.0))))))

(define fltanh
  (flop1 'fltanh
         (lambda (x)
           (cond ((flinfinite? x) (flcopysign 1.0 x))
                 ((flnan? x) x)
                 (else
                  (let ((a (flsinh x))
                        (b (flcosh x)))
                    (cond ((fl=? a b)
                           1.0)
                          ((fl=? a (fl- b))
                           -1.0)
                          (else
                           (fl/ (flsinh x) (flcosh x))))))))))

;;; inverse hyperbolic functions

(define flasinh
  (flop1 'flasinh
         (lambda (x)
           (cond ((or (flinfinite? x)
                      (flnan? x))
                  x)
                 ((flnegative? x)
                  (fl- (flasinh (fl- x))))
                 ((fl<? x 3.725290298461914e-9)   ;; (flexpt 2. -28.)
                  x)
                 ((fl<? x 2.)
                  (let ((x^2 (flsquare x)))
                    (fllog1+ (fl+ x
                                  (fl/ x^2
                                       (fl+ 1.
                                            (flsqrt (fl+ 1.0 x^2))))))))
                 ((fl<? x 268435456.) ;; (flexpt 2. 28.)
                  (let ((x^2 (flsquare x)))
                    (fllog (fl+ (fl* 2. x) ;; exact
                                (fl/ 1.
                                     (fl+ x
                                          (flsqrt (fl+ 1.0 x^2))))))))
                 (else
                  (fl+ (fllog x) fl-log-2))))))

(define flacosh
  (flop1 'flacosh
         (lambda (x)
           (cond ((flnan? x) x)
                 ((fl<? x 1.0) +nan.0)
                 ((fl<? x 2.0)
                  (let ((x-1 (fl- x 1.))) ;; exact
                    (fllog1+ (fl+ x-1 ;; smaller than next expression
                                  (flsqrt (fl+ (fl* 2. x-1) ;; exact
                                               (flsquare x-1)))))))
                 ((fl<? x 268435456.) ;; (flexpt 2. 28.)
                  (fllog (fl- (fl* 2. x) ;; exact
                              (fl/ (fl+ x (flsqrt (fl* (fl- x 1.) ;; exact
                                                       (fl+ x 1.) ;; exact
                                                       )))))))
                 (else
                  (fl+ (fllog x) fl-log-2))))))

(define flatanh
  (flop1 'flatanh
         (lambda (x)
           (cond ((fl<? x 0.)
                  (fl- (flatanh (fl- x))))
                 (else
                  (fl* +0.5                                    ;; exact
                       (fllog1+ (fl* +2.0                      ;; exact
                                     (fl/ x
                                          (fl- 1.0 x)))))))))) ;; exact

;;; Integer division

(define flquotient
  (flop2 'flquotient
         (lambda (x y)
           (fltruncate (fl/ x y)))))

(define flremainder
  (flop2 'flremainder
         (lambda (x y)
           (fl- x (fl* y (flquotient x y))))))

(define (flremquo x y)
  (check-flonum! 'flremquo x)
  (check-flonum! 'flremquo y)
  (let* ((quo (flround (fl/ x y)))
         (rem (fl- x (fl* y quo))))
    (values rem
            (exact quo))))

;;; ==================================================================
;;; 144.special.scm
;;; ==================================================================

;;; Gamma function

(define (flgamma x)
  (check-flonum! 'flgamma x)
  (cond ((fl>=? x flgamma:upper-cutoff)
         +inf.0)
        ((fl<=? x flgamma:lower-cutoff)
         (cond ((= x -inf.0)
                +nan.0)
               ((flinteger? x)    ; pole error
                +nan.0)
               ((flodd? (fltruncate x)) 0.0)
               (else -0.0)))
        (else (Gamma x))))

(define (Gamma x)
  (cond ((fl>? x 2.0)
         (let ((x (fl- x 2.0)))
           (fl* x (fl+ x 1.0) (Gamma x))))
        ((fl=? x 2.0)
         1.0)
        ((fl>? x 1.0)
         (let ((x (fl- x 1.0)))
           (fl* x (Gamma x))))
        ((fl=? x 1.0)
         1.0)
        ((fl=? x 0.0)
         +inf.0)
        ((fl<? x 0.0)
         (if (flinteger? x)    ; pole error
             +nan.0
             (fl/ (Gamma (fl+ x 2.0)) x (fl+ x 1.0))))
        (else
         (fl/ (polynomial-at x gamma-coefs)))))

;;; Series expansion for 1/Gamma(x), from Abramowitz and Stegun 6.1.34

(define gamma-coefs
  '(0.0
    1.0
    +0.5772156649015329
    -0.6558780715202538
    -0.0420026350340952
    +0.1665386113822915 ; x^5
    -0.0421977345555443
    -0.0096219715278770
    +0.0072189432466630
    -0.0011651675918591
    -0.0002152416741149 ; x^10
    +0.0001280502823882
    -0.0000201348547807
    -0.0000012504934821
    +0.0000011330272320
    -0.0000002056338417 ; x^15
    +0.0000000061160950
    +0.0000000050020075
    -0.0000000011812746
    +0.0000000001043427
    +0.0000000000077823 ; x^20
    -0.0000000000036968
    +0.0000000000005100
    -0.0000000000000206
    -0.0000000000000054
    +0.0000000000000014 ; x^25
    +0.0000000000000001
    ))

;;; If x >= flgamma:upper-cutoff, then (Gamma x) is +inf.0

;; Upstream finds these two cutoffs by iterating the (expensive) Gamma
;; ~350 times at load: the smallest x>=2 where Gamma overflows to +inf,
;; and the negative x past which Gamma underflows to 0. Under the
;; tree-walking interpreter that scan costs ~18s, so the deterministic
;; results are inlined as literals (bead nscheme-oeg.3.1). The original
;; do-loops are kept commented for provenance.
;;   (do ((x 2.0 (+ x 1.0)))  ((flinfinite? (Gamma x)) x))            => 172.0
;;   (do ((x -2.0 (- x 1.0))) ((flzero? (Gamma (fladjacent x 0.0))) x)) => -184.0
(define flgamma:upper-cutoff 172.0)

;;; If x <= flgamma:lower-cutoff, then (Gamma x) is a zero or NaN

(define flgamma:lower-cutoff -184.0)

;;; log (Gamma (x))

(define (flloggamma x)
  (check-flonum! 'flloggamma x)
  (cond ((flinfinite? x)
         (if (flpositive? x)
             (values x 1.0)
             (values +inf.0 +nan.0)))
        ((fl>=? x flloggamma:upper-threshold)
         (values (eqn6.1.48 x) 1.0))
        ((fl>? x 0.0)
         (let ((g (flgamma x)))
           (values (log g) 1.0)))
        (else
         (let ((g (flgamma x)))
           (values (log (flabs g))
                   (flcopysign 1.0 g))))))

(define (eqn6.1.48 x)
  (let ((+ fl+)
        (/ fl/))
    (+ (fl* (fl- x 0.5) (fllog x))
       (fl- x)
       (fl* 0.5 (fllog fl-2pi))
       (/ #i1/12
          (+ x
             (/ #i1/30
                (+ x
                   (/ #i53/210
                      (+ x
                         (/ #i195/371
                            (+ x
                               (/ #i22999/22737
                                  (+ x
                                     (/ #i29944523/19733142
                                        (+ x
                                           (/ #i109535241009/48264275462
                                              (+ x)))))))))))))))))

;;; With IEEE double precision, eqn6.1.48 is at least as accurate as
;;; (log (flgamma x)) starting around x = 20.0

(define flloggamma:upper-threshold 20.0)

;;; Bessel functions

(define (flfirst-bessel n x)
  (define (nan-protected y)
    (if (flfinite? y) y 0.0))
  (check-flonum! 'flfirst-bessel x)
  (cond (c-functions-are-available
         (jn n x))

        ((< n 0)
         (let ((result (flfirst-bessel (- n) x)))
           (if (even? n) result (- result))))

        ((< x 0)
         (let ((result (flfirst-bessel n (- x))))
           (if (even? n) result (- result))))

        ((= x +inf.0)
         0.0)

        (else
         (case n
          ((0)    (cond ((fl<? x 4.5)     ; FIXME
                         (eqn9.1.10 n x))
                        ((fl<? x 93.0)    ; FIXME
                         (eqn9.1.18 n x))
                        (else
                         (eqn9.2.5 n x))))
          ((1)    (cond ((fl<? x 11.0)    ; FIXME
                         (eqn9.1.10-fast n x))
                        ((fl<? x 300.0)   ; FIXME
                         (eqn9.1.75 n x))
                        ((fl<? x 1e12)    ; FIXME
                         (eqn9.2.5 n x))
                        (else
                         (eqn9.2.1 n x))))
          ((2)    (cond ((fl<? x 10.0)    ; FIXME
                         (eqn9.1.10-fast n x))
                        ((fl<? x 1e19)    ; FIXME
                         (eqn9.1.27-first-bessel n x))
                        (else
                         ;; FIXME
                         0.0)))
          ((3)    (cond ((fl<? x 10.0)    ; FIXME
                         (eqn9.1.10-fast n x))
                        ((fl<? x 1e6)     ; FIXME
                         (eqn9.1.27-first-bessel n x))
                        (else
                         (nan-protected (eqn9.2.5 n x)))))
          (else   (cond ((fl<? x 12.0)    ; FIXME
                         (nan-protected (eqn9.1.10-fast n x)))
                        ((fl<? x 150.0)   ; FIXME
                         (nan-protected (if (fl>? (inexact n) x)
                                            (method9.12ex1 n x)
                                            (eqn9.1.75 n x))))
                        ((fl<? x 1e18)    ; FIXME
                         (nan-protected (eqn9.1.27-first-bessel n x)))
                        (else
                         ;; FIXME
                         0.0)))))))

(define (flsecond-bessel n x)
  (check-flonum! 'flsecond-bessel x)
  (cond (c-functions-are-available
         (yn n x))

        ((< n 0)
         (let ((result (flsecond-bessel (- n) x)))
           (if (even? n) result (- result))))

        ((fl<? x 0.0)
         +nan.0)

        ((fl=? x 0.0)
         -inf.0)

        ((fl=? x +inf.0)
         0.0)

        (else
         (case n
          ((0)    (cond ((fl<? x 14.5)        ; FIXME
                         (eqn9.1.13 0 x))
                        (else
                         (eqn9.2.6 0 x))))
          ((1)    (cond ((fl<? x 1e12)        ; FIXME
                         (eqn9.1.16 n x))
                        (else
                         (eqn9.2.6 n x))))
          ((2 3)  (cond (else
                         (eqn9.1.27-second-bessel n x))))
          (else   (let ((ynx (eqn9.1.27-second-bessel n x)))
                    (if (flnan? ynx)
                        -inf.0
                        ynx)))))))

(define (eqn9.1.10 n x)
  (fl* (inexact (expt (* 0.5 x) n))
       (polynomial-at (flsquare x)
                      (cond ((= n 0)
                             eqn9.1.10-coefficients-0)
                            ((= n 1)
                             eqn9.1.10-coefficients-1)
                            (else
                             (eqn9.1.10-coefficients n))))))

(define (eqn9.1.10-coefficients n)
  (define (loop k prev)
    (if (flzero? (inexact prev))
        '()
        (let ((c (/ (* -1/4 prev) k (+ n k))))
          (cons c (loop (+ k 1) c)))))
  (let ((c (/ (fact n))))
    (map inexact (cons c (loop 1 c)))))

(define eqn9.1.10-coefficients-0
  (eqn9.1.10-coefficients 0))

(define eqn9.1.10-coefficients-1
  (eqn9.1.10-coefficients 1))

;;; This is faster than using exact arithmetic to compute coefficients
;;; at call time, and it seems to be about as accurate.

(define (eqn9.1.10-fast n x)
  (let* ((y (fl* 0.5 x))
         (y2 (fl- (fl* y y)))
         (bound (+ 25.0 (inexact n))))
    (define (loop k n+k)
      (if (fl>? n+k bound)
          1.0
          (fl+ 1.0
               (fl* (fl/ y2 (fl* k n+k))
                    (loop (fl+ 1.0 k) (fl+ 1.0 n+k))))))
    (fl/ (fl* (inexact (expt y n))
              (loop 1.0 (fl+ 1.0 (inexact n))))
         (factorial (inexact n)))))

(define (eqn9.1.13 n x)
  (if (not (= n 0)) (error "eqn9.1.13 requires n=0"))
  (fl* 2.0
       fl-1/pi
       (fl+ (fl* (fl+ (fllog (fl/ x 2.0)) fl-euler)
                 (flfirst-bessel 0 x))
            (polynomial-at (fl* 0.25 x x)
                           eqn9.1.13-coefficients))))

(define eqn9.1.13-coefficients
  (map (lambda (k)
         (cond ((= k 0) 0.0)
               ((= k 1) 1.0)
               (else
                ;; (1 + 1/2 + 1/3 + ... + 1/k) / (k!)^2
                (let ((c (/ (apply + (map / (cdr (iota (+ k 1)))))
                            (let ((k! (fact k)))
                              (* k! k!)))))
                  (inexact (if (even? k) (- c) c))))))
       (iota 25))) ; FIXME

(define (eqn9.1.16 n+1 x)
  (if (= 0 n+1)
      (flsecond-bessel 0 x)
      (let ((n (- n+1 1)))
        (fl/ (fl- (fl* (flfirst-bessel n+1 x) (flsecond-bessel n x))
                  (fl/ 2.0 (fl* fl-pi x)))
             (flfirst-bessel n x)))))

(define (eqn9.1.18 n x)
  (if (> n 0)
      (flfirst-bessel n x)
      (fl* fl-1/pi
           (definite-integral 0.0
                              fl-pi
                              (lambda (theta)
                                (flcos (fl* x (flsin theta))))
                              128))))

(define (eqn9.1.27-first-bessel n x)
  (eqn9.1.27 flfirst-bessel n x))

(define (eqn9.1.27-second-bessel n x)
  (eqn9.1.27 flsecond-bessel n x))

(define (eqn9.1.27 f n0 x)
  (define (loop n jn jn-1)
    (cond ((= n n0)
           jn)
          (else
           (loop (+ n 1)
                 (fl- (fl* (fl/ (inexact (+ n n)) x) jn)
                      jn-1)
                 jn))))
  (if (<= n0 1)
      (f n0 x)
      (loop 1 (f 1 x) (f 0 x))))

(define (method9.12ex1 n0 x)
  (define (loop n jn jn+1 jn0 sumEvens)
    (if (= n 0)
        (fl/ jn0 (+ jn sumEvens sumEvens))
        (let ((jn-1 (fl- (fl/ (fl* 2.0 (inexact n) jn) x) jn+1)))
          (loop (- n 1)
                jn-1
                jn
                (if (= n n0) jn jn0)
                (if (even? n) (fl+ jn sumEvens) sumEvens)))))
  (let* ((n (min 200 (+ n0 20))) ; FIXME
         (jn+1 (fl/ x (fl* 2.0 (inexact n))))
         (jn 1.0))
    (loop (- n 1) jn jn+1 0.0 0.0)))

(define (eqn9.1.75 n x)
  (define k (max 10 (* 2 (exact (flceiling x)))))
  (define (loop x2 m i)
    (if (> i k)
        (fl/ 1.0 (fl* m x2))
        (fl/ 1.0
             (fl- (fl* m x2)
                  (loop x2 (+ m 1.0) (+ i 1))))))
  (if (and (> n 0)
           (flpositive? x)
           (fl<? x 1e3))
      (fl* (eqn9.1.75 (- n 1) x)
           (loop (fl/ 2.0 x) (inexact n) 0))
      (flfirst-bessel n x)))

(define (eqn9.2.1 n x)
  (fl* (flsqrt (/ 2.0 (fl* fl-pi x)))
       (flcos (fl- x (fl* fl-pi (fl+ (fl* 0.5 (inexact n)) 0.25))))))

(define (eqn9.2.5 n x)
  (let ((theta (fl- x (fl* (fl+ (/ n 2.0) 0.25) fl-pi))))
    (fl* (flsqrt (fl/ 2.0 (fl* fl-pi x)))
         (fl- (fl* (eqn9.2.9 n x) (flcos theta))
              (fl* (eqn9.2.10 n x) (flsin theta))))))

(define (eqn9.2.6 n x)
  (let ((theta (fl- x (fl* (fl+ (/ n 2.0) 0.25) fl-pi))))
    (fl* (flsqrt (fl/ 2.0 (fl* fl-pi x)))
         (fl+ (fl* (eqn9.2.9 n x) (flsin theta))
              (fl* (eqn9.2.10 n x) (flcos theta))))))

(define (eqn9.2.9 n x) ; returns P(n, x)
  (define mu (fl* 4.0 (flsquare (inexact n))))
  (define (coefficients k2 p fact2k)
    (let ((c (fl/ p fact2k)))
      (if (fl>? k2 20.0) ; FIXME
          (list c)
          (cons c (coefficients (fl+ k2 2.0)
                                (fl* p
                                     (fl- mu (flsquare (fl+ k2 1.0)))
                                     (fl- mu (flsquare (fl+ k2 3.0))))
                                (fl* fact2k
                                     (fl+ k2 1.0)
                                     (fl+ k2 2.0)))))))
  (polynomial-at (fl- (fl/ (flsquare (fl* 8.0 x))))
                 (coefficients 0.0 1.0 1.0)))

(define (eqn9.2.10 n x) ; returns Q(n, x)
  (define mu (fl* 4.0 (flsquare (inexact n))))
  (define (coefficients k2+1 p fact2k+1)
    (let ((c (fl/ p fact2k+1)))
      (if (fl>? k2+1 20.0) ; FIXME
          (list c)
          (cons c (coefficients (fl+ k2+1 2.0)
                                (fl* p
                                     (fl- mu (flsquare (fl+ k2+1 2.0)))
                                     (fl- mu (flsquare (fl+ k2+1 4.0))))
                                (fl* fact2k+1
                                     (fl+ k2+1 1.0)
                                     (fl+ k2+1 2.0)))))))
  (fl* (fl/ (fl* 8.0 x))
       (polynomial-at (fl- (fl/ (flsquare (fl* 8.0 x))))
                      (coefficients 1.0 (fl- mu 1.0) 1.0))))

;;; Error functions

(define (flerf x)
  (check-flonum! 'flerf x)
  (cond ((flnegative? x)
         (fl- (flerf (fl- x))))
        ((fl<? x 2.0)
         (eqn7.1.6 x))
        ((fl<? x +inf.0)
         (- 1.0 (eqn7.1.14 x)))
        ((fl=? x +inf.0)
         1.0)
        (else x)))

(define (flerfc x)
  (check-flonum! 'flerfc x)
  (cond ((flnegative? x)
         (fl- 2.0 (flerfc (fl- x))))
        ((fl<? x 2.0)
         (eqn7.1.2 x))
        ((fl<? x +inf.0)
         (eqn7.1.14 x))
        ((fl=? x +inf.0)
         0.0)
        (else x)))

(define (eqn7.1.2 x)
  (fl- 1.0 (flerf x)))

(define (eqn7.1.6 x)
  (let ((x^2 (flsquare x)))
    (fl* fl-2/sqrt-pi
         (flexp (fl- x^2))
         x
         (polynomial-at x^2 eqn7.1.6-coefficients))))

(define eqn7.1.6-coefficients
  (let ()
    (define (loop n p)
      (if (> n 32) ; FIXME
          '()
          (let ((p (fl* p (inexact (+ (* 2 n) 1)))))
            (cons (fl/ (inexact (expt 2.0 n)) p)
                  (loop (+ n 1) p)))))
    (loop 0 1.0)))

(define (eqn7.1.14 x)
  (define (continued-fraction x)
    (fl/ 1.0 (fl+ x (loop 1 0.5))))
  (define (loop k frac)
    (if (> k 70) ; FIXME
        1.0
        (fl/ frac (fl+ x (loop (+ k 1) (fl+ frac 0.5))))))
  (fl/ (continued-fraction x)
       (fl* (flsqrt fl-pi)
            (flexp (flsquare x)))))

  )

  ;; If the C library is available, use it.  nscheme has no Larceny FFI,
  ;; so the else branch runs: C hooks resolve to error stubs and
  ;; fl-fast-fl+* / fl-fast-fl+* report no fused multiply-add.

  (cond-expand
   ((and larceny i386 unix (or gnu-linux darwin))
    (begin (define c-functions-are-available #t)
           (define fl-fast-fl+* #f))
    (include "144.ffi.scm"))
   (else
    (begin (define c-functions-are-available #f)
           (define fl-fast-fl+* #f)
           (define (fma x y z) (error "fma not defined"))
           (define (jn n x) (error "jn not defined"))
           (define (yn n x) (error "yn not defined")))))
  )

;;; eof
