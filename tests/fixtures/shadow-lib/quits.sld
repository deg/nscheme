;; Fixture for tests/browser_support.rs (bead nscheme-yd0.1): a library
;; whose body calls `exit`, to show the exit passes through the loader's
;; nested evaluation.
(define-library (quits)
  (import (scheme base) (scheme process-context))
  (begin (exit 6)))
