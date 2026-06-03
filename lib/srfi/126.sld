;;; (srfi 126) — R6RS-style hashtables (subset).
;;;
;;; nscheme has no native hash table, so this is an association-list
;;; backed implementation (O(n) per op) sufficient for the small tables
;;; its consumers use. It is the substrate SRFI 125 is built on, and is
;;; imported directly by the SRFI 125 reference test suite (for
;;; hashtable-copy). The stored hash function is kept only so
;;; hashtable-hash-function can return it; lookups use the equivalence
;;; predicate directly.
(define-library (srfi 126)
  (export make-hashtable make-eq-hashtable make-eqv-hashtable
          hashtable? hashtable-mutable? hashtable-size
          hashtable-contains? hashtable-ref hashtable-set!
          hashtable-delete! hashtable-clear! hashtable-keys
          hashtable-entries hashtable-values hashtable-copy
          hashtable-update! hashtable-equivalence-function
          hashtable-hash-function equal-hash)
  (import (scheme base))
  (begin
    ;; A trivial fallback hash (the alist backing never uses it for
    ;; storage; it only flows back out through hashtable-hash-function).
    (define (equal-hash x) 0)

    (define-record-type srfi126-hashtable
      (%make-ht equiv hashfn cells mutable?)
      srfi126-hashtable?
      (equiv    ht-equiv)
      (hashfn   ht-hashfn)
      (cells    ht-cells)       ; one-element list whose car is the alist
      (mutable? ht-mutable?))

    (define (%ht-alist ht) (car (ht-cells ht)))
    (define (%ht-set-alist! ht a) (set-car! (ht-cells ht) a))

    (define (%ht-assoc ht key)
      (let ((eq (ht-equiv ht)))
        (let loop ((a (%ht-alist ht)))
          (cond ((null? a) #f)
                ((eq (caar a) key) (car a))
                (else (loop (cdr a)))))))

    (define (make-hashtable hashfn equiv . rest) (%make-ht equiv hashfn (list '()) #t))
    (define (make-eq-hashtable . rest) (%make-ht eq? #f (list '()) #t))
    (define (make-eqv-hashtable . rest) (%make-ht eqv? #f (list '()) #t))

    (define (hashtable? obj) (srfi126-hashtable? obj))
    (define (hashtable-mutable? ht) (ht-mutable? ht))

    (define (%check-mutable ht who)
      (if (not (ht-mutable? ht))
          (error (string-append who ": hashtable is immutable") ht)))

    (define (hashtable-size ht) (length (%ht-alist ht)))
    (define (hashtable-contains? ht key) (and (%ht-assoc ht key) #t))

    (define (hashtable-ref ht key default)
      (let ((cell (%ht-assoc ht key)))
        (if cell (cdr cell) default)))

    (define (hashtable-set! ht key val)
      (%check-mutable ht "hashtable-set!")
      (let ((cell (%ht-assoc ht key)))
        (if cell
            (set-cdr! cell val)
            (%ht-set-alist! ht (cons (cons key val) (%ht-alist ht)))))
      (if #f #f))

    (define (hashtable-update! ht key proc default)
      (%check-mutable ht "hashtable-update!")
      (let ((cell (%ht-assoc ht key)))
        (if cell
            (set-cdr! cell (proc (cdr cell)))
            (%ht-set-alist! ht (cons (cons key (proc default)) (%ht-alist ht)))))
      (if #f #f))

    (define (hashtable-delete! ht key)
      (%check-mutable ht "hashtable-delete!")
      (let ((eq (ht-equiv ht)))
        (%ht-set-alist!
         ht
         (let loop ((a (%ht-alist ht)))
           (cond ((null? a) '())
                 ((eq (caar a) key) (cdr a))
                 (else (cons (car a) (loop (cdr a))))))))
      (if #f #f))

    (define (hashtable-clear! ht . rest)
      (%check-mutable ht "hashtable-clear!")
      (%ht-set-alist! ht '())
      (if #f #f))

    (define (hashtable-keys ht) (list->vector (map car (%ht-alist ht))))
    (define (hashtable-values ht) (list->vector (map cdr (%ht-alist ht))))

    (define (hashtable-entries ht)
      (let ((a (%ht-alist ht)))
        (values (list->vector (map car a)) (list->vector (map cdr a)))))

    ;; R6RS: (hashtable-copy ht [mutable?]); mutable? defaults to #f.
    (define (hashtable-copy ht . rest)
      (let ((mutable? (and (not (null? rest)) (car rest))))
        (%make-ht (ht-equiv ht)
                  (ht-hashfn ht)
                  (list (map (lambda (p) (cons (car p) (cdr p))) (%ht-alist ht)))
                  (and mutable? #t))))

    (define (hashtable-equivalence-function ht) (ht-equiv ht))
    (define (hashtable-hash-function ht) (ht-hashfn ht))))
