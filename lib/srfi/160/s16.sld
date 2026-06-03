;;; (srfi 160 s16) — Homogeneous s16 (signed 16-bit) numeric vector library.
;;;
;;; Derived from the SRFI 160 (Tangerine) reference implementation (John
;;; Cowan / Shiro Kawai, MIT). The generic SRFI 160 operations are the
;;; type-agnostic @-template ops; the base layer (constructor/accessor)
;;; is backed by a record-wrapped Scheme vector with the signed-16-bit
;;; element range, since nscheme has no native s16 array.
;;;
;;; SPDX-License-Identifier: MIT
;;; SPDX-FileCopyrightText: 2018 John Cowan <cowan@ccil.org>

(define-library (srfi 160 s16)
  (import (scheme base)
          (scheme case-lambda)
          (scheme cxr)
          (only (scheme r5rs) inexact->exact)
          (scheme complex)
          (scheme write)
          (srfi 128))
  ;; Constructors
  (export make-s16vector s16vector
          s16vector-unfold s16vector-unfold-right
          s16vector-copy s16vector-reverse-copy
          s16vector-append s16vector-concatenate
          s16vector-append-subvectors)
  ;; Predicates
  (export s16? s16vector? s16vector-empty? s16vector=)
  ;; Selectors
  (export s16vector-ref s16vector-length)
  ;; Iteration
  (export s16vector-take s16vector-take-right
          s16vector-drop s16vector-drop-right
          s16vector-segment
          s16vector-fold s16vector-fold-right
          s16vector-map s16vector-map! s16vector-for-each
          s16vector-count s16vector-cumulate)
  ;; Searching
  (export s16vector-take-while s16vector-take-while-right
          s16vector-drop-while s16vector-drop-while-right
          s16vector-index s16vector-index-right s16vector-skip s16vector-skip-right
          s16vector-any s16vector-every s16vector-partition
          s16vector-filter s16vector-remove)
  ;; Mutators
  (export s16vector-set! s16vector-swap! s16vector-fill! s16vector-reverse!
          s16vector-copy! s16vector-reverse-copy!
          s16vector-unfold! s16vector-unfold-right!)
  ;; Conversion
  (export s16vector->list list->s16vector
          reverse-s16vector->list reverse-list->s16vector
          s16vector->vector vector->s16vector)
  ;; Misc
  (export make-s16vector-generator s16vector-comparator write-s16vector)

  ;; --- (srfi 160 base) layer, s16 instantiation -----------------------
  ;; SRFI 4 s16 primitives == nscheme native bytevector primitives.
  (begin
    ;; nscheme has no native s16 array, so back it with a regular vector
    ;; wrapped in a record (so s16vector? is distinct from vector?).
    (define-record-type <s16vector-rec>
      (%make-s16vector backing) s16vector?
      (backing s16-backing))
    (define (make-s16vector len . fill)
      (%make-s16vector (make-vector len (if (pair? fill) (car fill) 0))))
    (define (s16vector . args) (%make-s16vector (list->vector args)))
    (define (s16vector-length v) (vector-length (s16-backing v)))
    (define (s16vector-ref v i) (vector-ref (s16-backing v) i))
    (define (s16vector-set! v i x) (vector-set! (s16-backing v) i x))

    ;; from base/valid.scm
    (define (s16? n) (and (exact-integer? n) (<= -32768 n 32767)))

    ;; from base/at-vector2list.scm (s16 expansion): the start/end-aware
    ;; s16vector->list that (srfi 160 base) exports.
    (define s16vector->list
      (case-lambda
        ((vec) (s16vector->list* vec 0 (s16vector-length vec)))
        ((vec start) (s16vector->list* vec start (s16vector-length vec)))
        ((vec start end) (s16vector->list* vec start end))))
    (define (s16vector->list* vec start end)
      (let loop ((i (- end 1)) (list '()))
        (if (< i start)
          list
          (loop (- i 1) (cons (s16vector-ref vec i) list)))))

    ;; from base (list->@vector); the @-impl uses only reverse-list->,
    ;; but the plain list->s16vector is a base export, defined here.
    (define (list->s16vector lst)
      (let* ((len (length lst))
             (v (make-s16vector len)))
        (let loop ((i 0) (lst lst))
          (if (= i len)
            v
            (begin
              (s16vector-set! v i (car lst))
              (loop (+ i 1) (cdr lst))))))))

  ;; --- @-impl.scm, expanded to s16 ------------------------------------
  (begin

(define (s16vector-unfold f len seed)
  (let ((v (make-s16vector len)))
    (let loop ((i 0) (state seed))
      (unless (= i len)
        (let-values (((value newstate) (f i state)))
          (s16vector-set! v i value)
          (loop (+ i 1) newstate))))
    v))

(define (s16vector-unfold-right f len seed)
  (let ((v (make-s16vector len)))
    (let loop ((i (- len 1)) (state seed))
      (unless (= i -1)
        (let-values (((value newstate) (f i state)))
          (s16vector-set! v i value)
          (loop (- i 1) newstate))))
    v))

(define s16vector-copy
  (case-lambda
    ((vec) (s16vector-copy* vec 0 (s16vector-length vec)))
    ((vec start) (s16vector-copy* vec start (s16vector-length vec)))
    ((vec start end) (s16vector-copy* vec start end))))

(define (s16vector-copy* vec start end)
  (let ((v (make-s16vector (- end start))))
    (s16vector-copy! v 0 vec start end)
    v))

(define s16vector-copy!
  (case-lambda
    ((to at from)
     (s16vector-copy!* to at from 0 (s16vector-length from)))
    ((to at from start)
     (s16vector-copy!* to at from start (s16vector-length from)))
    ((to at from start end) (s16vector-copy!* to at from start end))))

(define (s16vector-copy!* to at from start end)
  (let loop ((at at) (i start))
    (unless (= i end)
      (s16vector-set! to at (s16vector-ref from i))
      (loop (+ at 1) (+ i 1)))))

(define s16vector-reverse-copy
  (case-lambda
    ((vec) (s16vector-reverse-copy* vec 0 (s16vector-length vec)))
    ((vec start) (s16vector-reverse-copy* vec start (s16vector-length vec)))
    ((vec start end) (s16vector-reverse-copy* vec start end))))

(define (s16vector-reverse-copy* vec start end)
  (let ((v (make-s16vector (- end start))))
    (s16vector-reverse-copy! v 0 vec start end)
    v))

(define s16vector-reverse-copy!
  (case-lambda
    ((to at from)
     (s16vector-reverse-copy!* to at from 0 (s16vector-length from)))
    ((to at from start)
     (s16vector-reverse-copy!* to at from start (s16vector-length from)))
    ((to at from start end) (s16vector-reverse-copy!* to at from start end))))

(define (s16vector-reverse-copy!* to at from start end)
  (let loop ((at at) (i (- end 1)))
    (unless (< i start)
      (s16vector-set! to at (s16vector-ref from i))
      (loop (+ at 1) (- i 1)))))

(define (s16vector-append . vecs)
  (s16vector-concatenate vecs))

(define (s16vector-concatenate vecs)
  (let ((v (make-s16vector (len-sum vecs))))
    (let loop ((vecs vecs) (at 0))
      (unless (null? vecs)
        (let ((vec (car vecs)))
          (s16vector-copy! v at vec 0 (s16vector-length vec))
          (loop (cdr vecs) (+ at (s16vector-length vec)))))
    v)))

(define (len-sum vecs)
  (if (null? vecs)
    0
    (+ (s16vector-length (car vecs))
       (len-sum (cdr vecs)))))

(define (s16vector-append-subvectors . args)
  (let ((v (make-s16vector (len-subsum args))))
    (let loop ((args args) (at 0))
      (unless (null? args)
        (let ((vec (car args))
              (start (cadr args))
              (end (caddr args)))
          (s16vector-copy! v at vec start end)
          (loop (cdddr args) (+ at (- end start))))))
    v))

(define (len-subsum vecs)
  (if (null? vecs)
    0
    (+ (- (caddr vecs) (cadr vecs))
       (len-subsum (cdddr vecs)))))

(define (s16vector-empty? vec)
  (zero? (s16vector-length vec)))

(define (s16vector= . vecs)
  (s16vector=* (car vecs) (cadr vecs) (cddr vecs)))

(define (s16vector=* vec1 vec2 vecs)
  (and (s16dyadic-vecs= vec1 0 (s16vector-length vec1)
                      vec2 0 (s16vector-length vec2))
       (or (null? vecs)
           (s16vector=* vec2 (car vecs) (cdr vecs)))))

(define (s16dyadic-vecs= vec1 start1 end1 vec2 start2 end2)
  (cond
    ((not (= end1 end2)) #f)
    ((not (< start1 end1)) #t)
    ((let ((elt1 (s16vector-ref vec1 start1))
           (elt2 (s16vector-ref vec2 start2)))
      (= elt1 elt2))
     (s16dyadic-vecs= vec1 (+ start1 1) end1
                         vec2 (+ start2 1) end2))
    (else #f)))

(define (s16vector-take vec n)
  (let ((v (make-s16vector n)))
    (s16vector-copy! v 0 vec 0 n)
    v))

(define (s16vector-take-right vec n)
  (let ((v (make-s16vector n))
        (len (s16vector-length vec)))
    (s16vector-copy! v 0 vec (- len n) len)
    v))

(define (s16vector-drop vec n)
 (let* ((len (s16vector-length vec))
        (vlen (- len n))
        (v (make-s16vector vlen)))
    (s16vector-copy! v 0 vec n len)
    v))

(define (s16vector-drop-right vec n)
  (let* ((len (s16vector-length vec))
         (rlen (- len n))
         (v (make-s16vector rlen)))
    (s16vector-copy! v 0 vec 0 rlen)
    v))

(define (s16vector-segment vec n)
  (unless (and (integer? n) (positive? n))
    (error "length must be a positive integer" n))
  (let loop ((r '()) (i 0) (remain (s16vector-length vec)))
    (if (<= remain 0)
      (reverse r)
      (let ((size (min n remain)))
        (loop
          (cons (s16vector-copy vec i (+ i size)) r)
          (+ i size)
          (- remain size))))))

;; aux. procedure
(define (%s16vectors-ref vecs i)
  (map (lambda (v) (s16vector-ref v i)) vecs))

(define (s16vector-fold kons knil vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let ((len (s16vector-length vec)))
      (let loop ((r knil) (i 0))
        (if (= i len)
          r
          (loop (kons r (s16vector-ref vec i)) (+ i 1)))))
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs))))
      (let loop ((r knil) (i 0))
        (if (= i len)
          r
          (loop (apply kons r (%s16vectors-ref vecs i))
                (+ i 1)))))))

(define (s16vector-fold-right kons knil vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let ((len (s16vector-length vec)))
      (let loop ((r knil) (i (- (s16vector-length vec) 1)))
        (if (negative? i)
          r
          (loop (kons r (s16vector-ref vec i)) (- i 1)))))
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs))))
      (let loop ((r knil) (i (- len 1)))
        (if (negative? i)
          r
          (loop (apply kons r (%s16vectors-ref vecs i))
                (- i 1)))))))

