;;; (scheme mapping hash) — R7RS-large (Tangerine Edition) name for the
;;; hash sub-library of SRFI 146.
;;;
;;; The Tangerine Edition adopts SRFI 146's hashmaps under the name
;;; (scheme mapping hash); this library is a thin re-export of our
;;; vendored (srfi 146 hash).
(define-library (scheme mapping hash)
  (import (srfi 146 hash))
  (export hashmap hashmap-unfold
          hashmap? hashmap-contains? hashmap-empty? hashmap-disjoint?
          hashmap-ref hashmap-ref/default hashmap-key-comparator
          hashmap-adjoin hashmap-adjoin!
          hashmap-set hashmap-set!
          hashmap-replace hashmap-replace!
          hashmap-delete hashmap-delete! hashmap-delete-all hashmap-delete-all!
          hashmap-intern hashmap-intern!
          hashmap-update hashmap-update! hashmap-update/default hashmap-update!/default
          hashmap-pop hashmap-pop!
          hashmap-search hashmap-search!
          hashmap-size hashmap-find hashmap-count hashmap-any? hashmap-every?
          hashmap-keys hashmap-values hashmap-entries
          hashmap-map hashmap-map->list hashmap-for-each hashmap-fold
          hashmap-filter hashmap-filter!
          hashmap-remove hashmap-remove!
          hashmap-partition hashmap-partition!
          hashmap-copy hashmap->alist alist->hashmap alist->hashmap!
          hashmap=? hashmap<? hashmap>? hashmap<=? hashmap>=?
          hashmap-union hashmap-intersection hashmap-difference hashmap-xor
          hashmap-union! hashmap-intersection! hashmap-difference! hashmap-xor!
          make-hashmap-comparator
          hashmap-comparator
          comparator?))
