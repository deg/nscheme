;;; (scheme show) — R7RS-large (Tangerine Edition) name for SRFI 159.
;;;
;;; The Tangerine Edition adopts SRFI 159 (Combinator Formatting) under
;;; the name (scheme show); this library is a thin re-export of our
;;; vendored (srfi 159).
;;;
;;; Our (srfi 159) vendors the base + color combinators plus the
;;; columnar sub-library (columnar/tabular/wrapped/show-columns/…). The
;;; unicode sub-library (terminal-width-aware variants) is not vendored.
(define-library (scheme show)
  (import (srfi 159))
  (export
   ;; base / util
   call-with-output displayed each each-in-list escaped
   fitted fitted/both fitted/right fl fn forked
   joined joined/dot joined/last joined/prefix joined/range joined/suffix
   maybe-escaped nl nothing
   numeric numeric/comma numeric/fitted numeric/si
   padded padded/both padded/right pretty pretty-simply
   show space-to tab-to
   trimmed trimmed/both trimmed/lazy trimmed/right
   with with! written written-simply
   ;; color
   as-red as-blue as-green as-cyan as-yellow as-magenta as-white
   as-black as-bold as-underline
   ;; columnar
   call-with-output-generator call-with-output-generators columnar
   from-file justified line-numbers show-columns string->line-generator
   tabular wrapped wrapped/char wrapped/list))