(define (s16vector-map f vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let* ((len (s16vector-length vec))
           (v (make-s16vector len)))
      (let loop ((i 0))
        (unless (= i len)
          (s16vector-set! v i (f (s16vector-ref vec i)))
          (loop (+ i 1))))
      v)
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs)))
           (v (make-s16vector len)))
      (let loop ((i 0))
        (unless (= i len)
          (s16vector-set! v i (apply f (%s16vectors-ref vecs i)))
          (loop (+ i 1))))
      v)))


(define (s16vector-map! f vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let ((len (s16vector-length vec)))
      (let loop ((i 0))
        (unless (= i len)
          (s16vector-set! vec i (f (s16vector-ref vec i)))
          (loop (+ i 1)))))
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs))))
      (let loop ((i 0))
        (unless (= i len)
          (s16vector-set! vec i (apply f (%s16vectors-ref vecs i)))
          (loop (+ i 1)))))))

(define (s16vector-for-each f vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let ((len (s16vector-length vec)))
      (let loop ((i 0))
        (unless (= i len)
          (f (s16vector-ref vec i))
          (loop (+ i 1)))))
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs))))
      (let loop ((i 0))
        (unless (= i len)
          (apply f (%s16vectors-ref vecs i))
          (loop (+ i 1)))))))

