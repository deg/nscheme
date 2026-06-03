;; 06-regex-sre.scm — regular expressions as S-expressions  (SRFI 115)
;;
;; What's cool: the regex is *data*, not a cryptic string. `(rx ($ (+ numeric))
;; "-" ...)` is a structured pattern you can build, nest, and compose with
;; ordinary list operations — no backslash soup, no escaping. Submatches are
;; `($ ...)`. Neither CL's nor Clojure's standard regex is structured like
;; this; this is the "code is data" idea applied to pattern matching.

(import (scheme base) (scheme write) (scheme regex))

;; "digits - digits - digits", each group captured.
(define date (rx ($ (+ numeric)) "-" ($ (+ numeric)) "-" ($ (+ numeric))))

(define m (regexp-search date "the launch is on 2026-06-03, mark it"))

(display (list (regexp-match-submatch m 1)   ; "2026"
               (regexp-match-submatch m 2)   ; "06"
               (regexp-match-submatch m 3))) ; "03"
(newline)
