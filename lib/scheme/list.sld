;;; (scheme list) — R7RS-large (Red Edition) name for SRFI 1.
;;;
;;; The Red Edition adopts SRFI 1 (the list library) under the name
;;; (scheme list); this library is a thin re-export of our vendored
;;; (srfi 1).
(define-library (scheme list)
  (import (srfi 1))
  (export
   ;; Constructors
   xcons make-list list-tabulate cons* list-copy iota
   ;; Predicates
   proper-list? circular-list? dotted-list? not-pair? null-list? list=
   circular-list length+
   ;; Selectors
   first second third fourth fifth sixth seventh eighth ninth tenth
   car+cdr take drop take-right drop-right take! drop-right! split-at split-at!
   last last-pair
   ;; Miscellaneous: length, reverse, append & co.
   length append concatenate reverse append! concatenate! reverse!
   append-reverse append-reverse! zip unzip1 unzip2 unzip3 unzip4 unzip5 count
   ;; Fold, unfold & map
   map for-each fold unfold pair-fold reduce fold-right unfold-right
   pair-fold-right reduce-right append-map append-map! map! pair-for-each
   filter-map map-in-order
   ;; Filtering & partitioning
   filter partition remove filter! partition! remove!
   ;; Searching
   member find find-tail any every list-index take-while drop-while
   take-while! span break span! break!
   ;; Deletion
   delete delete-duplicates delete! delete-duplicates!
   ;; Association lists
   assoc alist-cons alist-copy alist-delete alist-delete!
   ;; Set operations on lists
   lset<= lset= lset-adjoin lset-union lset-union! lset-intersection
   lset-intersection! lset-difference lset-difference! lset-xor lset-xor!
   lset-diff+intersection lset-diff+intersection!
   ;; Primitive side-effects
   cons car cdr set-car! set-cdr!))