(define (s16vector-count pred vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let ((len (s16vector-length vec)))
      (let loop ((i 0) (r 0))
        (cond
         ((= i (s16vector-length vec)) r)
         ((pred (s16vector-ref vec i)) (loop (+ i 1) (+ r 1)))
         (else (loop (+ i 1) r)))))
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs))))
      (let loop ((i 0) (r 0))
        (cond
         ((= i len) r)
         ((apply pred (%s16vectors-ref vecs i)) (loop (+ i 1) (+ r 1)))
         (else (loop (+ i 1) r)))))))

(define (s16vector-cumulate f knil vec)
  (let* ((len (s16vector-length vec))
         (v (make-s16vector len)))
    (let loop ((r knil) (i 0))
      (unless (= i len)
        (let ((next (f r (s16vector-ref vec i))))
          (s16vector-set! v i next)
          (loop next (+ i 1)))))
    v))

(define (s16vector-foreach f vec)
  (let ((len (s16vector-length vec)))
    (let loop ((i 0))
      (unless (= i len)
        (f (s16vector-ref vec i))
        (loop (+ i 1))))))

(define (s16vector-take-while pred vec)
  (let* ((len (s16vector-length vec))
         (idx (s16vector-skip pred vec))
         (idx* (if idx idx len)))
    (s16vector-copy vec 0 idx*)))

