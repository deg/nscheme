;; Fixture for the `load` special-form tests (tests/load.rs).
;; Loading this file must make these definitions live in the loading env.
(define loaded-greeting "hello from load")
(define (loaded-double x) (* x 2))
