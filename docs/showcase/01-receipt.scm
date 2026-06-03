;; 01-receipt.scm — formatting as composable values  (SRFI 159, (scheme show))
;;
;; What's cool: there is no FORMAT mini-language and no printf string. A
;; "formatter" is a first-class value built from combinators, so `money`
;; and `table` are ordinary procedures you compose. The layout engine
;; auto-aligns columns from declared widths + an alignment symbol, and
;; `numeric/comma` does radix + precision + thousands-grouping declaratively.
;; Reusing `table` for the TOTAL row aligns it for free.

(import (scheme base) (scheme show) (srfi 1))

(define cart '(("artisanal coffee beans" 1899)
               ("conical burr grinder"   24900)
               ("gooseneck kettle"        8750)))

;; A first-class, composable formatter — cents -> "$1,234.56".
(define (money cents) (each "$" (numeric/comma (/ cents 100) 10 2)))

;; Two auto-aligned columns; the amount column is right-justified.
(define (table rows)
  (columnar 26 (each-in-list (map (lambda (r) (each (car r) nl)) rows))
            'right 9 (each-in-list (map (lambda (r) (each (money (cadr r)) nl)) rows))))

(define total (fold + 0 (map cadr cart)))

(show #t                                   ; #t = write to current output
  (table cart)
  (make-string 35 #\-) nl
  (table (list (list "TOTAL" total))))
