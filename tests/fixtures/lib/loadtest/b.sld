;; Fixture library for the filesystem-loader tests (bead nscheme-9q5).
;; Loaded indirectly: (loadtest a) imports this one, so resolving it
;; exercises recursive load + registry caching.
(define-library (loadtest b)
  (export b-value bump-b! read-b)
  (import (scheme base))
  (begin
    (define b-value 42)
    (define (bump-b!) (set! b-value (+ b-value 1)))
    (define (read-b) b-value)))
