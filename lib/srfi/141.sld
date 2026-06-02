;;; (srfi 141) — Integer division.
;;;
;;; Vendored from the SRFI 141 reference implementation by Taylor R.
;;; Campbell (2010–2011) and William D Clinger (2015). Flattened into a
;;; single file: the upstream .sld uses (include "srfi-141-impl.scm"),
;;; but nscheme resolves `include` relative to the process directory, so
;;; the body file is inlined here inside `begin` instead.
;;;
;;; The upstream library name `(srfi-141)` is rewritten to the
;;; two-element `(srfi 141)` so that `(scheme division)` can import it,
;;; matching the convention of the other vendored SRFIs in this tree.
;;; The duplicated `ceiling/ ceiling-quotient ceiling-remainder` export
;;; line from upstream is dropped (each name is exported once).
;;;
;;; The body redefines `exact-integer?`, `floor/`, `floor-quotient`,
;;; `floor-remainder`, `truncate/`, `truncate-quotient`, and
;;; `truncate-remainder`, which nscheme also provides as `(scheme base)`
;;; built-ins. In nscheme a library body `define` installs a fresh cell
;;; that shadows the imported binding, and the export references that
;;; cell, so this faithful inline loads correctly.
;;;
;;; SPDX-License-Identifier: MIT (library wrapper) / BSD-2-Clause (impl)
;;;
;;; Copyright (c) 2010--2011 Taylor R. Campbell
;;; All rights reserved.
;;;
;;; Redistribution and use in source and binary forms, with or without
;;; modification, are permitted provided that the following conditions
;;; are met:
;;; 1. Redistributions of source code must retain the above copyright
;;;    notice, this list of conditions and the following disclaimer.
;;; 2. Redistributions in binary form must reproduce the above copyright
;;;    notice, this list of conditions and the following disclaimer in the
;;;    documentation and/or other materials provided with the distribution.
;;;
;;; THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
;;; ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
;;; IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
;;; ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
;;; FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
;;; DAMAGES.  See the SRFI 141 document for full text.
;;;
;;; The round/ helper block is Copyright 2015 William D Clinger, used
;;; under permission to copy, use, and redistribute with this notice.

(define-library (srfi 141)
  (export ceiling/ ceiling-quotient ceiling-remainder)
  (export floor/ floor-quotient floor-remainder)
  (export truncate/ truncate-quotient truncate-remainder)
  (export round/ round-quotient round-remainder)
  (export euclidean/ euclidean-quotient euclidean-remainder)
  (export balanced/ balanced-quotient balanced-remainder)
  (import (scheme base))
  (begin

;;;; Shims
;;; SRFI-8
(define-syntax receive
  (syntax-rules ()
    ((receive formals expression body ...)
     (call-with-values (lambda () expression)
                       (lambda formals body ...)))))

;;; exact-integer?
(define (exact-integer? x) (and (integer? x) (exact? x)))

;;;; Integer Division

;;;; Ceiling

(define (ceiling/ n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (ceiling-/- n d))
            ((negative? n)
             (let ((n (- 0 n)))
               (values (- 0 (quotient n d)) (- 0 (remainder n d)))))
            ((negative? d)
             (let ((d (- 0 d)))
               (values (- 0 (quotient n d)) (remainder n d))))
            (else
             (ceiling+/+ n d)))
      (let ((q (ceiling (/ n d))))
        (values q (- n (* d q))))))

(define (ceiling-/- n d)
  (let ((n (- 0 n)) (d (- 0 d)))
    (let ((q (quotient n d)) (r (remainder n d)))
      (if (zero? r)
          (values q r)
          (values (+ q 1) (- d r))))))

(define (ceiling+/+ n d)
  (let ((q (quotient n d)) (r (remainder n d)))
    (if (zero? r)
        (values q r)
        (values (+ q 1) (- r d)))))

(define (ceiling-quotient n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (receive (q r) (ceiling-/- n d) r q))
            ((negative? n) (- 0 (quotient (- 0 n) d)))
            ((negative? d) (- 0 (quotient n (- 0 d))))
            (else (receive (q r) (ceiling+/+ n d) r q)))
      (ceiling (/ n d))))

(define (ceiling-remainder n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (receive (q r) (ceiling-/- n d) q r))
            ((negative? n) (- 0 (remainder (- 0 n) d)))
            ((negative? d) (remainder n (- 0 d)))
            (else (receive (q r) (ceiling+/+ n d) q r)))
      (- n (* d (ceiling (/ n d))))))

;;;; Euclidean Division

;;; 0 <= r < |d|

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

(define (euclidean-quotient n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (receive (q r) (ceiling-/- n d) r q))
            ((negative? n) (receive (q r) (floor-/+ n d) r q))
            ((negative? d) (- 0 (quotient n (- 0 d))))
            (else (quotient n d)))
      (if (negative? d) (ceiling (/ n d)) (floor (/ n d)))))

