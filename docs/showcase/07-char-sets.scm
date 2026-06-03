;; 07-char-sets.scm — set algebra, but on characters  (SRFI 14)
;;
;; What's cool: character classes are real set values you combine with
;; union / intersection / difference, not regex fragments. Here "the
;; consonants" is literally (letters minus vowels), computed as a set
;; expression. The result is a reusable predicate-like object. CL and
;; Clojure make you reach for regex or hand-rolled membership tests.

(import (scheme base) (scheme write) (scheme charset) (srfi 1))

(define consonants
  (char-set-difference
    (char-set-union char-set:lower-case char-set:upper-case)  ; all letters
    (string->char-set "aeiouAEIOU")))                         ; minus vowels

(display
  (list->string
    (filter (lambda (c) (char-set-contains? consonants c))
            (string->list "Hello, World!"))))                 ; => "HllWrld"
(newline)
