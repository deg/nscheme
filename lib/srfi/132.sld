;;; (srfi 132) — Sort Libraries.
;;;
;;; Vendored from the SRFI 132 reference implementation.  The sorting
;;; algorithms are Olin Shivers's SRFI-32 sort package (1998, 2002); the
;;; selection/median procedures and the R7RS library packaging are by
;;; William D Clinger (2016).  Both carry permissive licences, preserved
;;; below.  Flattened into a single file: the upstream .sld pulls in a
;;; dozen `(include "X.scm")` body files, but nscheme resolves `include`
;;; relative to the process directory, so every body file is inlined here
;;; inside `begin` instead.
;;;
;;; Upstream notices
;;; ----------------
;;; The sort algorithms (delndups, lmsort, sortp, vector-util, vhsort,
;;; visort, vmsort, vqsort2, vqsort3, sort) are:
;;;
;;;     Copyright (c) 1998, 2002 by Olin Shivers.
;;;     You may do as you please with this code, as long as you do not
;;;     delete this notice or hold me responsible for any outcome related
;;;     to its use.  -- Olin Shivers
;;;
;;; The selection code (select.scm) and the SRFI 132 test program are:
;;;
;;;     Copyright (C) William D Clinger (2016).
;;;     Permission is hereby granted, free of charge, to any person
;;;     obtaining a copy of this software and associated documentation
;;;     files (the "Software"), to deal in the Software without
;;;     restriction, including without limitation the rights to use, copy,
;;;     modify, merge, publish, distribute, sublicense, and/or sell copies
;;;     of the Software, and to permit persons to whom the Software is
;;;     furnished to do so, subject to the following conditions:
;;;     The above copyright notice and this permission notice shall be
;;;     included in all copies or substantial portions of the Software.
;;;     THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.
;;;
;;; SPDX-License-Identifier: MIT
;;;
;;; nscheme adaptation notes
;;; ------------------------
;;; * The upstream library imports `(only (srfi 27) random-integer)`
;;;   UNCONDITIONALLY, used only to pick a random pivot inside the
;;;   quickselect routines (%%vector-select / %%vector-select2).  nscheme
;;;   does not provide SRFI 27, so we supply a small self-contained
;;;   deterministic `random-integer` below.  Quickselect's correctness
;;;   does not depend on the pivot being *random* — any index in
;;;   [0, n) is a valid pivot — so results are identical; only the
;;;   (already worst-case O(n^2)) timing distribution differs.
;;; * The two upstream `(cond-expand ((library (rnrs ...)) ...))` clauses
;;;   are preserved verbatim; nscheme has neither (rnrs base) nor
;;;   (rnrs sorting), so both take their `else` branch (defining a local
;;;   `assert`, and using Olin's reference sort code).

(define-library (srfi 132)

  (export list-sorted?               vector-sorted?
          list-sort                  vector-sort
          list-stable-sort           vector-stable-sort
          list-sort!                 vector-sort!
          list-stable-sort!          vector-stable-sort!
          list-merge                 vector-merge
          list-merge!                vector-merge!
          list-delete-neighbor-dups  vector-delete-neighbor-dups
          list-delete-neighbor-dups! vector-delete-neighbor-dups!
          vector-find-median         vector-find-median!
          vector-select!             vector-separate!
          )

  (import (except (scheme base) vector-copy vector-copy! vector-fill!)
          (rename (only (scheme base) vector-copy vector-copy! vector-fill!)
                  (vector-copy  r7rs-vector-copy)
                  (vector-copy! r7rs-vector-copy!)
                  (vector-fill! r7rs-vector-fill!))
          (scheme cxr))

  ;; (srfi 27) is unavailable in nscheme; define a deterministic stand-in
  ;; for `random-integer`.  See the adaptation notes above.
  (begin
    (define %rng-state% 305419896)            ; arbitrary nonzero seed
    (define (random-integer n)
      ;; A tiny xorshift-ish LCG, masked to a nonneg fixnum and reduced
      ;; mod n.  Returns an integer in [0, n) for n > 0.
      (set! %rng-state%
            (modulo (+ (* %rng-state% 1103515245) 12345) 2147483648))
      (if (<= n 1) 0 (modulo %rng-state% n))))

  ;; Upstream cond-expand for `assert`: nscheme lacks (rnrs base), so the
  ;; else branch defines it.
  (cond-expand
   ((library (rnrs base))
    (import (only (rnrs base) assert)))
   (else
    (begin
     (define (assert x)
       (if (not x)
           (error "assertion failure"))))))

  (begin

;;; ============================ delndups.scm ============================
;;; The sort package -- delete neighboring duplicate elts
;;; Copyright (c) 1998 by Olin Shivers.

(define (list-delete-neighbor-dups = lis)
  (if (pair? lis)
      (let* ((x0 (car lis))
	     (xs (cdr lis))
	     (ans (let recur ((x0 x0) (xs xs))
		    (if (pair? xs)
			(let ((x1  (car xs))
			      (x2+ (cdr xs)))
			  (if (= x0 x1)
			      (recur x0 x2+)
			      (let ((ans-tail (recur x1 x2+)))
				(if (eq? ans-tail x2+) xs
				    (cons x1 ans-tail)))))
			xs))))
	(if (eq? ans xs) lis (cons x0 ans)))

      lis))

(define (list-delete-neighbor-dups! = lis)
  (if (pair? lis)
      (let lp1 ((prev lis) (prev-elt (car lis)) (lis (cdr lis)))
	(if (pair? lis)
	    (let ((lis-elt (car lis))
		  (next (cdr lis)))
	      (if (= prev-elt lis-elt)

		  (let lp2 ((lis next))
		    (if (pair? lis)
			(let ((lis-elt (car lis))
			      (next (cdr lis)))
			  (if (= prev-elt lis-elt)
			      (lp2 next)
			      (begin (set-cdr! prev lis)
				     (lp1 lis lis-elt next))))
			(set-cdr! prev lis)))	; Ran off end => quit.

		  (lp1 lis lis-elt next))))))
  lis)


(define (vector-delete-neighbor-dups elt= v . maybe-start+end)
  (call-with-values
   (lambda () (vector-start+end v maybe-start+end))
   (lambda (start end)
     (if (< start end)
	 (let* ((x (vector-ref v start))
		(ans (let recur ((x x) (i start) (j 1))
		       (if (< i end)
			   (let ((y (vector-ref v i))
				 (nexti (+ i 1)))
			     (if (elt= x y)
				 (recur x nexti j)
				 (let ((ansvec (recur y nexti (+ j 1))))
				   (vector-set! ansvec j y)
				   ansvec)))
			   (make-vector j)))))
	   (vector-set! ans 0 x)
	   ans)
	 '#()))))


;;; Packs the surviving elements to the left, in range [start,end'),
;;; and returns END'.
(define (vector-delete-neighbor-dups! elt= v . maybe-start+end)
  (call-with-values
   (lambda () (vector-start+end v maybe-start+end))
   (lambda (start end)

     (if (>= start end)
	 end
	 (let skip ((j start) (vj (vector-ref v start)))
	   (let ((j+1 (+ j 1)))
	     (if (>= j+1 end)
		 end
		 (let ((vj+1 (vector-ref v j+1)))
		   (if (not (elt= vj vj+1))
		       (skip j+1 vj+1)

		       (let lp2 ((j j) (vj vj) (k (+ j 2)))
			 (let lp3 ((k k))
			   (if (>= k end)
			       (+ j 1) ; Done.
			       (let ((vk (vector-ref v k))
				     (k+1 (+ k 1)))
				 (if (elt= vj vk)
				     (lp3 k+1)
				     (let ((j+1 (+ j 1)))
				       (vector-set! v j+1 vk)
				       (lp2 j+1 vk k+1))))))))))))))))

;;; ============================= lmsort.scm =============================
;;; list merge & list merge-sort
;;; Copyright (c) 1998 by Olin Shivers.

(define-syntax mlet ; Multiple-value LET*
  (syntax-rules ()
    ((mlet ((() exp) rest ...) body ...)
     (begin exp (mlet (rest ...) body ...)))

    ((mlet (((var) exp) rest ...) body ...)
     (let ((var exp)) (mlet (rest ...) body ...)))

    ((mlet ((vars exp) rest ...) body ...)
     (call-with-values (lambda () exp)
       (lambda vars (mlet (rest ...) body ...))))

    ((mlet () body ...) (begin body ...))))


(define (list-merge-sort elt< lis)

  (define (getrun lis)
    (let lp ((ans '())  (i 1)  (prev (car lis))  (xs (cdr lis)))
      (if (pair? xs)
	  (let ((x (car xs)))
	    (if (elt< x prev)
		(values (append-reverse ans (cons prev '())) i xs)
		(lp (cons prev ans) (+ i 1) x (cdr xs))))
	  (values (append-reverse ans (cons prev '())) i xs))))

  (define (append-reverse rev-head tail)
    (let lp ((rev-head rev-head) (tail tail))
      (if (null-list? rev-head) tail
	  (lp (cdr rev-head) (cons (car rev-head) tail)))))

  (define (null-list? l)
    (cond ((pair? l) #f)
	  ((null? l) #t)
	  (else (error  "argument out of domain" l))))

  (define (merge a b)
    (let recur ((x (car a)) (a a)
		(y (car b)) (b b))
      (if (elt< y x)
	  (cons y (let ((b (cdr b)))
		    (if (pair? b)
			(recur x a (car b) b)
			a)))
	  (cons x (let ((a (cdr a)))
		    (if (pair? a)
			(recur (car a) a y b)
			b))))))

  (define (grow s ls ls2 u lw)	; The core of the sort algorithm.
    (if (or (<= lw ls) (not (pair? u)))	; Met quota or out of data?
	(values s ls u)			; If so, we're done.
	(mlet (((ls2) (let lp ((ls2 ls2))
			(let ((ls2*2 (+ ls2 ls2)))
			  (if (<= ls2*2 ls) (lp ls2*2) ls2))))
	       ((r lr u2)  (getrun u))			; Get a run, then
	       ((t lt u3)  (grow r lr 1 u2 ls2))) 	; grow it up to be T.
	  (grow (merge s t) (+ ls lt) 		; Merge S & T,
		(+ ls2 ls2) u3 lw))))	     		;   and loop.

  (if (pair? lis)				; Don't sort an empty list.
      (mlet (((r lr tail)  (getrun lis))	; Pick off an initial run,
	     ((infinity)   #o100000000)		; then grow it up maximally.
	     ((a la v)     (grow r lr 1 tail infinity)))
	a)
      '()))


(define (list-merge-sort! elt< lis)
  (define (getrun lis)
    (let lp ((lis lis) (x (car lis)) (i 1) (next (cdr lis)))
      (if (pair? next)
	  (let ((y (car next)))
	    (if (elt< y x)
		(values i lis next)
		(lp next y (+ i 1) (cdr next))))
	  (values i lis next))))

  (define (merge! a enda b endb)
    (letrec ((scan-a (lambda (prev  x a  y b)     ; Zip down A until we
		       (cond ((elt< y x)          ; find an elt > (CAR B).
			      (set-cdr! prev b)
			      (let ((next-b (cdr b)))
				(if (eq? b endb)
				    (begin (set-cdr! b a) enda) ; Done.
				    (scan-b b x a (car next-b) next-b))))

			     ((eq? a enda) (maybe-set-cdr! a b) endb) ; Done.

			     (else (let ((next-a (cdr a)))  ; Continue scan.
				     (scan-a a (car next-a) next-a y b))))))

	     (scan-b (lambda (prev  x a  y b)     ; Zip down B while its
		       (cond ((elt< y x)          ; elts are < (CAR A).
			      (if (eq? b endb)
				  (begin (set-cdr! b a) enda)      ; Done.
				  (let ((next-b (cdr b))) ; Continue scan.
				    (scan-b b x a (car next-b) next-b))))

			     (else (set-cdr! prev a)
				   (if (eq? a enda)
				       (begin (maybe-set-cdr! a b) endb) ; Done.
				       (let ((next-a (cdr a)))
					 (scan-a a (car next-a) next-a y b)))))))

	     (maybe-set-cdr! (lambda (pair val) (if (not (eq? (cdr pair) val))
						    (set-cdr! pair val)))))

      (let ((x (car a))  (y (car b)))
	(if (elt< y x)

	    (values b (if (eq? b endb)
			  (begin (set-cdr! b a) enda)
			  (let ((next-b (cdr b)))
			    (scan-b b x a (car next-b) next-b))))

	    (values a (if (eq? a enda)
			  (begin (maybe-set-cdr! a b) endb)
			  (let ((next-a (cdr a)))
			    (scan-a a (car next-a) next-a y b))))))))

  (define (grow s ends ls ls2 u lw)
    (if (and (pair? u) (< ls lw))

	(mlet (((ls2) (let lp ((ls2 ls2))
			(let ((ls2*2 (+ ls2 ls2)))
			  (if (<= ls2*2 ls) (lp ls2*2) ls2))))
	       ((lr endr u2)   (getrun u))		  ; Get a run from U;
	       ((t endt lt u3) (grow u endr lr 1 u2 ls2)) ; grow it up to be T.
	       ((st end-st)    (merge! s ends t endt)))	  ; Merge S & T,
	  (grow st end-st (+ ls lt) (+ ls2 ls2) u3 lw))	  ; then loop.

	(values s ends ls u))) ; Done -- met LW quota or ran out of data.

  (if (pair? lis)
      (mlet (((lr endr rest)  (getrun lis))	; Pick off an initial run.
	     ((infinity)      #o100000000)	; Then grow it up maximally.
             ((a enda la v)   (grow lis endr lr 1 rest infinity)))
        (set-cdr! enda '())			; Nil-terminate answer.
        a)					; We're done.

      '()))					; Don't sort an empty list.


(define (list-merge < a b)
  (cond ((not (pair? a)) b)
	((not (pair? b)) a)
	(else (let recur ((x (car a)) (a a)	; A is a pair; X = (CAR A).
			  (y (car b)) (b b))	; B is a pair; Y = (CAR B).
		(if (< y x)

		    (let ((b (cdr b)))
		      (if (pair? b)
			  (cons y (recur x a (car b) b))
			  (cons y a)))

		    (let ((a (cdr a)))
		      (if (pair? a)
			  (cons x (recur (car a) a y b))
			  (cons x b))))))))


(define (list-merge! < a b)
  (letrec ((scan-a (lambda (prev a x b y)		; Zip down A doing
		     (if (< y x)			; no SET-CDR!s until
			 (let ((next-b (cdr b)))	; we hit a B elt that
			   (set-cdr! prev b)		; has to be inserted.
			   (if (pair? next-b)
			       (scan-b b a x next-b (car next-b))
			       (set-cdr! b a)))

			 (let ((next-a (cdr a)))
			   (if (pair? next-a)
			       (scan-a a next-a (car next-a) b y)
			       (set-cdr! a b))))))

	   (scan-b (lambda (prev a x b y)		; Zip down B doing
		     (if (< y x)			; no SET-CDR!s until
			 (let ((next-b (cdr b)))	; we hit an A elt that
			   (if (pair? next-b)			  ; has to be
			       (scan-b b a x next-b (car next-b)) ; inserted.
			       (set-cdr! b a)))

			 (let ((next-a (cdr a)))
			   (set-cdr! prev a)
			   (if (pair? next-a)
			       (scan-a a next-a (car next-a) b y)
			       (set-cdr! a b)))))))

    (cond ((not (pair? a)) b)
	  ((not (pair? b)) a)

	  ((< (car b) (car a))
	   (let ((next-b (cdr b)))
	     (if (null? next-b)
		 (set-cdr! b a)
		 (scan-b b a (car a) next-b (car next-b))))
	   b)

	  (else (let ((next-a (cdr a)))
		  (if (null? next-a)
		      (set-cdr! a b)
		      (scan-a a next-a (car next-a) b (car b))))
		a))))

;;; ============================= sortp.scm ==============================
;;; The sort package -- sorted predicates
;;; Olin Shivers 10/98.

(define (list-sorted? < list)
  (or (not (pair? list))
      (let lp ((prev (car list)) (tail (cdr list)))
	(or (not (pair? tail))
	    (let ((next (car tail)))
	      (and (not (< next prev))
		   (lp next (cdr tail))))))))

(define (vector-sorted? elt< v . maybe-start+end)
  (call-with-values
   (lambda () (vector-start+end v maybe-start+end))
   (lambda (start end)
     (or (>= start end)			; Empty range
	 (let lp ((i (+ start 1)) (vi-1 (vector-ref v start)))
	   (or (>= i end)
	       (let ((vi (vector-ref v i)))
		 (and (not (elt< vi vi-1))
		      (lp (+ i 1) vi)))))))))

;;; ========================== vector-util.scm ==========================
;;; Copyright (c) 1998 by Olin Shivers.

(define (vector-portion-copy vec start end)
  (let* ((len (vector-length vec))
	 (new-len (- end start))
	 (new (make-vector new-len)))
    (do ((i start (+ i 1))
	 (j 0 (+ j 1)))
	((= i end) new)
      (vector-set! new j (vector-ref vec i)))))

(define (vector-copy vec)
  (vector-portion-copy vec 0 (vector-length vec)))

(define (vector-portion-copy! target src start end)
  (let ((len (- end start)))
    (do ((i (- len 1) (- i 1))
	 (j (- end 1) (- j 1)))
	((< i 0))
      (vector-set! target i (vector-ref src j)))))

(define (has-element list index)
  (cond
   ((zero? index)
    (if (pair? list)
	(values #t (car list))
	(values #f #f)))
   ((null? list)
    (values #f #f))
   (else
    (has-element (cdr list) (- index 1)))))

(define (list-ref-or-default list index default)
  (call-with-values
   (lambda () (has-element list index))
   (lambda (has? maybe)
     (if has?
	 maybe
	 default))))

(define (vector-start+end vector maybe-start+end)
  (let ((start (list-ref-or-default maybe-start+end
				    0 0))
	(end (list-ref-or-default maybe-start+end
				  1 (vector-length vector))))
    (values start end)))

(define (vectors-start+end-2 vector-1 vector-2 maybe-start+end)
  (let ((start-1 (list-ref-or-default maybe-start+end
				      0 0))
	(end-1 (list-ref-or-default maybe-start+end
				    1 (vector-length vector-1)))
	(start-2 (list-ref-or-default maybe-start+end
				      2 0))
	(end-2 (list-ref-or-default maybe-start+end
				    3 (vector-length vector-2))))
    (values start-1 end-1
	    start-2 end-2)))

;;; ============================= vhsort.scm =============================
;;; The sort package -- vector heap sort
;;; Copyright (c) 2002 by Olin Shivers.

(define (really-vector-heap-sort! elt< v start end)
  (define (restore-heap! end i)
    (let* ((vi (vector-ref v i))
	   (first-leaf (quotient (+ start end) 2)) ; Can fixnum overflow.
	   (final-k (let lp ((k i))
		      (if (>= k first-leaf)
			  k		; Leaf, so done.
			  (let* ((k*2-start (+ k (- k start))) ; Don't overflow.
				 (child1 (+ 1 k*2-start))
				 (child2 (+ 2 k*2-start))
				 (child1-val (vector-ref v child1)))
			    (call-with-values
			     (lambda ()
			       (if (< child2 end)
				   (let ((child2-val (vector-ref v child2)))
				     (if (elt< child2-val child1-val)
					 (values child1 child1-val)
					 (values child2 child2-val)))
				   (values child1 child1-val)))
			     (lambda (max-child max-child-val)
			       (cond ((elt< vi max-child-val)
				      (vector-set! v k max-child-val)
				      (lp max-child))
				     (else k))))))))) ; Done.
      (vector-set! v final-k vi)))

  (let ((first-leaf (quotient (+ start end) 2)))  ; Can fixnum overflow.
    (do ((i (- first-leaf 1) (- i 1)))
	((< i start))
      (restore-heap! end i)))

  (do ((i (- end 1) (- i 1)))
      ((<= i start))
    (let ((top (vector-ref v start)))
      (vector-set! v start (vector-ref v i))
      (vector-set! v i top)
      (restore-heap! i start))))

(define (vector-heap-sort! elt< v . maybe-start+end)
  (call-with-values
   (lambda () (vector-start+end v maybe-start+end))
   (lambda (start end)
     (really-vector-heap-sort! elt< v start end))))

(define (vector-heap-sort elt< v . maybe-start+end)
  (call-with-values
   (lambda () (vector-start+end v maybe-start+end))
   (lambda (start end)
     (let ((ans (vector-portion-copy v start end)))
       (really-vector-heap-sort! elt< ans 0 (- end start))
       ans))))

;;; ============================= visort.scm =============================
;;; The sort package -- stable vector insertion sort
;;; Copyright (c) 1998 by Olin Shivers.

(define (vector-insert-sort elt< v . maybe-start+end)
  (call-with-values
   (lambda () (vector-start+end v maybe-start+end))
   (lambda (start end)
    (let ((ans (vector-portion-copy v start end)))
      (%vector-insert-sort! elt< ans 0 (- end start))
      ans))))

(define (vector-insert-sort! < v . maybe-start+end)
  (call-with-values
   (lambda () (vector-start+end v maybe-start+end))
   (lambda (start end)
     (%vector-insert-sort! < v start end))))

(define (%vector-insert-sort! elt< v start end)
  (do ((i (+ 1 start) (+ i 1)))	; Invariant: [start,i) is sorted.
      ((>= i end))
    (let ((val (vector-ref v i)))
      (vector-set! v (let lp ((j i))		; J is the location of the
		       (if (<= j start)
			   start	; "hole" as it bubbles down.
			   (let* ((j-1 (- j 1))
				  (vj-1 (vector-ref v j-1)))
			     (cond ((elt< val vj-1)
				    (vector-set! v j vj-1)
				    (lp j-1))
				   (else j)))))
		   val))))

;;; ============================= vmsort.scm =============================
;;; The sort package -- stable vector merge & merge sort
;;; Copyright (c) 1998 by Olin Shivers.

(define (vector-merge < v1 v2 . maybe-starts+ends)
  (call-with-values
   (lambda () (vectors-start+end-2 v1 v2 maybe-starts+ends))
   (lambda (start1 end1 start2 end2)
     (let ((ans (make-vector (+ (- end1 start1) (- end2 start2)))))
       (%vector-merge! < ans v1 v2 0 start1 end1 start2 end2)
       ans))))

(define (vector-merge! < v v1 v2 . maybe-starts+ends)
  (call-with-values
   (lambda ()
     (if (pair? maybe-starts+ends)
	 (values (car maybe-starts+ends)
		 (cdr maybe-starts+ends))
	 (values 0
		 '())))
   (lambda (start rest)
     (call-with-values
      (lambda () (vectors-start+end-2 v1 v2 rest))
      (lambda (start1 end1 start2 end2)
	(%vector-merge! < v v1 v2 start start1 end1 start2 end2))))))


(define (%vector-merge! elt< v v1 v2 start start1 end1 start2 end2)
  (letrec ((vblit (lambda (fromv j i end) ; Blit FROMV[J,END) to V[I,?].
		    (let lp ((j j) (i i))
		      (vector-set! v i (vector-ref fromv j))
		      (let ((j (+ j 1)))
			(if (< j end) (lp j (+ i 1))))))))

    (cond ((<= end1 start1) (if (< start2 end2) (vblit v2 start2 start end2)))
          ((<= end2 start2) (vblit v1 start1 start end1))

	  (else (let lp ((i start)
			 (j start1)  (x (vector-ref v1 start1))
			 (k start2)  (y (vector-ref v2 start2)))
		  (let ((i1 (+ i 1)))	; "i+1" is a complex number in R4RS!
		    (if (elt< y x)
			(let ((k (+ k 1)))
			  (vector-set! v i y)
			  (if (< k end2)
			      (lp i1 j x k (vector-ref v2 k))
			      (vblit v1 j i1 end1)))
			(let ((j (+ j 1)))
			  (vector-set! v i x)
			  (if (< j end1)
			      (lp i1 j (vector-ref v1 j) k y)
			      (vblit v2 k i1 end2))))))))))


(define (vector-merge-sort! < v . maybe-args)
  (call-with-values
   (lambda () (vector-start+end v maybe-args))
   (lambda (start end)
     (let ((temp (if (and (pair? maybe-args) ; kludge
			  (pair? (cdr maybe-args))
			  (pair? (cddr maybe-args)))
		     (caddr maybe-args)
		     (vector-copy v))))
       (%vector-merge-sort! < v start end temp)))))

(define (vector-merge-sort < v . maybe-args)
  (call-with-values
   (lambda () (vector-start+end v maybe-args))
   (lambda (start end)
     (let ((ans (r7rs-vector-copy v start end)))
       (vector-merge-sort! < ans)
       ans))))


(define (%vector-merge-sort! elt< v0 l r temp0)
  (define (xor a b) (not (eq? a b)))

  (define (merge target v1 v2 l len1 len2 v2=target?)
    (letrec ((vblit (lambda (fromv j i end)    ; Blit FROMV[J,END) to TARGET[I,?]
		      (let lp ((j j) (i i))    ; J < END. The final copy.
			(vector-set! target i (vector-ref fromv j))
			(let ((j (+ j 1)))
			  (if (< j end) (lp j (+ i 1))))))))

      (let* ((r1 (+ l  len1))
	     (r2 (+ r1 len2)))
							; Invariants:
	(let lp ((n l)					; N is next index of
		 (j l)   (x (vector-ref v1 l))		;   TARGET to write.
		 (k r1)  (y (vector-ref v2 r1)))	; X = V1[J]
	  (let ((n+1 (+ n 1)))				; Y = V2[K]
	    (if (elt< y x)
		(let ((k (+ k 1)))
		  (vector-set! target n y)
		  (if (< k r2)
		      (lp n+1 j x k (vector-ref v2 k))
		      (vblit v1 j n+1 r1)))
		(let ((j (+ j 1)))
		  (vector-set! target n x)
		  (if (< j r1)
		      (lp n+1 j (vector-ref v1 j) k y)
		      (if (not v2=target?) (vblit v2 k n+1 r2))))))))))


  (define (getrun v l r)
    (let lp ((i (+ l 1))  (x (vector-ref v l)))
      (if (>= i r)
	  (- i l)
	  (let ((y (vector-ref v i)))
	    (if (elt< y x)
		(- i l)
		(lp (+ i 1) y))))))

  (if (< l r) ; Don't try to sort an empty range.
      (call-with-values
       (lambda ()
	 (let recur ((l l) (want (- r l)))
	   (let ((len (- r l)))
	     (let lp ((pfxlen (getrun v0 l r)) (pfxlen2 1)
		      (v v0) (temp temp0)
		      (v=v0? #t))
	       (if (or (>= pfxlen want) (= pfxlen len))
		   (values pfxlen v v=v0?)
		   (let ((pfxlen2 (let lp ((j pfxlen2))
				    (let ((j*2 (+ j j)))
				      (if (<= j pfxlen) (lp j*2) j))))
			 (tail-len (- len pfxlen)))
		     (call-with-values
		      (lambda ()
			(recur (+ pfxlen l) pfxlen2))
		      (lambda (nr-len nr-vec nrvec=v0?)
			(merge temp v nr-vec l pfxlen nr-len
			       (xor nrvec=v0? v=v0?))
			(lp (+ pfxlen nr-len) (+ pfxlen2 pfxlen2)
			    temp v (not v=v0?))))))))))
       (lambda (ignored-len ignored-ansvec ansvec=v0?)
	 (if (not ansvec=v0?)
             (r7rs-vector-copy! v0 l temp0 l r))))))

;;; ============================ vqsort2.scm =============================
;;; The SRFI-32 sort package -- quick sort
;;; Copyright (c) 2002 by Olin Shivers.

(define (vector-quick-sort! < v . maybe-start+end)
  (call-with-values
      (lambda () (vector-start+end v maybe-start+end))
    (lambda (start end)
      (%quick-sort! < v start end))))

(define (vector-quick-sort < v . maybe-start+end)
  (call-with-values
      (lambda () (vector-start+end v maybe-start+end))
    (lambda (start end)
      (let ((ans (make-vector (- end start))))
	(vector-portion-copy! ans v start end)
	(%quick-sort! < ans 0 (- end start))
	ans))))

(define (%quick-sort! elt< v start end)
  (define (swap l r n)
    (if (> n 0)
	(let ((x   (vector-ref v l))
	      (r-1 (- r 1)))
	  (vector-set! v l   (vector-ref v r-1))
	  (vector-set! v r-1 x)
	  (swap (+ l 1) r-1 (- n 1)))))

  (define (median v1 v2 v3)
    (call-with-values
	(lambda () (if (elt< v1 v2) (values v1 v2) (values v2 v1)))
      (lambda (little big)
	(if (elt< big v3)
	    big
	    (if (elt< little v3) v3 little)))))

  (let recur ((l start) (r end))	; Sort the range [l,r).
    (if (< 10 (- r l))	     ; Ten: the gospel according to Sedgewick.

	(let ((pivot (median (vector-ref v l)
			     (vector-ref v (quotient (+ l r) 2))
			     (vector-ref v (- r 1)))))

	  (letrec ((lscan (lambda (i j k m) ; left-to-right scan
			    (let lp ((i i) (j j))
			      (if (> j k)
				  (done i j m)
				  (let ((x (vector-ref v j)))
				    (cond ((elt< x pivot) (lp i (+ j 1)))

					  ((elt< pivot x) (rscan i j k m))

					  (else ; Equal
					   (if (< i j)
					       (begin (vector-set! v j (vector-ref v i))
						      (vector-set! v i x)))
					   (lp (+ i 1) (+ j 1)))))))))

		   (rscan (lambda (i j k m) ; right-to-left scan
			    (let lp ((k k) (m m))
			      (if (<= k j)
				  (done i j m)
				  (let* ((x (vector-ref v k)))
				    (cond ((elt< pivot x) (lp (- k 1) m))

					  ((elt< x pivot) ; Swap j & k & lscan.
					   (vector-set! v k (vector-ref v j))
					   (vector-set! v j x)
					   (lscan i (+ j 1) (- k 1) m))

					  (else	; x=pivot
					   (if (< k m)
					       (begin (vector-set! v k (vector-ref v m))
						      (vector-set! v m x)))
					   (lp (- k 1) (- m 1)))))))))

		   (done (lambda (i j m)
			   (let ((num< (- j i))
				 (num> (+ 1 (- m j)))
				 (num=l (- i l))
				 (num=r (- (- r m) 1)))
			     (swap l j (min num< num=l)) ; Swap ='s into
			     (swap j r (min num> num=r)) ; the middle.
			     (cond ((<= num< num>)
				    (recur l          (+ l num<))
				    (recur (- r num>) r))
				   (else
				    (recur (- r num>) r)
				    (recur l          (+ l num<))))))))

	    (let ((r-1 (- r 1)))
	      (lscan l l r-1 r-1))))

	;; Small segment => punt to insert sort.
	(%vector-insert-sort! elt< v l r))))

;;; ============================ vqsort3.scm =============================
;;; The SRFI-32 sort package -- three-way quick sort
;;; Copyright (c) 2002 by Olin Shivers.

(define (vector-quick-sort3! c v . maybe-start+end)
  (call-with-values
      (lambda () (vector-start+end v maybe-start+end))
    (lambda (start end)
      (%quick-sort3! c v start end))))

(define (vector-quick-sort3 c v . maybe-start+end)
  (call-with-values
      (lambda () (vector-start+end v maybe-start+end))
    (lambda (start end)
      (let ((ans (make-vector (- end start))))
	(vector-portion-copy! ans v start end)
	(%quick-sort3! c ans 0 (- end start))
	ans))))

(define (%quick-sort3! c v start end)
  (define (swap l r n)			; Little utility -- swap the N
    (if (> n 0)
	(let ((x (vector-ref v l))	; outer pairs of the range [l,r).
	      (r-1 (- r 1)))
	  (vector-set! v l (vector-ref v r-1))
	  (vector-set! v r-1 x)
	  (swap (+ l 1) r-1 (- n 1)))))

  (define (sort3 v1 v2 v3)
    (call-with-values
	(lambda () (if (< (c v1 v2) 0) (values v1 v2) (values v2 v1)))
      (lambda (little big)
	(if (< (c big v3) 0)
	    (values little big v3)
	    (if (< (c little v3) 0)
		(values little v3 big)
		(values v3 little big))))))

  (define (elt< v1 v2)
    (negative? (c v1 v2)))

  (let recur ((l start) (r end))      ; Sort the range [l,r).
    (if (< 10 (- r l))		      ; 10: the gospel according to Sedgewick.

	(let* ((r-1 (- r 1))				; Three handy
	       (mid (quotient (+ l r) 2))		; common
	       (l+1 (+ l 1))				; subexpressions
	       (pivot (call-with-values
			  (lambda ()
			    (sort3 (vector-ref v l)
				   (vector-ref v mid)
				   (vector-ref v r-1)))
			(lambda (lo piv hi)
			  (let ((tmp (vector-ref v l+1)))	; Put LO, PIV & HI
			    (vector-set! v l   piv)		; back into V
			    (vector-set! v r-1 hi)		; where they belong,
			    (vector-set! v l+1 lo)
			    (vector-set! v mid tmp)
			    piv)))))		; and return PIV as pivot.


	  (letrec ((lscan (lambda (i j k m) ; left-to-right scan
			    (let lp ((i i) (j j))
			      (if (> j k)
				  (done i j m)
				  (let* ((x (vector-ref v j))
					 (sign (c x pivot)))
				    (cond ((< sign 0) (lp i (+ j 1)))

					  ((= sign 0)
					   (if (< i j)
					       (begin (vector-set! v j (vector-ref v i))
						      (vector-set! v i x)))
					   (lp (+ i 1) (+ j 1)))

					  ((> sign 0) (rscan i j k m))))))))

		   (rscan (lambda (i j k m) ; right-to-left scan
			    (let lp ((k k) (m m))
			      (if (<= k j)
				  (done i j m)
				  (let* ((x (vector-ref v k))
					 (sign (c x pivot)))
				    (cond ((> sign 0) (lp (- k 1) m))

					   ((= sign 0)
					    (if (< k m)
						(begin (vector-set! v k (vector-ref v m))
						       (vector-set! v m x)))
					    (lp (- k 1) (- m 1)))

					   ((< sign 0) ; Swap j & k & lscan.
					    (vector-set! v k (vector-ref v j))
					    (vector-set! v j x)
					    (lscan i (+ j 1) (- k 1) m))))))))

		   (done (lambda (i j m)
			   (let ((num< (- j i))
				 (num> (+ 1 (- m j)))
				 (num=l (- i l))
				 (num=r (- (- r m) 1)))
			     (swap l j (min num< num=l))	; Swap ='s into
			     (swap j r (min num> num=r))	; the middle.
			     (cond ((<= num< num>)
				    (recur l          (+ l num<))
				    (recur (- r num>) r))
				   (else
				    (recur (- r num>) r)
				    (recur l          (+ l num<))))))))

	    (lscan l+1 (+ l 2) (- r 2) r-1)))

	;; Small segment => punt to insert sort.
	(%vector-insert-sort! elt< v l r))))

;;; ============================== sort.scm ==============================
;;; The sort package -- general sort & merge procedures
;;; Copyright (c) 1998 by Olin Shivers.

(define (list-sort < l)			; Sort lists by converting to
  (let ((v (list->vector l)))		; a vector and sorting that.
    (vector-heap-sort! < v)
    (vector->list v)))

(define list-sort! list-merge-sort!)

(define list-stable-sort  list-merge-sort)
(define list-stable-sort! list-merge-sort!)

(define vector-sort  vector-quick-sort)
(define vector-sort! vector-quick-sort!)

(define vector-stable-sort  vector-merge-sort)
(define vector-stable-sort! vector-merge-sort!)

;;; ============================= select.scm =============================
;;; Linear-time (average case) selection.
;;; Copyright (C) William D Clinger (2016).

(define (vector-find-median < v knil . rest)
  (let* ((mean (if (null? rest)
                   (lambda (a b) (/ (+ a b) 2))
                   (car rest)))
         (n (vector-length v)))
    (cond ((zero? n)
           knil)
          ((odd? n)
           (%vector-select < v (quotient n 2) 0 n))
          (else
           (call-with-values
            (lambda () (%vector-select2 < v (- (quotient n 2) 1) 0 n))
            (lambda (a b)
              (mean a b)))))))

(define (vector-find-median! < v knil . rest)
  (let* ((mean (if (null? rest)
                   (lambda (a b) (/ (+ a b) 2))
                   (car rest)))
         (n (vector-length v)))
    (vector-sort! < v)
    (cond ((zero? n)
           knil)
          ((odd? n)
           (vector-ref v (quotient n 2)))
          (else
           (mean (vector-ref v (- (quotient n 2) 1))
                 (vector-ref v (quotient n 2)))))))

(define (vector-select < v k . rest)
  (let* ((start (if (null? rest)
                    0
                    (car rest)))
         (end (if (and (pair? rest)
                       (pair? (cdr rest)))
                  (car (cdr rest))
                  (vector-length v))))
    (%vector-select < v k start end)))

(define vector-select! vector-select)

(define (vector-separate! < v k . rest)
  (let* ((start (if (null? rest)
                    0
                    (car rest)))
         (end (if (and (pair? rest)
                       (pair? (cdr rest)))
                  (car (cdr rest))
                  (vector-length v))))
    (if (and (> k 0)
             (> end start))
        (let ((pivot (vector-select < v (- k 1) start end)))
          (call-with-values
           (lambda () (count-smaller < pivot v start end 0 0))
           (lambda (count count2)
             (let* ((v2 (make-vector count))
                    (v3 (make-vector (- end start count count2))))
               (copy-smaller! < pivot v2 0 v start end)
               (copy-bigger! < pivot v3 0 v start end)
               (r7rs-vector-copy! v start v2)
               (r7rs-vector-fill! v pivot (+ start count) (+ start count count2))
               (r7rs-vector-copy! v (+ start count count2) v3))))))))

(define just-sort-it-threshold 50)

(define (%vector-select <? v k start end)
  (assert (and 'vector-select
               (procedure? <?)
               (vector? v)
               (exact-integer? k)
               (exact-integer? start)
               (exact-integer? end)
               (<= 0 k (- end start 1))
               (<= 0 start end (vector-length v))))
  (%%vector-select <? v k start end))

(define (%vector-select2 <? v k start end)
  (assert (and 'vector-select
               (procedure? <?)
               (vector? v)
               (exact-integer? k)
               (exact-integer? start)
               (exact-integer? end)
               (<= 0 k (- end start 1 1))
               (<= 0 start end (vector-length v))))
  (%%vector-select2 <? v k start end))

(define (%%vector-select <? v k start end)
  (let ((size (- end start)))
    (cond ((= 1 size)
           (vector-ref v (+ k start)))
          ((= 2 size)
           (cond ((<? (vector-ref v start)
                      (vector-ref v (+ start 1)))
                  (vector-ref v (+ k start)))
                 (else
                  (vector-ref v (+ (- 1 k) start)))))
          ((< size just-sort-it-threshold)
           (vector-ref (vector-sort <? (r7rs-vector-copy v start end)) k))
          (else
           (let* ((ip (random-integer size))
                  (pivot (vector-ref v (+ start ip))))
             (call-with-values
              (lambda () (count-smaller <? pivot v start end 0 0))
              (lambda (count count2)
                (cond ((< k count)
                       (let* ((n count)
                              (v2 (make-vector n)))
                         (copy-smaller! <? pivot v2 0 v start end)
                         (%%vector-select <? v2 k 0 n)))
                      ((< k (+ count count2))
                       pivot)
                      (else
                       (let* ((n (- size count count2))
                              (v2 (make-vector n))
                              (k2 (- k count count2)))
                         (copy-bigger! <? pivot v2 0 v start end)
                         (%%vector-select <? v2 k2 0 n)))))))))))

(define (%%vector-select2 <? v k start end)
  (let ((size (- end start)))
    (cond ((= 2 size)
           (let ((a (vector-ref v start))
                 (b (vector-ref v (+ start 1))))
             (cond ((<? a b)
                    (values a b))
                   (else
                    (values b a)))))
          ((< size just-sort-it-threshold)
           (let ((v2 (vector-sort <? (r7rs-vector-copy v start end))))
             (values (vector-ref v2 k)
                     (vector-ref v2 (+ k 1)))))
          (else
           (let* ((ip (random-integer size))
                  (pivot (vector-ref v (+ start ip))))
             (call-with-values
              (lambda () (count-smaller <? pivot v start end 0 0))
              (lambda (count count2)
                (cond ((= (+ k 1) count)
                       (values (%%vector-select <? v k start end)
                               pivot))
                      ((< k count)
                       (let* ((n count)
                              (v2 (make-vector n)))
                         (copy-smaller! <? pivot v2 0 v start end)
                         (%%vector-select2 <? v2 k 0 n)))
                      ((< k (+ count count2))
                       (values pivot
                               (if (< (+ k 1) (+ count count2))
                                   pivot
                                   (%%vector-select <? v (+ k 1) start end))))
                      (else
                       (let* ((n (- size count count2))
                              (v2 (make-vector n))
                              (k2 (- k count count2)))
                         (copy-bigger! <? pivot v2 0 v start end)
                         (%%vector-select2 <? v2 k2 0 n)))))))))))

(define (count-smaller <? pivot v i end count count2)
  (cond ((= i end)
         (values count count2))
        ((<? (vector-ref v i) pivot)
         (count-smaller <? pivot v (+ i 1) end (+ count 1) count2))
        ((<? pivot (vector-ref v i))
         (count-smaller <? pivot v (+ i 1) end count count2))
        (else
         (count-smaller <? pivot v (+ i 1) end count (+ count2 1)))))

(define (copy-smaller! <? pivot dst at src start end)
  (cond ((= start end) dst)
        ((<? (vector-ref src start) pivot)
         (vector-set! dst at (vector-ref src start))
         (copy-smaller! <? pivot dst (+ at 1) src (+ start 1) end))
        (else
         (copy-smaller! <? pivot dst at src (+ start 1) end))))

(define (copy-bigger! <? pivot dst at src start end)
  (cond ((= start end) dst)
        ((<? pivot (vector-ref src start))
         (vector-set! dst at (vector-ref src start))
         (copy-bigger! <? pivot dst (+ at 1) src (+ start 1) end))
        (else
         (copy-bigger! <? pivot dst at src (+ start 1) end))))

))
