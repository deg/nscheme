;; Fixture for tests/browser_support.rs (bead nscheme-yd0.1): a stand-in
;; (scheme list) whose `iota` returns a marker, to show that a library
;; on the disk search path wins over the copy compiled into the binary.
(define-library (scheme list)
  (export iota)
  (import (scheme base))
  (begin
    (define (iota . _) 'from-disk)))