(define (s16vector-take-while-right pred vec)
  (let* ((len (s16vector-length vec))
         (idx (s16vector-skip-right pred vec))
         (idx* (if idx (+ idx 1) 0)))
    (s16vector-copy vec idx* len)))

(define (s16vector-drop-while pred vec)
  (let* ((len (s16vector-length vec))
         (idx (s16vector-skip pred vec))
         (idx* (if idx idx len)))
    (s16vector-copy vec idx* len)))

(define (s16vector-drop-while-right pred vec)
  (let* ((len (s16vector-length vec))
         (idx (s16vector-skip-right pred vec))
         (idx* (if idx idx -1)))
    (s16vector-copy vec 0 (+ 1 idx*))))

(define (s16vector-index pred vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let ((len (s16vector-length vec)))
      (let loop ((i 0))
        (cond
         ((= i len) #f)
         ((pred (s16vector-ref vec i)) i)
         (else (loop (+ i 1))))))
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs))))
      (let loop ((i 0))
        (cond
         ((= i len) #f)
         ((apply pred (%s16vectors-ref vecs i)) i)
         (else (loop (+ i 1))))))))

(define (s16vector-index-right pred vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let ((len (s16vector-length vec)))
      (let loop ((i (- len 1)))
        (cond
         ((negative? i) #f)
         ((pred (s16vector-ref vec i)) i)
         (else (loop (- i 1))))))
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs))))
      (let loop ((i (- len 1)))
        (cond
         ((negative? i) #f)
         ((apply pred (%s16vectors-ref vecs i)) i)
         (else (loop (- i 1))))))))

(define (s16vector-skip pred vec . vecs)
  (if (null? vecs)
    (s16vector-index (lambda (x) (not (pred x))) vec)
    (apply s16vector-index (lambda xs (not (apply pred xs))) vec vecs)))

(define (s16vector-skip-right pred vec . vecs)
  (if (null? vecs)
    (s16vector-index-right (lambda (x) (not (pred x))) vec)
    (apply s16vector-index-right (lambda xs (not (apply pred xs))) vec vecs)))

