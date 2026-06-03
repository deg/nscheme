;; 10-ideque.scm — an immutable double-ended queue  (SRFI 134)
;;
;; What's cool: a persistent deque with O(1) access AND removal at *both*
;; ends — and every operation returns a new deque, sharing structure with
;; the old. That makes the palindrome check below read declaratively: peek
;; both ends, then recurse on the deque with both ends removed. Clojure's
;; persistent vectors are fast at one end; CL has no persistent deque at all.

(import (scheme base) (scheme write) (scheme ideque))

(define (palindrome? s)
  (let loop ((dq (list->ideque (string->list s))))
    (cond ((or (ideque-empty? dq) (= 1 (ideque-length dq))) #t)
          ((char=? (ideque-front dq) (ideque-back dq))
           (loop (ideque-remove-front (ideque-remove-back dq)))) ; new deque
          (else #f))))

(display (map palindrome? '("racecar" "hello" "noon" "scheme")))
(newline)                                   ; => (#t #f #t #f)
