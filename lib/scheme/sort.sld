;;; (scheme sort) — R7RS-large (Red Edition) name for SRFI 132.
;;;
;;; The Red Edition adopts SRFI 132 (Sort Libraries) under the name
;;; (scheme sort); this library is a thin re-export of our vendored
;;; (srfi 132).
(define-library (scheme sort)
  (import (srfi 132))
  (export list-sorted?               vector-sorted?
          list-sort                  vector-sort
          list-stable-sort           vector-stable-sort
          list-sort!                 vector-sort!
          list-stable-sort!          vector-stable-sort!
          list-merge                 vector-merge
          list-merge!                vector-merge!
          list-delete-neighbor-dups  vector-delete-neighbor-dups
          list-delete-neighbor-dups! vector-delete-neighbor-dups!
          vector-find-median         vector-find-median!
          vector-select!             vector-separate!))
