;; 08-sets.scm — real set values with set algebra  (SRFI 113)
;;
;; What's cool: a proper Set type parameterized by a comparator, with
;; union / intersection / difference as first-class operations. Clojure
;; has #{...} and clojure.set; Common Lisp makes you fake sets with lists.
;; Here the comparator (shared between both sets) decides membership, so
;; the same machinery works for numbers, strings, or your own records.

(import (scheme base) (scheme write) (scheme comparator) (scheme set))

(define cmp (make-default-comparator))     ; both sets MUST share a comparator
(define a (set cmp 1 2 3 4))
(define b (set cmp 3 4 5 6))

(display (list (set->list (set-intersection a b))   ; (4 3)
               (set->list (set-union a b))          ; (4 3 2 1 6 5)
               (set->list (set-difference a b))))   ; (2 1)
(newline)
