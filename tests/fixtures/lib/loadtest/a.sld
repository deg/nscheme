;; Fixture library for the filesystem-loader tests (bead nscheme-9q5).
;; Imports a base procedure (+) and a second on-disk library (loadtest b),
;; so loading it drives recursion through import_one and the q1c shared
;; cells (a-plus-b reads loadtest b's mutable b-value).
(define-library (loadtest a)
  (export a-plus-b)
  (import (scheme base) (loadtest b))
  (begin
    (define (a-plus-b x) (+ x b-value))))
