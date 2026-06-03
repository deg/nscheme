;; 04-persistent-map.scm — a persistent *ordered* map  (SRFI 146)
;;
;; What's cool: like Clojure's maps, updates are non-destructive — `m1`
;; is a new map and `m0` still has the old value. Unlike a Clojure hash
;; map, this one is *ordered* by whatever comparator you give it, so the
;; keys come back sorted for free (it's a balanced tree, not a hash). One
;; comparator object decides identity AND order. Common Lisp has nothing
;; like this in the standard.

(import (scheme base) (scheme write) (scheme comparator) (scheme mapping))

(define m0 (mapping (make-default-comparator)
                    "banana" 3 "apple" 5 "cherry" 7))

(define m1 (mapping-set m0 "apple" 99))    ; returns a NEW map; m0 untouched

(display (list (mapping-ref m0 "apple")    ; => 5  (the original survives)
               (mapping-ref m1 "apple")))  ; => 99 (the update)
(newline)

(display (mapping-keys m0))                 ; => (apple banana cherry) — sorted
(newline)
