;;; (srfi 143) — Fixnums.
;;;
;;; Vendored from the SRFI 143 reference implementation by John Cowan
;;; (2016), MIT licence (text preserved below). Flattened into a single
;;; file: the upstream .sld uses (include "rubber-chicken.scm"),
;;; (include "fxcore.scm"), (include "carries.scm"), and
;;; (include "srfi-143-impl.scm"), but nscheme resolves `include`
;;; relative to the process directory, so each body file is inlined here
;;; inside `begin` instead.
;;;
;;; The upstream cond-expand selects platform-specific bitwise cores
;;; (chibi / gauche), falling back to the portable `fxcore.scm` in the
;;; `else` branch. nscheme defines neither `chibi` nor `gauche`, so the
;;; `else` branch is taken; the cond-expand is preserved intact and only
;;; the selected branch's body is inlined. The reference implementation
;;; deliberately wraps generic arithmetic (it is the portable fallback),
;;; so all operations are bignum-correct rather than hardware fixnums.
;;;
;;; SPDX-License-Identifier: MIT
;;; Copyright (C) John Cowan (2016). All Rights Reserved.
;;;
;;; Permission is hereby granted, free of charge, to any person
;;; obtaining a copy of this software and associated documentation files
;;; (the "Software"), to deal in the Software without restriction,
;;; including without limitation the rights to use, copy, modify, merge,
;;; publish, distribute, sublicense, and/or sell copies of the Software,
;;; and to permit persons to whom the Software is furnished to do so,
;;; subject to the following conditions:
;;;
;;; The above copyright notice and this permission notice shall be
;;; included in all copies or substantial portions of the Software.
;;;
;;; THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
;;; EXPRESS OR IMPLIED. See the SRFI 143 document for full text.

