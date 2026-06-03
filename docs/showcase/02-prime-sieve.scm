;; 02-prime-sieve.scm — the *infinite* Sieve of Eratosthenes  (SRFI 41, streams)
;;
;; What's cool: `primes` is a genuinely endless sequence — no upper bound,
;; no array, no `(sieve-up-to n)`. Each prime lazily filters the rest of
;; the stream. Clojure has lazy seqs, but the recursion here (the sieve
;; calls itself on its own filtered tail) reads like the textbook math.
;; Common Lisp has no built-in equivalent. You ask for as many as you want.

(import (scheme base) (scheme write) (scheme stream))

(define (sift p s)                         ; drop everything divisible by p
  (stream-filter (lambda (n) (not (zero? (modulo n p)))) s))

(define (sieve s)
  (stream-cons (stream-car s)
               (sieve (sift (stream-car s) (stream-cdr s)))))

(define primes (sieve (stream-from 2)))    ; the integers 2,3,4,... sieved

(display (stream->list (stream-take 12 primes))) (newline)   ; first 12
(display (stream-ref primes 99))           ; the 100th prime, on demand
(newline)
