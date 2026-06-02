;;; (srfi 113) — Sets and bags.
;;;
;;; Vendored from the SRFI 113 reference implementation by John Cowan
;;; (2013), MIT licence (text preserved below). Flattened into a single
;;; file: the upstream .sld uses (include "sets-impl.scm"), but nscheme
;;; resolves `include` relative to the process directory, so the body is
;;; inlined here inside `begin` instead.
;;;
;;; Upstream is built on a SRFI 69 hash table, obtained through the
;;; SRFI 114→128 transition shim `make-hash-table/comparator` (which uses
;;; ONLY the comparator's equality predicate, never its hash function).
;;; nscheme ships no hash-table SRFI (no 69/125/126), so this file inlines
;;; a tiny self-contained association-list-backed hash table implementing
;;; exactly the SRFI-69 subset the set/bag code needs. Keying on the
;;; comparator's equality predicate makes results correct (only the
;;; asymptotics differ — lookups are O(n) instead of O(1)). The shim is the
;;; ONLY substantive deviation from upstream; the set/bag logic is verbatim.
;;;
;;; SPDX-License-Identifier: MIT
;;; Copyright (C) John Cowan (2013). All Rights Reserved.
;;;
;;; Permission is hereby granted, free of charge, to any person obtaining
;;; a copy of this software and associated documentation files (the
;;; "Software"), to deal in the Software without restriction, including
;;; without limitation the rights to use, copy, modify, merge, publish,
;;; distribute, sublicense, and/or sell copies of the Software, and to
;;; permit persons to whom the Software is furnished to do so, subject to
;;; the following conditions:
;;;
;;; The above copyright notice and this permission notice shall be
;;; included in all copies or substantial portions of the Software.
;;;
;;; THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
;;; EXPRESS OR IMPLIED. See the SRFI 113 document for full text.

