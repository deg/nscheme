;; 03-generators.scm — lazy pipelines + their mirror image  (SRFI 158)
;;
;; What's cool: a generator is a pull-based lazy producer (like a Python
;; generator), and you compose them with map/filter/take that are *also*
;; lazy — nothing runs until `generator->list` pulls. The surprise for a
;; Clojure/CL programmer is the dual: an *accumulator* is the exact mirror
;; image — a procedure you push values into, then read the result out.
;; Producers and consumers as first-class, composable values.

(import (scheme base) (scheme write) (scheme generator))

;; Square the naturals, keep evens, take 6 — all lazy, nothing eager.
(define squares-of-evens
  (gtake (gfilter even? (gmap (lambda (x) (* x x))
                              (make-iota-generator 100 1)))
         6))
(display (generator->list squares-of-evens)) (newline)

;; An accumulator is a generator run backwards: push in, then read out.
(define acc (list-accumulator))
(for-each acc '(3 1 2))
(display (acc (eof-object)))               ; eof signals "give me the result"
(newline)
