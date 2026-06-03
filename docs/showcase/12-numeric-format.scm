;; 12-numeric-format.scm — declarative number formatting  (SRFI 159)
;;
;; What's cool: every way you might want to render a number is a combinator,
;; not a format-string code you have to memorize. Thousands separators,
;; fixed precision, SI suffixes (k/M/G), arbitrary radix, and field padding
;; — each is a named, composable value. Compare CL's `~,2F`/`~:D` directives
;; or hand-rolled string mangling.

(import (scheme base) (scheme show))

(show #t
  "comma:  " (numeric/comma 1234567)       nl   ; 1,234,567
  "money:  $" (numeric/comma 9999.5 10 2)  nl   ; $9,999.50
  "SI:     " (numeric/si 1500000 1000) "B"  nl  ; 1.5MB
  "hex:    " (numeric 255 16)              nl   ; ff
  "binary: " (numeric 10 2)                nl   ; 1010
  "padded: |" (padded 8 (numeric 42)) "|"  nl)  ; |      42|
