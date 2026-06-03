;; 05-comparators.scm — ordering as a first-class object  (SRFI 128)
;;
;; What's cool: a comparator bundles type-test + equality + ordering +
;; hash into ONE value you can pass around — to sort, to a set, to a map,
;; to a hash table. Here we build a composite order ("shorter first, then
;; alphabetical") once and hand its ordering predicate to the sort. In CL
;; you'd pass a bare :key/:test; in Clojure a comparator fn. Here the
;; comparator is a reusable object that knows all four operations at once.

(import (scheme base) (scheme write) (scheme comparator) (scheme sort))

(define ranked
  (make-comparator
    string? string=?
    (lambda (a b)                          ; the ordering: length, then text
      (if (= (string-length a) (string-length b))
          (string<? a b)
          (< (string-length a) (string-length b))))
    #f))

(display (list-sort (comparator-ordering-predicate ranked)
                    '("pear" "fig" "kiwi" "ant" "apple" "ox")))
(newline)                                   ; => (ox ant fig kiwi pear apple)
