;;; (scheme lseq) — R7RS-large (Red Edition) name for SRFI 127.
;;;
;;; The Red Edition adopts SRFI 127 (Lazy Sequences) under the name
;;; (scheme lseq); this library is a thin re-export of our vendored
;;; (srfi 127).
(define-library (scheme lseq)
  (import (srfi 127))
  (export generator->lseq lseq? lseq=?)
  (export lseq-car lseq-first lseq-cdr lseq-rest lseq-ref lseq-take lseq-drop)
  (export lseq-realize lseq->generator lseq-length lseq-append lseq-zip)
  (export lseq-map lseq-for-each lseq-filter lseq-remove)
  (export lseq-find lseq-find-tail lseq-take-while lseq-drop-while
          lseq-any lseq-every lseq-index lseq-member lseq-memq lseq-memv))
