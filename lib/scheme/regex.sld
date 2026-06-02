;;; (scheme regex) — R7RS-large (Red Edition) name for SRFI 115.
;;;
;;; The Red Edition adopts SRFI 115 under the name (scheme regex);
;;; this library is a thin re-export of our vendored (srfi 115).
;;;
;;; NOTE: (srfi 115) is currently BLOCKED in nscheme — it depends on
;;; SRFI 14 (char-sets) and a bitwise SRFI (60/33/151), neither of
;;; which nscheme provides. This re-export will not load until those
;;; are vendored. See lib/srfi/115.sld for details.
(define-library (scheme regex)
  (import (srfi 115))
  (export regexp regexp? valid-sre? rx regexp->sre char-set->sre
          regexp-matches regexp-matches? regexp-search
          regexp-replace regexp-replace-all regexp-match->list
          regexp-fold regexp-extract regexp-split regexp-partition
          regexp-match? regexp-match-count
          regexp-match-submatch
          regexp-match-submatch-start regexp-match-submatch-end))