(define-library (srfi 143)

  (import (rename (scheme base)
                  (exact-integer-sqrt fxsqrt)))

  (export fx-width fx-greatest fx-least)
  (export fixnum? fx=? fx<? fx>? fx<=? fx>=?
          fxzero? fxpositive? fxnegative?
          fxodd? fxeven? fxmax fxmin)
  (export fx+ fx- fxneg fx* fxquotient fxremainder
          fxabs fxsquare fxsqrt)
  (export fx+/carry fx-/carry fx*/carry)
  (export fxnot fxand fxior fxxor fxarithmetic-shift
          fxarithmetic-shift-left fxarithmetic-shift-right
          fxbit-count fxlength fxif fxbit-set? fxcopy-bit
          fxfirst-set-bit fxbit-field
          fxbit-field-rotate fxbit-field-reverse)

  ;; ----------------------------------------------------------------
  ;; Provide Chicken emulation (rubber-chicken.scm, inlined)
  ;; ----------------------------------------------------------------
  ;; Portable (generic) versions of Chicken fixnum operations.
  (begin

    ;;; Fixnum limits.  24 is the SRFI minimum width.
    (define fx-width 24)
    (define fx-greatest 8388607)
    (define fx-least -8388608)

    (define (fixnum? x)
      (and (exact-integer? x) (<= fx-least x fx-greatest)))

    ;;; Basic arithmetic

    (define (fx+ i j) (+ i j))
    (define (fx- i j) (- i j))
    (define (fx* i j) (* i j))
    (define (fxquotient i j) (quotient i j))
    (define (fxremainder i j) (remainder i j))
    (define (fxneg i) (- i))

    ;;; Defined as syntax upstream (never exported, non-recursive).

    (define-syntax chicken:fxmax
      (syntax-rules ()
        ((chicken:fxmax i j) (if (> i j) i j))))

    (define-syntax chicken:fxmin
      (syntax-rules ()
        ((chicken:fxmin i j) (if (< i j) i j))))

    (define-syntax chicken:fx=
      (syntax-rules ()
        ((chicken:fx= i j) (= i j))))

    (define-syntax chicken:fx<
      (syntax-rules ()
        ((chicken:fx< i j) (< i j))))

    (define-syntax chicken:fx>
      (syntax-rules ()
        ((chicken:fx> i j) (> i j))))

    (define-syntax chicken:fx<=
      (syntax-rules ()
        ((chicken:fx<= i j) (<= i j))))

    (define-syntax chicken:fx>=
      (syntax-rules ()
        ((chicken:fx>= i j) (>= i j))))

    (define (fxodd? i) (odd? i))

    (define (fxeven? i) (even? i)))

  ;; ----------------------------------------------------------------
  ;; Provide core bitwise functions.  Upstream cond-expand kept intact;
  ;; nscheme defines neither `chibi` nor `gauche`, so the `else` branch
  ;; (fxcore.scm) is selected and inlined below.
  ;; ----------------------------------------------------------------
  (cond-expand
    (chibi
      (include-shared "srfi/142/bit")
      (begin
        (define (fxnot i) (- -1 i))

        (define (make-nary proc2 default)
          (lambda args
            (if (null? args)
                default
                (let lp ((i (car args)) (ls (cdr args)))
                  (if (null? ls)
                      i
                      (lp (proc2 i (car ls)) (cdr ls)))))))

        (define fxand  (make-nary bit-and  -1))
        (define fxior  (make-nary bit-ior   0))
        (define fxxor  (make-nary bit-xor   0))
        (define fxlength integer-length)
        (define fxbit-count bit-count)
        (define fxarithmetic-shift-left arithmetic-shift)
        (define (fxarithmetic-shift-right i count)
          (fxarithmetic-shift-left i (- count)))))

    (gauche
      (import (only (gauche base)
                    integer-length))
      (import (rename (only (gauche base)
                            lognot logand logior logxor ash)
                      (lognot fxnot)
                      (logand fxand)
                      (logior fxior)
                      (logxor fxxor)
                      (ash arithmetic-shift-left)))
      (begin
        (define (arithmetic-shift-right i count)
          (arithmetic-shift-left i (- count)))))

    ;; ------------------------------------------------------------
    ;; else: fxcore.scm — fixnum version of core bitwise operations.
    ;; Copyright (C) 1991, 1993, 2001, 2003, 2005 Aubrey Jaffer.
    ;; Drawn from the SRFI 60 / SRFI 33 implementation. (Permission to
    ;; copy, modify, redistribute, and use for any purpose granted,
    ;; subject to retaining this notice; no warranty.)
    ;; ------------------------------------------------------------
    (else
      (begin

        (define (fxnot n) (fx- -1 n))

        (define logical:boole-xor
         '#(#(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15)
            #(1 0 3 2 5 4 7 6 9 8 11 10 13 12 15 14)
            #(2 3 0 1 6 7 4 5 10 11 8 9 14 15 12 13)
            #(3 2 1 0 7 6 5 4 11 10 9 8 15 14 13 12)
            #(4 5 6 7 0 1 2 3 12 13 14 15 8 9 10 11)
            #(5 4 7 6 1 0 3 2 13 12 15 14 9 8 11 10)
            #(6 7 4 5 2 3 0 1 14 15 12 13 10 11 8 9)
            #(7 6 5 4 3 2 1 0 15 14 13 12 11 10 9 8)
            #(8 9 10 11 12 13 14 15 0 1 2 3 4 5 6 7)
            #(9 8 11 10 13 12 15 14 1 0 3 2 5 4 7 6)
            #(10 11 8 9 14 15 12 13 2 3 0 1 6 7 4 5)
            #(11 10 9 8 15 14 13 12 3 2 1 0 7 6 5 4)
            #(12 13 14 15 8 9 10 11 4 5 6 7 0 1 2 3)
            #(13 12 15 14 9 8 11 10 5 4 7 6 1 0 3 2)
            #(14 15 12 13 10 11 8 9 6 7 4 5 2 3 0 1)
            #(15 14 13 12 11 10 9 8 7 6 5 4 3 2 1 0)))

        (define logical:boole-and
         '#(#(0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0)
            #(0 1 0 1 0 1 0 1 0 1 0 1 0 1 0 1)
            #(0 0 2 2 0 0 2 2 0 0 2 2 0 0 2 2)
            #(0 1 2 3 0 1 2 3 0 1 2 3 0 1 2 3)
            #(0 0 0 0 4 4 4 4 0 0 0 0 4 4 4 4)
            #(0 1 0 1 4 5 4 5 0 1 0 1 4 5 4 5)
            #(0 0 2 2 4 4 6 6 0 0 2 2 4 4 6 6)
            #(0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7)
            #(0 0 0 0 0 0 0 0 8 8 8 8 8 8 8 8)
            #(0 1 0 1 0 1 0 1 8 9 8 9 8 9 8 9)
            #(0 0 2 2 0 0 2 2 8 8 10 10 8 8 10 10)
            #(0 1 2 3 0 1 2 3 8 9 10 11 8 9 10 11)
            #(0 0 0 0 4 4 4 4 8 8 8 8 12 12 12 12)
            #(0 1 0 1 4 5 4 5 8 9 8 9 12 13 12 13)
            #(0 0 2 2 4 4 6 6 8 8 10 10 12 12 14 14)
            #(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15)))

        (define (logical:ash-4 x)
          (if (negative? x)
              (+ -1 (quotient (+ 1 x) 16))
              (quotient x 16)))

        (define (logical:reduce op4 ident)
          (lambda args
            (do ((res ident (op4 res (car rgs) 1 0))
                 (rgs args (cdr rgs)))
                ((null? rgs) res))))

        (define fxand
          (letrec
              ((lgand
                (lambda (n2 n1 scl acc)
                  (cond ((fx=? n1 n2) (fx+ acc (fx* scl n1)))
                        ((fxzero? n2) acc)
                        ((fxzero? n1) acc)
                        (else (lgand (logical:ash-4 n2)
                                     (logical:ash-4 n1)
                                     (fx* 16 scl)
                                     (fx+ (fx* (vector-ref (vector-ref logical:boole-and
                                                                   (modulo n1 16))
                                                       (modulo n2 16))
                                           scl)
                                        acc)))))))
            (logical:reduce lgand -1)))

        (define fxior
          (letrec
              ((lgior
                (lambda (n2 n1 scl acc)
                  (cond ((fx=? n1 n2) (fx+ acc (fx* scl n1)))
                        ((fxzero? n2) (fx+ acc (fx* scl n1)))
                        ((fxzero? n1) (fx+ acc (fx* scl n2)))
                        (else (lgior (logical:ash-4 n2)
                                     (logical:ash-4 n1)
                                     (fx* 16 scl)
                                     (fx+ (fx* (fx- 15 (vector-ref
                                                  (vector-ref logical:boole-and
                                                              (fx- 15 (modulo n1 16)))
                                                  (fx- 15 (modulo n2 16))))
                                           scl)
                                        acc)))))))
            (logical:reduce lgior 0)))

        (define fxxor
          (letrec
              ((lgxor
                (lambda (n2 n1 scl acc)
                  (cond ((fx=? n1 n2) acc)
                        ((fxzero? n2) (fx+ acc (fx* scl n1)))
                        ((fxzero? n1) (fx+ acc (fx* scl n2)))
                        (else (lgxor (logical:ash-4 n2)
                                     (logical:ash-4 n1)
                                     (fx* 16 scl)
                                     (fx+ (fx* (vector-ref (vector-ref logical:boole-xor
                                                                   (modulo n1 16))
                                                       (modulo n2 16))
                                           scl)
                                        acc)))))))
            (logical:reduce lgxor 0)))

        (define (fxarithmetic-shift-right n count)
          (let ((k (expt 2 count)))
            (if (fxnegative? n)
              (fx+ -1 (quotient (fx+ 1 n) k))
                (quotient n k))))

        (define (fxarithmetic-shift-left n count)
          (* (expt 2 count) n))

        (define fxlength
          (letrec ((intlen (lambda (n tot)
                             (case n
                               ((0 -1) (fx+ 0 tot))
                               ((1 -2) (fx+ 1 tot))
                               ((2 3 -3 -4) (fx+ 2 tot))
                               ((4 5 6 7 -5 -6 -7 -8) (fx+ 3 tot))
                               (else (intlen (logical:ash-4 n) (fx+ 4 tot)))))))
            (lambda (n) (intlen n 0))))

        (define fxbit-count
          (letrec ((logcnt (lambda (n tot)
                             (if (fxzero? n)
                                 tot
                                 (logcnt (quotient n 16)
                                         (fx+ (vector-ref
                                             '#(0 1 1 2 1 2 2 3 1 2 2 3 2 3 3 4)
                                             (modulo n 16))
                                            tot))))))
            (lambda (n)
              (cond ((fxnegative? n) (logcnt (fxnot n) 0))
                    ((fxpositive? n) (logcnt n 0))
                    (else 0))))))))

  ;; ----------------------------------------------------------------
  ;; Stable part of the implementation: carries.scm — generic
  ;; implementation of the carry functions from the R6RS standard.
  ;; ----------------------------------------------------------------
  (begin

    (define exp-width (expt 2 fx-width))

    (define (fx+/carry i j k)
      (let*-values (((s) (+ i j k))
             ((q r) (balanced/ s exp-width)))
      (values r q)))

    (define (fx-/carry i j k)
      (let*-values (((d) (- i j k))
             ((q r) (balanced/ d exp-width)))
        (values r q)))

    (define (fx*/carry i j k)
      (let*-values (((s) (+ (* i j) k))
             ((q r) (balanced/ s exp-width)))
        (values r q)))

    ;;; Helper functions from SRFI 151

    (define (floor-/+ n d)
      (let ((n (- 0 n)))
        (let ((q (quotient n d)) (r (remainder n d)))
          (if (zero? r)
              (values (- 0 q) r)
              (values (- (- 0 q) 1) (- d r))))))

    (define (ceiling-/- n d)
      (let ((n (- 0 n)) (d (- 0 d)))
        (let ((q (quotient n d)) (r (remainder n d)))
          (if (zero? r)
              (values q r)
              (values (+ q 1) (- d r))))))

    (define (euclidean/ n d)
      (if (and (exact-integer? n) (exact-integer? d))
          (cond ((and (negative? n) (negative? d)) (ceiling-/- n d))
                ((negative? n) (floor-/+ n d))
                ((negative? d)
                 (let ((d (- 0 d)))
                   (values (- 0 (quotient n d)) (remainder n d))))
                (else (values (quotient n d) (remainder n d))))
          (let ((q (if (negative? d) (ceiling (/ n d)) (floor (/ n d)))))
            (values q (- n (* d q))))))

    (define (balanced/ x y)
      (call-with-values
       (lambda () (euclidean/ x y))
       (lambda (q r)
         (cond ((< r (abs (/ y 2)))
                (values q r))
               ((> y 0)
                (values (+ q 1) (- x (* (+ q 1) y))))
               (else
                (values (- q 1) (- x (* (- q 1) y)))))))))

  ;; ----------------------------------------------------------------
  ;; srfi-143-impl.scm — procedures not provided by Chicken or by
  ;; rubber-chicken.  (Inlined.)
  ;; ----------------------------------------------------------------
  (begin

    ;;; Implementations of arithmetic functions

    (define (fx=? i j . ks)
      (if (null? ks)
        (chicken:fx= i j)
        (and (chicken:fx= i j) (apply fx=? j ks))))

    (define (fx<? i j . ks)
      (if (null? ks)
        (chicken:fx< i j)
        (and (chicken:fx< i j) (apply fx<? j ks))))

    (define (fx>? i j . ks)
      (if (null? ks)
        (chicken:fx> i j)
        (and (chicken:fx> i j) (apply fx>? j ks))))

    (define (fx<=? i j . ks)
      (if (null? ks)
        (chicken:fx<= i j)
        (and (chicken:fx<= i j) (apply fx<=? j ks))))

    (define (fx>=? i j . ks)
      (if (null? ks)
        (chicken:fx>= i j)
        (and (chicken:fx>= i j) (apply fx>=? j ks))))

    (define (fxzero? i) (chicken:fx= i 0))
    (define (fxpositive? i) (chicken:fx> i 0))
    (define (fxnegative? i) (chicken:fx< i 0))

    (define (fxmax i j . ks)
      (if (null? ks)
        (chicken:fxmax i j)
        (chicken:fxmax (chicken:fxmax i j) (apply fxmax j ks))))

    (define (fxmin i j . ks)
      (if (null? ks)
        (chicken:fxmin i j)
        (chicken:fxmin (chicken:fxmin i j) (apply fxmin j ks))))

    (define (fxabs i)
      (if (fxnegative? i) (fxneg i) i))

    (define (fxsquare i) (fx* i i))

    (define (fxarithmetic-shift i count)
      (if (negative? count)
        (fxarithmetic-shift-right i (- count))
        (fxarithmetic-shift-left i count)))

    ;;; Bitwise functions cloned from SRFI 151, fixnum version

    ;; Helper function
    (define (mask start end) (fxnot (fxarithmetic-shift-left -1 (- end start))))

    (define (fxif mask n0 n1)
      (fxior (fxand mask n0)
              (fxand (fxnot mask) n1)))

    (define (fxbit-set? index n)
      (not (fxzero? (fxand (fxarithmetic-shift-left 1 index) n))))

    (define (fxcopy-bit index to bool)
      (if bool
          (fxior to (fxarithmetic-shift-left 1 index))
          (fxand to (fxnot (fxarithmetic-shift-left 1 index)))))

    (define (fxfirst-set-bit i) (- (fxbit-count (fxxor i (- i 1))) 1))

    (define (fxbit-field n start end)
      (fxand (mask start end) (fxarithmetic-shift n (- start))))

    (define (fxbit-field-rotate n count start end)
      (define width (fx- end start))
      (set! count (modulo count width))
      (let ((mask (fxnot (fxarithmetic-shift -1 width))))
        (define zn (fxand mask (fxarithmetic-shift n (- start))))
        (fxior (fxarithmetic-shift
                 (fxior (fxand mask (fxarithmetic-shift zn count))
                         (fxarithmetic-shift zn (- count width)))
                 start)
                (fxand (fxnot (fxarithmetic-shift mask start)) n))))

    (define (fxreverse k n)
      (do ((m (if (negative? n) (fxnot n) n) (fxarithmetic-shift-right m 1))
           (k (fx+ -1 k) (fx+ -1 k))
           (rvs 0 (fxior (fxarithmetic-shift-left rvs 1) (fxand 1 m))))
          ((fxnegative? k) (if (fxnegative? n) (fxnot rvs) rvs))))

    (define (fxbit-field-reverse n start end)
      (define width (- end start))
      (let ((mask (fxnot (fxarithmetic-shift-left -1 width))))
        (define zn (fxand mask (fxarithmetic-shift-right n start)))
        (fxior (fxarithmetic-shift-left (fxreverse width zn) start)
                (fxand (fxnot (fxarithmetic-shift-left mask start)) n))))))