(define (s16vector-any pred vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let ((len (s16vector-length vec)))
      (let loop ((i 0))
        (cond
         ((= i len) #f)
         ((pred (s16vector-ref vec i)))  ;returns result of pred
         (else (loop (+ i 1))))))
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs))))
      (let loop ((i 0))
        (cond
         ((= i len) #f)
         ((apply pred (%s16vectors-ref vecs i))) ;returns result of pred
         (else (loop (+ i 1))))))))

(define (s16vector-every pred vec . vecs)
  (if (null? vecs)
    ;; fast path
    (let ((len (s16vector-length vec)))
      (let loop ((i 0) (last #t))
        (cond
         ((= i len) last)
         ((pred (s16vector-ref vec i)) => (lambda (r) (loop (+ i 1) r)))
         (else #f))))
    ;; generic case
    (let* ((vecs (cons vec vecs))
           (len (apply min (map s16vector-length vecs))))
      (let loop ((i 0) (last #t))
        (cond
         ((= i len) last)
         ((apply pred (%s16vectors-ref vecs i)) => (lambda (r) (loop (+ i 1) r)))
         (else #f))))))

(define (s16vector-partition pred vec)
  (let* ((len (s16vector-length vec))
         (cnt (s16vector-count pred vec))
         (r (make-s16vector len)))
    (let loop ((i 0) (yes 0) (no cnt))
      (cond
        ((= i len) (values r cnt))
        ((pred (s16vector-ref vec i))
         (s16vector-set! r yes (s16vector-ref vec i))
         (loop (+ i 1) (+ yes 1) no))
        (else
         (s16vector-set! r no (s16vector-ref vec i))
         (loop (+ i 1) yes (+ no 1)))))))

(define (s16vector-filter pred vec)
  (let* ((len (s16vector-length vec))
         (cnt (s16vector-count pred vec))
         (r (make-s16vector cnt)))
    (let loop ((i 0) (j 0))
      (cond
        ((= i len) r)
        ((pred (s16vector-ref vec i))
         (s16vector-set! r j (s16vector-ref vec i))
         (loop (+ i 1) (+ j 1)))
        (else
         (loop (+ i 1) j))))))

(define (s16vector-remove pred vec)
  (s16vector-filter (lambda (x) (not (pred x))) vec))

(define (s16vector-swap! vec i j)
  (let ((ival (s16vector-ref vec i))
        (jval (s16vector-ref vec j)))
    (s16vector-set! vec i jval)
    (s16vector-set! vec j ival)))

(define s16vector-fill!
  (case-lambda
    ((vec fill) (s16vector-fill-some! vec fill 0 (s16vector-length vec)))
    ((vec fill start) (s16vector-fill-some! vec fill start (s16vector-length vec)))
    ((vec fill start end) (s16vector-fill-some! vec fill start end))))

(define (s16vector-fill-some! vec fill start end)
  (unless (= start end)
    (s16vector-set! vec start fill)
    (s16vector-fill-some! vec fill (+ start 1) end)))

(define s16vector-reverse!
  (case-lambda
    ((vec) (s16vector-reverse-some! vec 0 (s16vector-length vec)))
    ((vec start) (s16vector-reverse-some! vec start (s16vector-length vec)))
    ((vec start end) (s16vector-reverse-some! vec start end))))

(define (s16vector-reverse-some! vec start end)
  (let loop ((i start) (j (- end 1)))
    (when (< i j)
      (s16vector-swap! vec i j)
      (loop (+ i 1) (- j 1)))))

(define (s16vector-unfold! f vec start end seed)
  (let loop ((i start) (seed seed))
    (when (< i end)
      (let-values (((elt seed) (f i seed)))
        (s16vector-set! vec i elt)
        (loop (+ i 1) seed)))))

(define (s16vector-unfold-right! f vec start end seed)
  (let loop ((i (- end 1)) (seed seed))
    (when (>= i start)
      (let-values (((elt seed) (f i seed)))
        (s16vector-set! vec i elt)
        (loop (- i 1) seed)))))

(define reverse-s16vector->list
  (case-lambda
    ((vec) (reverse-s16vector->list* vec 0 (s16vector-length vec)))
    ((vec start) (reverse-s16vector->list* vec start (s16vector-length vec)))
    ((vec start end) (reverse-s16vector->list* vec start end))))

(define (reverse-s16vector->list* vec start end)
  (let loop ((i start) (r '()))
    (if (= i end)
      r
      (loop (+ 1 i) (cons (s16vector-ref vec i) r)))))

(define (reverse-list->s16vector list)
  (let* ((len (length list))
         (r (make-s16vector len)))
    (let loop ((i 0) (list list))
      (cond
        ((= i len) r)
        (else
          (s16vector-set! r (- len i 1) (car list))
          (loop (+ i 1) (cdr list)))))))

(define s16vector->vector
  (case-lambda
    ((vec) (s16vector->vector* vec 0 (s16vector-length vec)))
    ((vec start) (s16vector->vector* vec start (s16vector-length vec)))
    ((vec start end) (s16vector->vector* vec start end))))

(define (s16vector->vector* vec start end)
  (let* ((len (- end start))
         (r (make-vector len)))
    (let loop ((i start) (o 0))
      (cond
        ((= i end) r)
        (else
          (vector-set! r o (s16vector-ref vec i))
          (loop (+ i 1) (+ o 1)))))))

(define vector->s16vector
  (case-lambda
    ((vec) (vector->s16vector* vec 0 (vector-length vec)))
    ((vec start) (vector->s16vector* vec start (vector-length vec)))
    ((vec start end) (vector->s16vector* vec start end))))

(define (vector->s16vector* vec start end)
  (let* ((len (- end start))
         (r (make-s16vector len)))
    (let loop ((i start) (o 0))
      (cond
        ((= i end) r)
        (else
          (s16vector-set! r o (vector-ref vec i))
          (loop (+ i 1) (+ o 1)))))))

(define make-s16vector-generator
  (case-lambda ((vec) (make-s16vector-generator vec 0 (s16vector-length vec)))
               ((vec start) (make-s16vector-generator vec start (s16vector-length vec)))
               ((vec start end)
                (lambda () (if (>= start end)
                             (eof-object)
                             (let ((next (s16vector-ref vec start)))
                              (set! start (+ start 1))
                              next))))))

(define write-s16vector
  (case-lambda
    ((vec) (write-s16vector* vec (current-output-port)))
    ((vec port) (write-s16vector* vec port))))


(define (write-s16vector* vec port)
  (display "#s16(" port)  ; s16-expansion is blind, so will expand this too
  (let ((last (- (s16vector-length vec) 1)))
    (let loop ((i 0))
      (cond
        ((= i last)
         (write (s16vector-ref vec i) port)
         (display ")" port))
        (else
          (write (s16vector-ref vec i) port)
          (display " " port)
          (loop (+ i 1)))))))

(define (s16vector< vec1 vec2)
  (let ((len1 (s16vector-length vec1))
        (len2 (s16vector-length vec2)))
    (cond
      ((< len1 len2)
       #t)
      ((> len1 len2)
       #f)
      (else
       (let loop ((i 0))
         (cond
           ((= i len1)
            #f)
           ((< (s16vector-ref vec1 i) (s16vector-ref vec2 i))
            #t)
           ((> (s16vector-ref vec1 i) (s16vector-ref vec2 i))
            #f)
           (else
             (loop (+ i 1)))))))))

(define (s16vector-hash vec)
  (let ((len (min 256 (s16vector-length vec))))
    (let loop ((i 0) (r 0))
      (if (= i len)
        (abs (floor (real-part (inexact->exact r))))
        (loop (+ i 1) (+ r (s16vector-ref vec i)))))))

(define s16vector-comparator
  (make-comparator s16vector? s16vector= s16vector< s16vector-hash))

))
