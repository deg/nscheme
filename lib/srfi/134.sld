;;; (srfi 134) — Immutable Deques.
;;;
;;; Vendored from the SRFI 134 reference implementation (the simple
;;; two-list "banker's deque" variant by Shiro Kawai, 2015). Flattened
;;; into a single file: the upstream .sld uses (include "ideque-impl.scm")
;;; and imports (srfi 1), (srfi 9), and (srfi 121); nscheme has none of
;;; those vendored and resolves `include` relative to the process
;;; directory, so the implementation body is inlined here inside `begin`,
;;; and the handful of SRFI 1 list helpers it relies on (plus the one
;;; SRFI 121 helper, generator->list) are defined inline as well. The
;;; deque algorithm itself is unchanged from upstream.
;;;
;;; SPDX-License-Identifier: MIT
;;; Copyright (c) 2015 Shiro Kawai <shiro@acm.org>
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
;;; EXPRESS OR IMPLIED. See the SRFI 134 document for full text.

(define-library (srfi 134)

  (import (scheme base)
          (scheme case-lambda))

  (export ideque ideque-tabulate ideque-unfold ideque-unfold-right
          ideque? ideque-empty? ideque= ideque-any ideque-every

          ideque-front ideque-add-front ideque-remove-front
          ideque-back  ideque-add-back  ideque-remove-back

          ideque-ref
          ideque-take ideque-take-right ideque-drop ideque-drop-right
          ideque-split-at

          ideque-length ideque-append ideque-reverse
          ideque-count ideque-zip

          ideque-map ideque-filter-map
          ideque-for-each ideque-for-each-right
          ideque-fold ideque-fold-right
          ideque-append-map

          ideque-filter ideque-remove ideque-partition

          ideque-find ideque-find-right
          ideque-take-while ideque-take-while-right
          ideque-drop-while ideque-drop-while-right
          ideque-span ideque-break

          list->ideque ideque->list
          generator->ideque ideque->generator)

  (begin

;;;; SRFI 1 subset and generator->list, inlined.
;;; nscheme provides neither (srfi 1) nor (srfi 121); the upstream
;;; implementation uses the list helpers below plus generator->list.
;;; These mirror the SRFI 1 / SRFI 121 semantics for the cases the
;;; deque code exercises (all arguments are proper lists / a thunk
;;; generator that yields an eof-object when exhausted).

(define (take lis n)
  (let loop ((lis lis) (n n) (acc '()))
    (if (or (<= n 0) (null? lis))
        (reverse acc)
        (loop (cdr lis) (- n 1) (cons (car lis) acc)))))

(define (drop lis n)
  (let loop ((lis lis) (n n))
    (if (or (<= n 0) (null? lis))
        lis
        (loop (cdr lis) (- n 1)))))

(define (take-right lis n)
  (drop lis (- (length lis) n)))

(define (list-tabulate n proc)
  (let loop ((i (- n 1)) (acc '()))
    (if (< i 0)
        acc
        (loop (- i 1) (cons (proc i) acc)))))

(define (unfold p f g seed)
  (if (p seed)
      '()
      (cons (f seed) (unfold p f g (g seed)))))

(define (unfold-right p f g seed)
  (let loop ((seed seed) (acc '()))
    (if (p seed)
        acc
        (loop (g seed) (cons (f seed) acc)))))

(define (concatenate lists)
  (apply append lists))

(define (count pred lis)
  (let loop ((lis lis) (n 0))
    (cond ((null? lis) n)
          ((pred (car lis)) (loop (cdr lis) (+ n 1)))
          (else (loop (cdr lis) n)))))

(define (fold proc knil lis)
  (let loop ((lis lis) (acc knil))
    (if (null? lis)
        acc
        (loop (cdr lis) (proc (car lis) acc)))))

(define (fold-right proc knil lis)
  (if (null? lis)
      knil
      (proc (car lis) (fold-right proc knil (cdr lis)))))

(define (filter pred lis)
  (let loop ((lis lis) (acc '()))
    (cond ((null? lis) (reverse acc))
          ((pred (car lis)) (loop (cdr lis) (cons (car lis) acc)))
          (else (loop (cdr lis) acc)))))

(define (remove pred lis)
  (filter (lambda (x) (not (pred x))) lis))

(define (partition pred lis)
  (let loop ((lis lis) (yes '()) (no '()))
    (cond ((null? lis) (values (reverse yes) (reverse no)))
          ((pred (car lis)) (loop (cdr lis) (cons (car lis) yes) no))
          (else (loop (cdr lis) yes (cons (car lis) no))))))

(define (filter-map proc lis)
  (let loop ((lis lis) (acc '()))
    (if (null? lis)
        (reverse acc)
        (let ((x (proc (car lis))))
          (if x
              (loop (cdr lis) (cons x acc))
              (loop (cdr lis) acc))))))

(define (append-map proc lis)
  (concatenate (map proc lis)))

(define (span pred lis)
  (let loop ((lis lis) (acc '()))
    (if (and (pair? lis) (pred (car lis)))
        (loop (cdr lis) (cons (car lis) acc))
        (values (reverse acc) lis))))

(define (break pred lis)
  (span (lambda (x) (not (pred x))) lis))

(define (any pred lis)
  (and (pair? lis)
       (let loop ((head (car lis)) (tail (cdr lis)))
         (if (null? tail)
             (pred head)
             (or (pred head)
                 (loop (car tail) (cdr tail)))))))

(define (every pred lis)
  (or (null? lis)
      (let loop ((head (car lis)) (tail (cdr lis)))
        (if (null? tail)
            (pred head)
            (and (pred head)
                 (loop (car tail) (cdr tail)))))))

(define (list= elt= . lists)
  (or (null? lists)
      (let loop ((a (car lists)) (rest (cdr lists)))
        (or (null? rest)
            (let ((b (car rest)))
              ;; Compare a and b without consuming b for the next
              ;; round: advance the loop with the original b so each
              ;; adjacent pair (list1=list2, list2=list3, …) is checked.
              (and (let pair-loop ((a a) (b b))
                     (cond ((and (null? a) (null? b)) #t)
                           ((or (null? a) (null? b)) #f)
                           ((elt= (car a) (car b)) (pair-loop (cdr a) (cdr b)))
                           (else #f)))
                   (loop b (cdr rest))))))))

(define (zip lis . lists)
  (apply map list lis lists))

;; SRFI 121: drain a generator (a thunk yielding an eof-object when
;; exhausted) into a list, preserving order.
(define (generator->list gen)
  (let loop ((acc '()))
    (let ((v (gen)))
      (if (eof-object? v)
          (reverse acc)
          (loop (cons v acc))))))

;;;; SRFI 134 reference implementation (two-list banker's deque).

;; some compatibility stuff
(define-syntax receive
  (syntax-rules ()
    ((_ binds mv-expr body ...)
     (let-values ((binds mv-expr)) body ...))))

;;;
;;; Record
;;;

(define-record-type <ideque> (%make-dq lenf f lenr r) ideque?
  (lenf dq-lenf)  ; length of front chain
  (f    dq-f)     ; front chain
  (lenr dq-lenr)  ; length of rear chain
  (r    dq-r))    ; rear chain

;; We use a singleton for empty deque
(define *empty* (%make-dq 0 '() 0 '()))

;; Common type checker
(define (%check-ideque x)
  (unless (ideque? x)
    (error "ideque expected, but got:" x)))

;;;
;;; Constructors
;;;

;; API
(define (ideque . args) (list->ideque args))

;; API
(define (ideque-tabulate size init)
  (let ((lenf (quotient size 2))
        (lenr (quotient (+ size 1) 2)))
    (%make-dq lenf (list-tabulate lenf init)
              lenr (unfold (lambda (n) (= n lenr))
                           (lambda (n) (init (- size n 1)))
                           (lambda (n) (+ n 1))
                           0))))

;; API
(define (ideque-unfold p f g seed)
  (list->ideque (unfold p f g seed)))

;; API
(define (ideque-unfold-right p f g seed)
  (list->ideque (unfold-right p f g seed)))
;; alternatively:
;; (ideque-reverse (list->ideque (unfold p f g seed)))

;; Internal constructor.  Returns a new ideque, with balancing 'front' and
;; 'rear' chains.  (The name 'check' comes from Okasaki's book.)

(define C 3)

(define (check lenf f lenr r)
  (cond ((> lenf (+ (* lenr C) 1))
         (let* ((i (quotient (+ lenf lenr) 2))
                (j (- (+ lenf lenr) i))
                (f. (take f i))
                (r. (append r (reverse (drop f i)))))
           (%make-dq i f. j r.)))
        ((> lenr (+ (* lenf C) 1))
         (let* ((j (quotient (+ lenf lenr) 2))
                (i (- (+ lenf lenr) j))
                (r. (take r j))
                (f. (append f (reverse (drop r j)))))
           (%make-dq i f. j r.)))
        (else (%make-dq lenf f lenr r))))

;;;
;;; Basic operations
;;;

;; API
(define (ideque-empty? dq)
  (%check-ideque dq)
  (and (zero? (dq-lenf dq))
       (zero? (dq-lenr dq))))

;; API
(define (ideque-add-front dq x)
  (%check-ideque dq)
  (check (+ (dq-lenf dq) 1) (cons x (dq-f dq)) (dq-lenr dq) (dq-r dq)))

;; API
(define (ideque-front dq)
  (%check-ideque dq)
  (if (zero? (dq-lenf dq))
    (if (zero? (dq-lenr dq))
      (error "Empty deque:" dq)
      (car (dq-r dq)))
    (car (dq-f dq))))

;; API
(define (ideque-remove-front dq)
  (%check-ideque dq)
  (if (zero? (dq-lenf dq))
    (if (zero? (dq-lenr dq))
      (error "Empty deque:" dq)
      *empty*)
    (check (- (dq-lenf dq) 1) (cdr (dq-f dq)) (dq-lenr dq) (dq-r dq))))

;; API
(define (ideque-add-back dq x)
  (%check-ideque dq)
  (check (dq-lenf dq) (dq-f dq) (+ (dq-lenr dq) 1) (cons x (dq-r dq))))

;; API
(define (ideque-back dq)
  (%check-ideque dq)
  (if (zero? (dq-lenr dq))
    (if (zero? (dq-lenf dq))
      (error "Empty deque:" dq)
      (car (dq-f dq)))
    (car (dq-r dq))))

;; API
(define (ideque-remove-back dq)
  (%check-ideque dq)
  (if (zero? (dq-lenr dq))
    (if (zero? (dq-lenf dq))
      (error "Empty deque:" dq)
      *empty*)
    (check (dq-lenf dq) (dq-f dq) (- (dq-lenr dq) 1) (cdr (dq-r dq)))))

;; API
(define (ideque-reverse dq)
  (%check-ideque dq)
  (if (ideque-empty? dq)
    *empty*
    (%make-dq (dq-lenr dq) (dq-r dq) (dq-lenf dq) (dq-f dq))))

;;
;; Other operations
;;

;; API
(define ideque=
  (case-lambda
    ((elt=) #t)
    ((elt= ideque) (%check-ideque ideque) #t)
    ((elt= dq1 dq2)
     ;; we optimize two-arg case
     (%check-ideque dq1)
     (%check-ideque dq2)
     (or (eq? dq1 dq2)
         (let ((len1 (+ (dq-lenf dq1) (dq-lenr dq1)))
               (len2 (+ (dq-lenf dq2) (dq-lenr dq2))))
           (and (= len1 len2)
                (receive (x t1 t2) (list-prefix= elt= (dq-f dq1) (dq-f dq2))
                  (and x
                       (receive (y r1 r2) (list-prefix= elt= (dq-r dq1) (dq-r dq2))
                         (and y
                              (if (null? t1)
                                (list= elt= t2 (reverse r1))
                                (list= elt= t1 (reverse r2)))))))))))
    ((elt= . dqs)
     ;; The comparison scheme is the same as srfi-1's list=.
     (apply list= elt= (map ideque->list dqs)))))

;; Compare two lists up to whichever shorter one.
;; Returns the compare result and the tails of uncompared lists.
(define (list-prefix= elt= a b)
  (let loop ((a a) (b b))
    (cond ((or (null? a) (null? b)) (values #t a b))
          ((elt= (car a) (car b)) (loop (cdr a) (cdr b)))
          (else (values #f a b)))))

;; API
(define (ideque-ref dq n)
  (%check-ideque dq)
  (let ((len (+ (dq-lenf dq) (dq-lenr dq))))
    (cond ((or (< n 0) (>= n len)) (error "Index out of range:" n))
          ((< n (dq-lenf dq)) (list-ref (dq-f dq) n))
          (else (list-ref (dq-r dq) (- len n 1))))))

(define (%ideque-take dq n)             ; n is within the range
  (let ((lenf (dq-lenf dq))
        (f    (dq-f dq)))
    (if (<= n lenf)
      (check n (take f n) 0 '())
      (let ((lenr. (- n lenf)))
        (check lenf f lenr. (take-right (dq-r dq) lenr.))))))

(define (%ideque-drop dq n)             ; n is within the range
  (let ((lenf (dq-lenf dq))
        (f    (dq-f dq))
        (lenr (dq-lenr dq))
        (r    (dq-r dq)))
    (if (<= n lenf)
      (check (- lenf n) (drop f n) lenr r)
      (let ((lenr. (- lenr (- n lenf))))
        (check 0 '() lenr. (take r lenr.))))))

(define (%check-length dq n)
  (unless (<= 0 n (ideque-length dq))
    (error "argument is out of range:" n)))

;; API
(define (ideque-take dq n)
  (%check-ideque dq)
  (%check-length dq n)
  (%ideque-take dq n))

;; API
(define (ideque-take-right dq n)
  (%check-ideque dq)
  (%check-length dq n)
  (%ideque-drop dq (- (ideque-length dq) n)))

;; API
(define (ideque-drop dq n)
  (%check-ideque dq)
  (%check-length dq n)
  (%ideque-drop dq n))

;; API
(define (ideque-drop-right dq n)
  (%check-ideque dq)
  (%check-length dq n)
  (%ideque-take dq (- (ideque-length dq) n)))

;; API
(define (ideque-split-at dq n)
  (%check-ideque dq)
  (%check-length dq n)
  (values (%ideque-take dq n)
          (%ideque-drop dq n)))

;; API
(define (ideque-length dq)
  (%check-ideque dq)
  (+ (dq-lenf dq) (dq-lenr dq)))

;; API
(define (ideque-append . dqs)
  ;; We could save some list copying by carefully split dqs into front and
  ;; rear groups and append separately, but for now we don't bother...
  (list->ideque (concatenate (map ideque->list dqs))))

;; API
(define (ideque-count pred dq)
  (%check-ideque dq)
  (+ (count pred (dq-f dq)) (count pred (dq-r dq))))

;; API
(define (ideque-zip dq . dqs)
  ;; An easy way.
  (let ((elts (apply zip (ideque->list dq) (map ideque->list dqs))))
    (check (length elts) elts 0 '())))

;; API
(define (ideque-map proc dq)
  (%check-ideque dq)
  (%make-dq (dq-lenf dq) (map proc (dq-f dq))
            (dq-lenr dq) (map proc (dq-r dq))))

;; API
(define (ideque-filter-map proc dq)
  (%check-ideque dq)
  (let ((f (filter-map proc (dq-f dq)))
        (r (filter-map proc (dq-r dq))))
    (check (length f) f (length r) r)))

;; API
(define (ideque-for-each proc dq)
  (%check-ideque dq)
  (for-each proc (dq-f dq))
  (for-each proc (reverse (dq-r dq))))

;; API
(define (ideque-for-each-right proc dq)
  (%check-ideque dq)
  (for-each proc (dq-r dq))
  (for-each proc (reverse (dq-f dq))))

;; API
(define (ideque-fold proc knil dq)
  (%check-ideque dq)
  (fold proc (fold proc knil (dq-f dq)) (reverse (dq-r dq))))

;; API
(define (ideque-fold-right proc knil dq)
  (%check-ideque dq)
  (fold-right proc (fold-right proc knil (reverse (dq-r dq))) (dq-f dq)))

;; API
(define (ideque-append-map proc dq)
  ;; can be cleverer, but for now...
  (list->ideque (append-map proc (ideque->list dq))))

(define (%ideque-filter-remove op pred dq)
  (%check-ideque dq)
  (let ((f (op pred (dq-f dq)))
        (r (op pred (dq-r dq))))
    (check (length f) f (length r) r)))

;; API
(define (ideque-filter pred dq) (%ideque-filter-remove filter pred dq))
(define (ideque-remove pred dq) (%ideque-filter-remove remove pred dq))

;; API
(define (ideque-partition pred dq)
  (%check-ideque dq)
  (receive (f1 f2) (partition pred (dq-f dq))
    (receive (r1 r2) (partition pred (dq-r dq))
      (values (check (length f1) f1 (length r1) r1)
              (check (length f2) f2 (length r2) r2)))))

(define *not-found* (cons #f #f)) ; unique value

(define (%search pred seq1 seq2 failure)
  ;; We could write seek as CPS, but we employ *not-found* instead to avoid
  ;; closure allocation.
  (define (seek pred s)
    (cond ((null? s) *not-found*)
          ((pred (car s)) (car s))
          (else (seek pred (cdr s)))))
  (let ((r (seek pred seq1)))
    (if (not (eq? r *not-found*))
      r
      (let ((r (seek pred (reverse seq2))))
        (if (not (eq? r *not-found*))
          r
          (failure))))))

;; API
(define (ideque-find pred dq . opts)
  (%check-ideque dq)
  (let ((failure (if (pair? opts) (car opts) (lambda () #f))))
    (%search pred (dq-f dq) (dq-r dq) failure)))

;; API
(define (ideque-find-right pred dq . opts)
  (%check-ideque dq)
  (let ((failure (if (pair? opts) (car opts) (lambda () #f))))
    (%search pred (dq-r dq) (dq-f dq) failure)))

;; API
(define (ideque-take-while pred dq)
  (%check-ideque dq)
  (receive (hd tl) (span pred (dq-f dq))
    (if (null? tl)
      (receive (hd. tl.) (span pred (reverse (dq-r dq)))
        (check (dq-lenf dq) (dq-f dq) (length hd.) (reverse hd.)))
      (check (length hd) hd 0 '()))))

;; API
(define (ideque-take-while-right pred dq)
  (%check-ideque dq)
  (ideque-reverse (ideque-take-while pred (ideque-reverse dq))))

;; API
(define (ideque-drop-while pred dq)
  (%check-ideque dq)
  (receive (hd tl) (span pred (dq-f dq))
    (if (null? tl)
      (receive (hd. tl.) (span pred (reverse (dq-r dq)))
        (check (length tl.) tl. 0 '()))
      (check (length tl) tl (dq-lenr dq) (dq-r dq)))))

;; API
(define (ideque-drop-while-right pred dq)
  (%check-ideque dq)
  (ideque-reverse (ideque-drop-while pred (ideque-reverse dq))))

(define (%idq-span-break op pred dq)
  (%check-ideque dq)
  (receive (head tail) (op pred (dq-f dq))
    (if (null? tail)
      (receive (head. tail.) (op pred (reverse (dq-r dq)))
        (values (check (length head) head (length head.) (reverse head.))
                (check (length tail.) tail. 0 '())))
      (values (check (length head) head 0 '())
              (check (length tail) tail (dq-lenr dq) (dq-r dq))))))

;; API
(define (ideque-span pred dq) (%idq-span-break span pred dq))
(define (ideque-break pred dq) (%idq-span-break break pred dq))

;; API
(define (ideque-any pred dq)
  (%check-ideque dq)
  (if (null? (dq-r dq))
    (any pred (dq-f dq))
    (or (any pred (dq-f dq)) (any pred (reverse (dq-r dq))))))

;; API
(define (ideque-every pred dq)
  (%check-ideque dq)
  (if (null? (dq-r dq))
    (every pred (dq-f dq))
    (and (every pred (dq-f dq)) (every pred (reverse (dq-r dq))))))

;; API
(define (ideque->list dq)
  (%check-ideque dq)
  (append (dq-f dq) (reverse (dq-r dq))))

;; API
(define (list->ideque lis) (check (length lis) lis 0 '()))

;; API
(define (ideque->generator dq)
  (%check-ideque dq)
  (lambda ()
    (if (ideque-empty? dq)
      (eof-object)
      (let ((v (ideque-front dq)))
        (set! dq (ideque-remove-front dq))
        v))))

;; API
(define (generator->ideque gen)
  (list->ideque (generator->list gen)))

))