(define (euclidean-remainder n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (receive (q r) (ceiling-/- n d) q r))
            ((negative? n) (receive (q r) (floor-/+ n d) q r))
            ((negative? d) (remainder n (- 0 d)))
            (else (remainder n d)))
      (- n (* d (if (negative? d) (ceiling (/ n d)) (floor (/ n d)))))))

;;;; Floor

(define (floor/ n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (let ((n (- 0 n)) (d (- 0 d)))
               (values (quotient n d) (- 0 (remainder n d)))))
            ((negative? n) (floor-/+ n d))
            ((negative? d) (floor+/- n d))
            (else (values (quotient n d) (remainder n d))))
      (let ((q (floor (/ n d))))
        (values q (- n (* d q))))))

(define (floor-/+ n d)
  (let ((n (- 0 n)))
    (let ((q (quotient n d)) (r (remainder n d)))
      (if (zero? r)
          (values (- 0 q) r)
          (values (- (- 0 q) 1) (- d r))))))

(define (floor+/- n d)
  (let ((d (- 0 d)))
    (let ((q (quotient n d)) (r (remainder n d)))
      (if (zero? r)
          (values (- 0 q) r)
          (values (- (- 0 q) 1) (- r d))))))

(define (floor-quotient n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d)) (quotient (- 0 n) (- 0 d)))
            ((negative? n) (receive (q r) (floor-/+ n d) r q))
            ((negative? d) (receive (q r) (floor+/- n d) r q))
            (else (quotient n d)))
      (floor (/ n d))))

(define (floor-remainder n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (- 0 (remainder (- 0 n) (- 0 d))))
            ((negative? n) (receive (q r) (floor-/+ n d) q r))
            ((negative? d) (receive (q r) (floor+/- n d) q r))
            (else (remainder n d)))
      (- n (* d (floor (/ n d))))))

;;;; Round Ties to Even

(define (round/ n d)
  (define (divide n d adjust leave)
    (let ((q (quotient n d)) (r (remainder n d)))
      (if (and (not (zero? r))
               (or (and (odd? q) (even? d) (divisible? n (quotient d 2)))
                   (< d (* 2 r))))
          (adjust (+ q 1) (- r d))
          (leave q r))))
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (divide (- 0 n) (- 0 d)
               (lambda (q r) (values q (- 0 r)))
               (lambda (q r) (values q (- 0 r)))))
            ((negative? n)
             (divide (- 0 n) d
               (lambda (q r) (values (- 0 q) (- 0 r)))
               (lambda (q r) (values (- 0 q) (- 0 r)))))
            ((negative? d)
             (divide n (- 0 d)
               (lambda (q r) (values (- 0 q) r))
               (lambda (q r) (values (- 0 q) r))))
            (else
             (let ((return (lambda (q r) (values q r))))
               (divide n d return return))))
      (let ((q (round (/ n d))))
        (values q (- n (* d q))))))

(define (divisible? n d)
  ;; This operation admits a faster implementation than the one given
  ;; here.
  (zero? (remainder n d)))

(define (round-quotient n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (receive (q r) (round/ n d)
        r                               ;ignore
        q)
      (round (/ n d))))

(define (round-remainder n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (receive (q r) (round/ n d)
        q                               ;ignore
        r)
      (- n (* d (round (/ n d))))))

;;;; Truncate

(define (truncate/ n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (let ((n (- 0 n)) (d (- 0 d)))
               (values (quotient n d) (- 0 (remainder n d)))))
            ((negative? n)
             (let ((n (- 0 n)))
               (values (- 0 (quotient n d)) (- 0 (remainder n d)))))
            ((negative? d)
             (let ((d (- 0 d)))
               (values (- 0 (quotient n d)) (remainder n d))))
            (else
             (values (quotient n d) (remainder n d))))
      (let ((q (truncate (/ n d))))
        (values q (- n (* d q))))))

(define (truncate-quotient n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d)) (quotient (- 0 n) (- 0 d)))
            ((negative? n) (- 0 (quotient (- 0 n) d)))
            ((negative? d) (- 0 (quotient n (- 0 d))))
            (else (quotient n d)))
      (truncate (/ n d))))

(define (truncate-remainder n d)
  (if (and (exact-integer? n) (exact-integer? d))
      (cond ((and (negative? n) (negative? d))
             (- 0 (remainder (- 0 n) (- 0 d))))
            ((negative? n) (- 0 (remainder (- 0 n) d)))
            ((negative? d) (remainder n (- 0 d)))
            (else (remainder n d)))
      (- n (* d (truncate (/ n d))))))

;;;; Balanced (Copyright 2015 William D Clinger)

(define (balanced/ x y)
  (call-with-values
   (lambda () (euclidean/ x y))
   (lambda (q r)
     (cond ((< r (abs (/ y 2)))
            (values q r))
           ((> y 0)
            (values (+ q 1) (- x (* (+ q 1) y))))
           (else
            (values (- q 1) (- x (* (- q 1) y))))))))

(define (balanced-quotient x y)
  (call-with-values
   (lambda () (balanced/ x y))
   (lambda (q r) q)))

(define (balanced-remainder x y)
  (call-with-values
   (lambda () (balanced/ x y))
   (lambda (q r) r)))

))