(define-library (srfi 113)
  (export set set-unfold)
  (export set? set-contains? set-empty? set-disjoint?)
  (export set-member set-element-comparator)
  (export set-adjoin set-adjoin! set-replace set-replace!
          set-delete set-delete! set-delete-all set-delete-all! set-search!)
  (export set-size set-find set-count set-any? set-every?)
  ;; NOTE: upstream lists `set-remove` twice here (a typo); deduped so the
  ;; export list is a valid set of distinct names.
  (export set-map set-for-each set-fold
          set-filter set-remove set-partition
          set-filter! set-remove! set-partition!)
  (export set-copy set->list list->set list->set!)
  (export set=? set<? set>? set<=? set>=?)
  (export set-union set-intersection set-difference set-xor
          set-union! set-intersection! set-difference! set-xor!)
  (export set-comparator)

  (export bag bag-unfold)
  (export bag? bag-contains? bag-empty? bag-disjoint?)
  (export bag-member bag-element-comparator)
  (export bag-adjoin bag-adjoin! bag-replace bag-replace!
          bag-delete bag-delete! bag-delete-all bag-delete-all! bag-search!)
  (export bag-size bag-find bag-count bag-any? bag-every?)
  (export bag-map bag-for-each bag-fold
          bag-filter bag-remove bag-partition
          bag-filter! bag-remove! bag-partition!)
  (export bag-copy bag->list list->bag list->bag!)
  (export bag=? bag<? bag>? bag<=? bag>=?)
  (export bag-union bag-intersection bag-difference bag-xor
          bag-union! bag-intersection! bag-difference! bag-xor!)
  (export bag-comparator)

  (export bag-sum bag-sum! bag-product bag-product!
          bag-unique-size bag-element-count bag-for-each-unique bag-fold-unique
          bag-increment! bag-decrement! bag->set set->bag set->bag!
          bag->alist alist->bag)

  (import (scheme base)
          (scheme case-lambda)
          (scheme write)
          (scheme comparator))

  ;;; --- Inlined SRFI-69-subset hash table -----------------------------
  ;;; nscheme has no hash-table SRFI on disk, so we provide just the
  ;;; operations the set/bag code uses. A table is a record holding an
  ;;; equality predicate and a mutable association list of (key . value)
  ;;; pairs. Only `make-hash-table`'s first argument (the equality
  ;;; predicate) is significant; the second (a hash function) is accepted
  ;;; and ignored, matching how the upstream shim calls it.
  (begin

    (define-record-type ht
      (raw-make-ht equality alist)
      ht?
      (equality ht-equality)
      (alist ht-alist set-ht-alist!))

    (define (make-hash-table equality . ignored)
      (raw-make-ht equality '()))

    (define (ht-find table key)
      ;; Return the (key . value) pair for KEY, or #f.
      (let ((eq (ht-equality table)))
        (let loop ((p (ht-alist table)))
          (cond
            ((null? p) #f)
            ((eq (caar p) key) (car p))
            (else (loop (cdr p)))))))

    (define (hash-table-exists? table key)
      (and (ht-find table key) #t))

    (define (hash-table-ref/default table key default)
      (let ((pair (ht-find table key)))
        (if pair (cdr pair) default)))

    (define (hash-table-set! table key value)
      (let ((pair (ht-find table key)))
        (if pair
            (set-cdr! pair value)
            (set-ht-alist! table (cons (cons key value) (ht-alist table))))))

    (define (hash-table-update!/default table key proc default)
      (let ((pair (ht-find table key)))
        (if pair
            (set-cdr! pair (proc (cdr pair)))
            (set-ht-alist! table
              (cons (cons key (proc default)) (ht-alist table))))))

    (define (hash-table-delete! table key)
      (let ((eq (ht-equality table)))
        (set-ht-alist! table
          (let loop ((p (ht-alist table)))
            (cond
              ((null? p) '())
              ((eq (caar p) key) (cdr p))
              (else (cons (car p) (loop (cdr p)))))))))

    (define (hash-table-size table)
      (length (ht-alist table)))

    (define (hash-table-walk table proc)
      (for-each (lambda (pair) (proc (car pair) (cdr pair)))
                (ht-alist table)))

    (define (hash-table-copy table)
      ;; Independent copy: fresh pairs so mutation of the copy never
      ;; affects the original (sob-copy relies on this).
      (raw-make-ht
        (ht-equality table)
        (map (lambda (pair) (cons (car pair) (cdr pair)))
             (ht-alist table)))))

  ;;; --- Set/bag implementation (verbatim from sets-impl.scm) ----------
  (begin

;;; A "sob" object is the representation of both sets and bags.

(define (make-hash-table/comparator comparator)
  (make-hash-table (comparator-equality-predicate comparator)
                   (modulizer (comparator-hash-function comparator))))

(define (modulizer hash-function)
  (case-lambda
    ((obj) (hash-function obj))
    ((obj limit) (modulo (hash-function obj) limit))))

(define hash-table-contains? hash-table-exists?)

(define (hash-table-for-each proc hash-table)
  (hash-table-walk hash-table proc))


;;; Record definition and core typing/checking procedures

(define-record-type sob
  (raw-make-sob hash-table comparator multi?)
  sob?
  (hash-table sob-hash-table)
  (comparator sob-comparator)
  (multi? sob-multi?))

(define (set? obj) (and (sob? obj) (not (sob-multi? obj))))

(define (bag? obj) (and (sob? obj) (sob-multi? obj)))

(define (check-set obj) (if (not (set? obj)) (error "not a set" obj)))

(define (check-bag obj) (if (not (bag? obj)) (error "not a bag" obj)))

(define (check-all-sets list)
  (for-each (lambda (obj) (check-set obj)) list)
  (sob-check-comparators list))

(define (check-all-bags list)
  (for-each (lambda (obj) (check-bag obj)) list)
  (sob-check-comparators list))

(define (sob-check-comparators list)
  (if (not (null? list))
      (for-each
        (lambda (sob)
          (check-same-comparator (car list) sob))
        (cdr list))))

(define (check-same-comparator a b)
  (if (not (eq? (sob-comparator a) (sob-comparator b)))
    (error "different comparators" a b)))

(define (check-element sob element)
  (comparator-check-type (sob-comparator sob) element))

;;; Constructors

(define (make-sob comparator multi?)
  (raw-make-sob (make-hash-table/comparator comparator) comparator multi?))

(define (sob-copy sob)
  (raw-make-sob (hash-table-copy (sob-hash-table sob))
            (sob-comparator sob)
            (sob-multi? sob)))

(define (set-copy set)
  (check-set set)
  (sob-copy set))

(define (bag-copy bag)
  (check-bag bag)
  (sob-copy bag))

(define (sob-empty-copy sob)
  (make-sob (sob-comparator sob) (sob-multi? sob)))

(define (set comparator . elements)
  (let ((result (make-sob comparator #f)))
    (for-each (lambda (x) (sob-increment! result x 1)) elements)
    result))

(define (bag comparator . elements)
  (let ((result (make-sob comparator #t)))
    (for-each (lambda (x) (sob-increment! result x 1)) elements)
    result))

(define (sob-unfold stop? mapper successor seed comparator multi?)
  (let ((result (make-sob comparator multi?)))
    (let loop ((seed seed))
      (if (stop? seed)
          result
          (begin
            (sob-increment! result (mapper seed) 1)
            (loop (successor seed)))))))

(define (set-unfold continue? mapper successor seed comparator)
  (sob-unfold continue? mapper successor seed comparator #f))

(define (bag-unfold continue? mapper successor seed comparator)
  (sob-unfold continue? mapper successor seed comparator #t))

;;; Predicates

(define (sob-contains? sob member)
  (hash-table-contains? (sob-hash-table sob) member))

(define (set-contains? set member)
  (check-set set)
  (sob-contains? set member))

(define (bag-contains? bag member)
  (check-bag bag)
  (sob-contains? bag member))

(define (sob-empty? sob)
  (= 0 (hash-table-size (sob-hash-table sob))))

(define (set-empty? set)
  (check-set set)
  (sob-empty? set))

(define (bag-empty? bag)
  (check-bag bag)
  (sob-empty? bag))

(define (sob-half-disjoint? a b)
  (let ((ha (sob-hash-table a))
        (hb (sob-hash-table b)))
    (call/cc
      (lambda (return)
        (hash-table-for-each
          (lambda (key val) (if (hash-table-contains? hb key) (return #f)))
          ha)
      #t))))

(define (set-disjoint? a b)
  (check-set a)
  (check-set b)
  (check-same-comparator a b)
  (and (sob-half-disjoint? a b) (sob-half-disjoint? b a)))

(define (bag-disjoint? a b)
  (check-bag a)
  (check-bag b)
  (check-same-comparator a b)
  (and (sob-half-disjoint? a b) (sob-half-disjoint? b a)))

;; Accessors

(define (sob-member sob element default)
  (define (same? a b) (=? (sob-comparator sob) a b))
  (call/cc
    (lambda (return)
      (hash-table-for-each
        (lambda (key val) (if (same? key element) (return key)))
        (sob-hash-table sob))
      default)))

(define (set-member set element default)
  (check-set set)
  (sob-member set element default))

(define (bag-member bag element default)
  (check-bag bag)
  (sob-member bag element default))

(define (set-element-comparator set)
  (check-set set)
  (sob-comparator set))

(define (bag-element-comparator bag)
  (check-bag bag)
  (sob-comparator bag))


;; Updaters (pure functional and linear update)

(define (sob-increment! sob element count)
  (check-element sob element)
  (hash-table-update!/default
    (sob-hash-table sob)
    element
    (if (sob-multi? sob)
      (lambda (value) (+ value count))
      (lambda (value) 1))
    0))

(define (sob-decrement! sob element count)
  (hash-table-update!/default
    (sob-hash-table sob)
    element
    (lambda (value) (- value count))
    0))

(define (sob-cleanup! sob)
  (let ((ht (sob-hash-table sob)))
    (for-each (lambda (key) (hash-table-delete! ht key))
              (nonpositive-keys ht))
    sob))

(define (nonpositive-keys ht)
  (let ((result '()))
    (hash-table-for-each
      (lambda (key value)
        (when (<= value 0)
          (set! result (cons key result))))
      ht)
    result))

(define (bag-increment! bag element count)
  (check-bag bag)
  (sob-increment! bag element count)
  bag)

(define (bag-decrement! bag element count)
  (check-bag bag)
  (sob-decrement! bag element count)
  (sob-cleanup! bag)
  bag)

(define (sob-adjoin-all! sob elements)
  (for-each
    (lambda (elem)
      (sob-increment! sob elem 1))
    elements))

(define (set-adjoin! set . elements)
  (check-set set)
  (sob-adjoin-all! set elements)
  set)

(define (bag-adjoin! bag . elements)
  (check-bag bag)
  (sob-adjoin-all! bag elements)
  bag)


(define (set-adjoin set . elements)
  (check-set set)
  (let ((result (sob-copy set)))
    (sob-adjoin-all! result elements)
    result))

(define (bag-adjoin bag . elements)
  (check-bag bag)
  (let ((result (sob-copy bag)))
    (sob-adjoin-all! result elements)
    result))

(define (sob-replace! sob element)
  (let* ((comparator (sob-comparator sob))
         (= (comparator-equality-predicate comparator))
         (ht (sob-hash-table sob)))
    (comparator-check-type comparator element)
    (call/cc
      (lambda (return)
        (hash-table-for-each
          (lambda (key value)
            (when (= key element)
              (hash-table-delete! ht key)
              (hash-table-set! ht element value)
              (return sob)))
          ht)
        sob))))

(define (set-replace! set element)
  (check-set set)
  (sob-replace! set element)
  set)

(define (bag-replace! bag element)
  (check-bag bag)
  (sob-replace! bag element)
  bag)

(define (set-replace set element)
  (check-set set)
  (let ((result (sob-copy set)))
    (sob-replace! result element)
    result))

(define (bag-replace bag element)
  (check-bag bag)
  (let ((result (sob-copy bag)))
    (sob-replace! result element)
    result))

(define (sob-delete-all! sob elements)
  (for-each (lambda (element) (sob-decrement! sob element 1)) elements)
  (sob-cleanup! sob)
  sob)

(define (set-delete! set . elements)
  (check-set set)
  (sob-delete-all! set elements))

(define (bag-delete! bag . elements)
  (check-bag bag)
  (sob-delete-all! bag elements))

(define (set-delete-all! set elements)
  (check-set set)
  (sob-delete-all! set elements))

(define (bag-delete-all! bag elements)
  (check-bag bag)
  (sob-delete-all! bag elements))

(define (set-delete set . elements)
  (check-set set)
  (sob-delete-all! (sob-copy set) elements))

(define (bag-delete bag . elements)
  (check-bag bag)
  (sob-delete-all! (sob-copy bag) elements))

(define (set-delete-all set elements)
  (check-set set)
  (sob-delete-all! (sob-copy set) elements))

(define (bag-delete-all bag elements)
  (check-bag bag)
  (sob-delete-all! (sob-copy bag) elements))

(define missing (string-copy "missing"))

(define (sob-search! sob element failure success)
  (define (insert obj)
    (sob-increment! sob element 1)
    (values sob obj))
  (define (ignore obj)
    (values sob obj))
  (define (update new-elem obj)
    (sob-decrement! sob element 1)
    (sob-increment! sob new-elem 1)
    (values (sob-cleanup! sob) obj))
  (define (remove obj)
    (sob-decrement! sob element 1)
    (values (sob-cleanup! sob) obj))
  (let ((true-element (sob-member sob element missing)))
    (if (eq? true-element missing)
      (failure insert ignore)
      (success true-element update remove))))

(define (set-search! set element failure success)
  (check-set set)
  (sob-search! set element failure success))

(define (bag-search! bag element failure success)
  (check-bag bag)
  (sob-search! bag element failure success))

(define (sob-size sob)
  (if (sob-multi? sob)
    (let ((result 0))
      (hash-table-for-each
        (lambda (elem count) (set! result (+ count result)))
        (sob-hash-table sob))
      result)
    (hash-table-size (sob-hash-table sob))))

(define (set-size set)
  (check-set set)
  (sob-size set))

(define (bag-size bag)
  (check-bag bag)
  (sob-size bag))

(define (sob-find pred sob failure)
  (call/cc
    (lambda (return)
      (hash-table-for-each
        (lambda (key value)
          (if (pred key) (return key)))
        (sob-hash-table sob))
    (failure))))

(define (set-find pred set failure)
  (check-set set)
  (sob-find pred set failure))

(define (bag-find pred bag failure)
  (check-bag bag)
  (sob-find pred bag failure))

(define (sob-count pred sob)
  (sob-fold
    (lambda (elem total) (if (pred elem) (+ total 1) total))
    0
    sob))

(define (set-count pred set)
  (check-set set)
  (sob-count pred set))

(define (bag-count pred bag)
  (check-bag bag)
  (sob-count pred bag))

(define (sob-any? pred sob)
  (call/cc
    (lambda (return)
      (hash-table-for-each
        (lambda (elem value) (if (pred elem) (return #t)))
        (sob-hash-table sob))
      #f)))

(define (set-any? pred set)
  (check-set set)
  (sob-any? pred set))

(define (bag-any? pred bag)
  (check-bag bag)
  (sob-any? pred bag))

(define (sob-every? pred sob)
  (call/cc
    (lambda (return)
      (hash-table-for-each
        (lambda (elem value) (if (not (pred elem)) (return #f)))
        (sob-hash-table sob))
      #t)))

(define (set-every? pred set)
  (check-set set)
  (sob-every? pred set))

(define (bag-every? pred bag)
  (check-bag bag)
  (sob-every? pred bag))


;;; Mapping and folding

(define (do-n-times cmd n)
  (let loop ((n n))
    (when (> n 0)
      (cmd)
      (loop (- n 1)))))

(define (sob-for-each proc sob)
  (hash-table-for-each
    (lambda (key value) (do-n-times (lambda () (proc key)) value))
    (sob-hash-table sob)))

(define (set-for-each proc set)
  (check-set set)
  (sob-for-each proc set))

(define (bag-for-each proc bag)
  (check-bag bag)
  (sob-for-each proc bag))

(define (sob-map comparator proc sob)
  (let ((result (make-sob comparator (sob-multi? sob))))
    (hash-table-for-each
      (lambda (key value) (sob-increment! result (proc key) value))
      (sob-hash-table sob))
    result))

(define (set-map comparator proc set)
  (check-set set)
  (sob-map comparator proc set))

(define (bag-map comparator proc bag)
  (check-bag bag)
  (sob-map comparator proc bag))

(define (sob-fold proc nil sob)
  (let ((result nil))
    (sob-for-each
      (lambda (elem) (set! result (proc elem result)))
      sob)
    result))

(define (set-fold proc nil set)
  (check-set set)
  (sob-fold proc nil set))

(define (bag-fold proc nil bag)
  (check-bag bag)
  (sob-fold proc nil bag))

(define (sob-filter pred sob)
  (let ((result (sob-empty-copy sob)))
    (hash-table-for-each
      (lambda (key value)
        (if (pred key) (sob-increment! result key value)))
      (sob-hash-table sob))
    result))

(define (set-filter pred set)
  (check-set set)
  (sob-filter pred set))

(define (bag-filter pred bag)
  (check-bag bag)
  (sob-filter pred bag))

(define (set-remove pred set)
  (check-set set)
  (sob-filter (lambda (x) (not (pred x))) set))

(define (bag-remove pred bag)
  (check-bag bag)
  (sob-filter (lambda (x) (not (pred x))) bag))

(define (sob-filter! pred sob)
  (hash-table-for-each
    (lambda (key value)
      (if (not (pred key)) (sob-decrement! sob key value)))
    (sob-hash-table sob))
  (sob-cleanup! sob))

(define (set-filter! pred set)
  (check-set set)
  (sob-filter! pred set))

(define (bag-filter! pred bag)
  (check-bag bag)
  (sob-filter! pred bag))

(define (set-remove! pred set)
  (check-set set)
  (sob-filter! (lambda (x) (not (pred x))) set))

(define (bag-remove! pred bag)
  (check-bag bag)
  (sob-filter! (lambda (x) (not (pred x))) bag))

(define (sob-partition pred sob)
  (let ((res1 (sob-empty-copy sob))
        (res2 (sob-empty-copy sob)))
    (hash-table-for-each
      (lambda (key value)
        (if (pred key)
          (sob-increment! res1 key value)
          (sob-increment! res2 key value)))
      (sob-hash-table sob))
    (values res1 res2)))

(define (set-partition pred set)
  (check-set set)
  (sob-partition pred set))

(define (bag-partition pred bag)
  (check-bag bag)
  (sob-partition pred bag))

(define (sob-partition! pred sob)
  (let ((result (sob-empty-copy sob)))
    (hash-table-for-each
      (lambda (key value)
        (if (not (pred key))
          (begin
            (sob-decrement! sob key value)
            (sob-increment! result key value))))
      (sob-hash-table sob))
    (values (sob-cleanup! sob) result)))

(define (set-partition! pred set)
  (check-set set)
  (sob-partition! pred set))

(define (bag-partition! pred bag)
  (check-bag bag)
  (sob-partition! pred bag))


;;; Copying and conversion

(define (sob->list sob)
  (sob-fold (lambda (elem list) (cons elem list)) '() sob))

(define (set->list set)
  (check-set set)
  (sob->list set))

(define (bag->list bag)
  (check-bag bag)
  (sob->list bag))

(define (list->sob! sob list)
  (for-each (lambda (elem) (sob-increment! sob elem 1)) list)
  sob)

(define (list->set comparator list)
  (list->sob! (make-sob comparator #f) list))

(define (list->bag comparator list)
  (list->sob! (make-sob comparator #t) list))

(define (list->set! set list)
  (check-set set)
  (list->sob! set list))

(define (list->bag! bag list)
  (check-bag bag)
  (list->sob! bag list))


;;; Subsets

(define sob=?
  (case-lambda
    ((sob) #t)
    ((sob1 sob2) (dyadic-sob=? sob1 sob2))
    ((sob1 sob2 . sobs)
     (and (dyadic-sob=? sob1 sob2)
          (apply sob=? sob2 sobs)))))

(define (set=? . sets)
  (check-all-sets sets)
  (apply sob=? sets))

(define (bag=? . bags)
  (check-all-bags bags)
  (apply sob=? bags))

(define (dyadic-sob=? sob1 sob2)
  (call/cc
    (lambda (return)
      (let ((ht1 (sob-hash-table sob1))
            (ht2 (sob-hash-table sob2)))
        (if (not (= (hash-table-size ht1) (hash-table-size ht2)))
          (return #f))
        (hash-table-for-each
          (lambda (key value)
            (if (not (= value (hash-table-ref/default ht2 key 0)))
              (return #f)))
          ht1))
     #t)))

(define sob<=?
  (case-lambda
    ((sob) #t)
    ((sob1 sob2) (dyadic-sob<=? sob1 sob2))
    ((sob1 sob2 . sobs)
     (and (dyadic-sob<=? sob1 sob2)
          (apply sob<=? sob2 sobs)))))

(define (set<=? . sets)
  (check-all-sets sets)
  (apply sob<=? sets))

(define (bag<=? . bags)
  (check-all-bags bags)
  (apply sob<=? bags))

(define (dyadic-sob<=? sob1 sob2)
  (call/cc
    (lambda (return)
      (let ((ht1 (sob-hash-table sob1))
            (ht2 (sob-hash-table sob2)))
        (if (not (<= (hash-table-size ht1) (hash-table-size ht2)))
          (return #f))
        (hash-table-for-each
          (lambda (key value)
            (if (not (<= value (hash-table-ref/default ht2 key 0)))
              (return #f)))
          ht1))
      #t)))

(define sob<?
  (case-lambda
    ((sob) #t)
    ((sob1 sob2) (dyadic-sob<? sob1 sob2))
    ((sob1 sob2 . sobs)
     (and (dyadic-sob<? sob1 sob2)
          (apply sob<? sob2 sobs)))))

(define (set<? . sets)
  (check-all-sets sets)
  (apply sob<? sets))

(define (bag<? . bags)
  (check-all-bags bags)
  (apply sob<? bags))

(define (dyadic-sob<? sob1 sob2)
  (call/cc
    (lambda (return)
      (let ((ht1 (sob-hash-table sob1))
            (ht2 (sob-hash-table sob2)))
        (let ((smaller-count
               (cond
                ((< (hash-table-size ht1) (hash-table-size ht2)) 1)
                ((= (hash-table-size ht1) (hash-table-size ht2)) 0)
                (else (return #f)))))
          (hash-table-for-each
           (lambda (key value)
             (let ((value2 (hash-table-ref/default ht2 key 0)))
               (if (not (<= value value2))
                 (return #f)
                 (if (< value value2)
                   (set! smaller-count (+ smaller-count 1))))))
           ht1)
          (positive? smaller-count))))))

(define sob>?
  (case-lambda
    ((sob) #t)
    ((sob1 sob2) (dyadic-sob>? sob1 sob2))
    ((sob1 sob2 . sobs)
     (and (dyadic-sob>? sob1 sob2)
          (apply sob>? sob2 sobs)))))

(define (set>? . sets)
  (check-all-sets sets)
  (apply sob>? sets))

(define (bag>? . bags)
  (check-all-bags bags)
  (apply sob>? bags))

(define (dyadic-sob>? sob1 sob2)
  (dyadic-sob<? sob2 sob1))

(define sob>=?
  (case-lambda
    ((sob) #t)
    ((sob1 sob2) (dyadic-sob>=? sob1 sob2))
    ((sob1 sob2 . sobs)
     (and (dyadic-sob>=? sob1 sob2)
          (apply sob>=? sob2 sobs)))))

(define (set>=? . sets)
  (check-all-sets sets)
  (apply sob>=? sets))

(define (bag>=? . bags)
  (check-all-bags bags)
  (apply sob>=? bags))

(define (dyadic-sob>=? sob1 sob2)
  (dyadic-sob<=? sob2 sob1))


;;; Set theory operations

(define (max-one n multi?)
    (if multi? n (if (> n 1) 1 n)))

(define (sob-union sob1 . sobs)
  (if (null? sobs)
    sob1
    (let ((result (sob-empty-copy sob1)))
      (dyadic-sob-union! result sob1 (car sobs))
      (for-each
       (lambda (sob) (dyadic-sob-union! result result sob))
       (cdr sobs))
      result)))

(define (dyadic-sob-union! result sob1 sob2)
  (let ((sob1-ht (sob-hash-table sob1))
        (sob2-ht (sob-hash-table sob2))
        (result-ht (sob-hash-table result)))
    (hash-table-for-each
      (lambda (key value1)
        (let ((value2 (hash-table-ref/default sob2-ht key 0)))
          (hash-table-set! result-ht key (max value1 value2))))
      sob1-ht)
    (hash-table-for-each
      (lambda (key value2)
        (let ((value1 (hash-table-ref/default sob1-ht key 0)))
          (if (= value1 0)
              (hash-table-set! result-ht key value2))))
      sob2-ht)))

(define (set-union . sets)
  (check-all-sets sets)
  (apply sob-union sets))

(define (bag-union . bags)
  (check-all-bags bags)
  (apply sob-union bags))

(define (sob-union! sob1 . sobs)
  (for-each
   (lambda (sob) (dyadic-sob-union! sob1 sob1 sob))
   sobs)
  sob1)

(define (set-union! . sets)
  (check-all-sets sets)
  (apply sob-union! sets))

(define (bag-union! . bags)
  (check-all-bags bags)
  (apply sob-union! bags))

(define (sob-intersection sob1 . sobs)
  (if (null? sobs)
    sob1
    (let ((result (sob-empty-copy sob1)))
      (dyadic-sob-intersection! result sob1 (car sobs))
      (for-each
       (lambda (sob) (dyadic-sob-intersection! result result sob))
       (cdr sobs))
      (sob-cleanup! result))))

(define (dyadic-sob-intersection! result sob1 sob2)
  (let ((sob1-ht (sob-hash-table sob1))
        (sob2-ht (sob-hash-table sob2))
        (result-ht (sob-hash-table result)))
    (hash-table-for-each
      (lambda (key value1)
        (let ((value2 (hash-table-ref/default sob2-ht key 0)))
          (hash-table-set! result-ht key (min value1 value2))))
      sob1-ht)))

(define (set-intersection . sets)
  (check-all-sets sets)
  (apply sob-intersection sets))

(define (bag-intersection . bags)
  (check-all-bags bags)
  (apply sob-intersection bags))

(define (sob-intersection! sob1 . sobs)
  (for-each
   (lambda (sob) (dyadic-sob-intersection! sob1 sob1 sob))
   sobs)
  (sob-cleanup! sob1))

(define (set-intersection! . sets)
  (check-all-sets sets)
  (apply sob-intersection! sets))

(define (bag-intersection! . bags)
  (check-all-bags bags)
  (apply sob-intersection! bags))

(define (sob-difference sob1 . sobs)
  (if (null? sobs)
    sob1
    (let ((result (sob-empty-copy sob1)))
      (dyadic-sob-difference! result sob1 (car sobs))
      (for-each
       (lambda (sob) (dyadic-sob-difference! result result sob))
       (cdr sobs))
      (sob-cleanup! result))))

(define (dyadic-sob-difference! result sob1 sob2)
  (let ((sob1-ht (sob-hash-table sob1))
        (sob2-ht (sob-hash-table sob2))
        (result-ht (sob-hash-table result)))
    (hash-table-for-each
      (lambda (key value1)
        (let ((value2 (hash-table-ref/default sob2-ht key 0)))
          (hash-table-set! result-ht key (- value1 value2))))
      sob1-ht)))

(define (set-difference . sets)
  (check-all-sets sets)
  (apply sob-difference sets))

(define (bag-difference . bags)
  (check-all-bags bags)
  (apply sob-difference bags))

(define (sob-difference! sob1 . sobs)
  (for-each
   (lambda (sob) (dyadic-sob-difference! sob1 sob1 sob))
   sobs)
  (sob-cleanup! sob1))

(define (set-difference! . sets)
  (check-all-sets sets)
  (apply sob-difference! sets))

(define (bag-difference! . bags)
  (check-all-bags bags)
  (apply sob-difference! bags))

(define (sob-sum sob1 . sobs)
  (if (null? sobs)
    sob1
    (let ((result (sob-empty-copy sob1)))
      (dyadic-sob-sum! result sob1 (car sobs))
      (for-each
       (lambda (sob) (dyadic-sob-sum! result result sob))
       (cdr sobs))
      result)))

(define (dyadic-sob-sum! result sob1 sob2)
  (let ((sob1-ht (sob-hash-table sob1))
        (sob2-ht (sob-hash-table sob2))
        (result-ht (sob-hash-table result)))
    (hash-table-for-each
      (lambda (key value1)
        (let ((value2 (hash-table-ref/default sob2-ht key 0)))
          (hash-table-set! result-ht key (+ value1 value2))))
      sob1-ht)
    (hash-table-for-each
      (lambda (key value2)
        (let ((value1 (hash-table-ref/default sob1-ht key 0)))
          (if (= value1 0)
              (hash-table-set! result-ht key value2))))
      sob2-ht)))

(define (bag-sum . bags)
  (check-all-bags bags)
  (apply sob-sum bags))

(define (sob-sum! sob1 . sobs)
  (for-each
   (lambda (sob) (dyadic-sob-sum! sob1 sob1 sob))
   sobs)
  sob1)

(define (bag-sum! . bags)
  (check-all-bags bags)
  (apply sob-sum! bags))

(define (sob-xor! result sob1 sob2)
  (let ((sob1-ht (sob-hash-table sob1))
        (sob2-ht (sob-hash-table sob2))
        (result-ht (sob-hash-table result)))
    (hash-table-for-each
      (lambda (key value2)
        (let ((value1 (hash-table-ref/default sob1-ht key 0)))
          (if (= value1 0)
              (hash-table-set! result-ht key value2))))
      sob2-ht)
    (hash-table-for-each
      (lambda (key value1)
        (let ((value2 (hash-table-ref/default sob2-ht key 0)))
          (hash-table-set! result-ht key (abs (- value1 value2)))))
      sob1-ht)
    (sob-cleanup! result)))

(define (set-xor set1 set2)
  (check-set set1)
  (check-set set2)
  (check-same-comparator set1 set2)
  (sob-xor! (sob-empty-copy set1) set1 set2))

(define (bag-xor bag1 bag2)
  (check-bag bag1)
  (check-bag bag2)
  (check-same-comparator bag1 bag2)
  (sob-xor! (sob-empty-copy bag1) bag1 bag2))

(define (set-xor! set1 set2)
  (check-set set1)
  (check-set set2)
  (check-same-comparator set1 set2)
  (sob-xor! set1 set1 set2))

(define (bag-xor! bag1 bag2)
  (check-bag bag1)
  (check-bag bag2)
  (check-same-comparator bag1 bag2)
  (sob-xor! bag1 bag1 bag2))


;;; A few bag-specific procedures

(define (sob-product! n result sob)
  (let ((rht (sob-hash-table result)))
    (hash-table-for-each
      (lambda (elem count) (hash-table-set! rht elem (* count n)))
      (sob-hash-table sob))
    result))

(define (valid-n n)
   (and (integer? n) (exact? n) (positive? n)))

(define (bag-product n bag)
  (check-bag bag)
  (valid-n n)
  (sob-product! n (sob-empty-copy bag) bag))

(define (bag-product! n bag)
  (check-bag bag)
  (valid-n n)
  (sob-product! n bag bag))

(define (bag-unique-size bag)
  (check-bag bag)
  (hash-table-size (sob-hash-table bag)))

(define (bag-element-count bag elem)
  (check-bag bag)
  (hash-table-ref/default (sob-hash-table bag) elem 0))

(define (bag-for-each-unique proc bag)
  (check-bag bag)
  (hash-table-for-each
    (lambda (key value) (proc key value))
    (sob-hash-table bag)))

(define (bag-fold-unique proc nil bag)
  (check-bag bag)
  (let ((result nil))
    (hash-table-for-each
      (lambda (elem count) (set! result (proc elem count result)))
      (sob-hash-table bag))
    result))

(define (bag->set bag)
  (check-bag bag)
  (let ((result (make-sob (sob-comparator bag) #f)))
    (hash-table-for-each
      (lambda (key value) (sob-increment! result key value))
      (sob-hash-table bag))
    result))

(define (set->bag set)
  (check-set set)
  (let ((result (make-sob (sob-comparator set) #t)))
    (hash-table-for-each
      (lambda (key value) (sob-increment! result key value))
      (sob-hash-table set))
    result))

(define (set->bag! bag set)
  (check-bag bag)
  (check-set set)
  (check-same-comparator set bag)
  (hash-table-for-each
    (lambda (key value) (sob-increment! bag key value))
    (sob-hash-table set))
  bag)

(define (bag->alist bag)
  (check-bag bag)
  (bag-fold-unique
    (lambda (elem count list) (cons (cons elem count) list))
    '()
    bag))

(define (alist->bag comparator alist)
  (let* ((result (bag comparator))
         (ht (sob-hash-table result)))
    (for-each
      (lambda (assoc)
        (let ((element (car assoc)))
          (if (not (hash-table-contains? ht element))
              (sob-increment! result element (cdr assoc)))))
      alist)
    result))

;;; Comparators

(define (sob-hash sob)
  (let* ((ht (sob-hash-table sob))
         (hash (comparator-hash-function (sob-comparator sob))))
    (sob-fold
      (lambda (element result) (+ (hash element) result))
      5381
      sob)))

(define set-comparator (make-comparator set? set=? #f sob-hash))

(define bag-comparator (make-comparator bag? bag=? #f sob-hash))

(comparator-register-default! set-comparator)
(comparator-register-default! bag-comparator)

(define (sob-print sob port)
  (display (if (sob-multi? sob) "&bag[" "&set[") port)
  (sob-for-each
    (lambda (elem) (display " " port) (write elem port))
    sob)
  (display " ]" port))

;; Chicken-specific; the else branch (empty) is taken everywhere else.
(cond-expand
  (chicken
    (define-record-printer sob sob-print))
  (else))

))
