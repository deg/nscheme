;;; (srfi 125) — Intermediate hash tables.
;;;
;;; Vendored from the SRFI 125 reference implementation by William D
;;; Clinger (2015). Flattened into a single file: the upstream .sld uses
;;; (include "125.body.scm"), but nscheme resolves `include` relative to
;;; the process directory, so the body is inlined below inside `begin`.
;;;
;;; SUBSTITUTION NOTE (see also missing_features in the integration
;;; report): the upstream library is implemented on top of SRFI 126
;;; R6RS-style hashtables `(import (srfi 126))`, which nscheme does not
;;; provide and which need no native primitive. We therefore supply a
;;; small, self-contained `hashtable-*` shim in pure (scheme base),
;;; backed by an association list, inside the first `begin` block. It is
;;; correctness-oriented, not performance-oriented (lookups are O(n)).
;;; The upstream body is otherwise preserved verbatim.
;;;
;;; The upstream `(except (srfi 128) hash-salt string-hash string-ci-hash
;;; symbol-hash)` excluded those names only because (srfi 126) re-supplied
;;; them; with 126 gone we import (srfi 128) plainly so they resolve there.
;;;
;;; SPDX-License-Identifier: LicenseRef-Clinger
;;; Copyright 2015 William D Clinger.
;;;
;;; Permission to copy this software, in whole or in part, to use this
;;; software for any lawful purpose, and to redistribute this software
;;; is granted subject to the restriction that all copies made of this
;;; software must include this copyright and permission notice in full.

(define-library (srfi 125)

  (export

   make-hash-table
   hash-table
   hash-table-unfold
   alist->hash-table

   hash-table?
   hash-table-contains?
   hash-table-empty?
   hash-table=?
   hash-table-mutable?

   hash-table-ref
   hash-table-ref/default

   hash-table-set!
   hash-table-delete!
   hash-table-intern!
   hash-table-update!
   hash-table-update!/default
   hash-table-pop!
   hash-table-clear!

   hash-table-size
   hash-table-keys
   hash-table-values
   hash-table-entries
   hash-table-find
   hash-table-count

   hash-table-map
   hash-table-for-each
   hash-table-map!
   hash-table-map->list
   hash-table-fold
   hash-table-prune!

   hash-table-copy
   hash-table-empty-copy
   hash-table->alist

   hash-table-union!
   hash-table-intersection!
   hash-table-difference!
   hash-table-xor!

   ;; The following procedures are deprecated by SRFI 125:

   (rename deprecated:hash                     hash)
   (rename deprecated:string-hash              string-hash)
   (rename deprecated:string-ci-hash           string-ci-hash)
   (rename deprecated:hash-by-identity         hash-by-identity)

   (rename deprecated:hash-table-equivalence-function
                                               hash-table-equivalence-function)
   (rename deprecated:hash-table-hash-function hash-table-hash-function)
   (rename deprecated:hash-table-exists?       hash-table-exists?)
   (rename deprecated:hash-table-walk          hash-table-walk)
   (rename deprecated:hash-table-merge!        hash-table-merge!)

   )

  (import (scheme base)
          (scheme write) ; for warnings about deprecated features
          (srfi 128))

  (cond-expand
   ((library (scheme char))
    (import (scheme char)))
   (else
    (begin (define string-ci=? string=?))))

  ;; --------------------------------------------------------------------
  ;; SRFI 126 hashtable shim (substituting for the missing (srfi 126)).
  ;; An alist-backed mutable record. The stored hash function is kept only
  ;; so `hashtable-hash-function` can return it; it is never used for
  ;; storage. Equality is decided by the stored `equiv` predicate.
  ;; --------------------------------------------------------------------
  (begin

    ;; (srfi 128) defines `equal-hash` internally but does not export it,
    ;; so it is out of scope here even though `%make-hash-table` (from the
    ;; upstream body, below) calls it for `equal?`-keyed tables. We supply
    ;; the same trivial fallback the comparator library uses. Since our
    ;; alist shim never uses the hash function for storage, this only ever
    ;; flows back out through the deprecated `hash-table-hash-function`.
    (define (equal-hash x) 0)

    (define-record-type srfi126-hashtable
      (%make-ht equiv hashfn cells mutable?)
      srfi126-hashtable?
      (equiv    ht-equiv)
      (hashfn   ht-hashfn)
      ;; cells is a one-element list whose car is the alist of (key . val)
      (cells    ht-cells)
      (mutable? ht-mutable?))

    (define (%ht-alist ht) (car (ht-cells ht)))
    (define (%ht-set-alist! ht a) (set-car! (ht-cells ht) a))

    (define (%ht-assoc ht key)
      (let ((eq (ht-equiv ht)))
        (let loop ((a (%ht-alist ht)))
          (cond ((null? a) #f)
                ((eq (caar a) key) (car a))
                (else (loop (cdr a)))))))

    (define (make-hashtable hashfn equiv . rest)
      (%make-ht equiv hashfn (list '()) #t))

    (define (make-eq-hashtable . rest)
      (%make-ht eq? #f (list '()) #t))

    (define (make-eqv-hashtable . rest)
      (%make-ht eqv? #f (list '()) #t))

    (define (hashtable? obj) (srfi126-hashtable? obj))

    (define (hashtable-mutable? ht) (ht-mutable? ht))

    (define (%check-mutable ht who)
      (if (not (ht-mutable? ht))
          (error (string-append who ": hashtable is immutable") ht)))

    (define (hashtable-size ht) (length (%ht-alist ht)))

    (define (hashtable-contains? ht key)
      (and (%ht-assoc ht key) #t))

    (define (hashtable-ref ht key default)
      (let ((cell (%ht-assoc ht key)))
        (if cell (cdr cell) default)))

    (define (hashtable-set! ht key val)
      (%check-mutable ht "hashtable-set!")
      (let ((cell (%ht-assoc ht key)))
        (if cell
            (set-cdr! cell val)
            (%ht-set-alist! ht (cons (cons key val) (%ht-alist ht)))))
      (if #f #f))

    (define (hashtable-delete! ht key)
      (%check-mutable ht "hashtable-delete!")
      (let ((eq (ht-equiv ht)))
        (%ht-set-alist!
         ht
         (let loop ((a (%ht-alist ht)))
           (cond ((null? a) '())
                 ((eq (caar a) key) (cdr a))
                 (else (cons (car a) (loop (cdr a))))))))
      (if #f #f))

    (define (hashtable-clear! ht . rest)
      (%check-mutable ht "hashtable-clear!")
      (%ht-set-alist! ht '())
      (if #f #f))

    (define (hashtable-keys ht)
      (list->vector (map car (%ht-alist ht))))

    (define (hashtable-entries ht)
      (let ((a (%ht-alist ht)))
        (values (list->vector (map car a))
                (list->vector (map cdr a)))))

    ;; R6RS: (hashtable-copy ht [mutable?]); mutable? defaults to #f.
    (define (hashtable-copy ht . rest)
      (let ((mutable? (and (not (null? rest)) (car rest))))
        (%make-ht (ht-equiv ht)
                  (ht-hashfn ht)
                  (list (map (lambda (p) (cons (car p) (cdr p)))
                             (%ht-alist ht)))
                  (and mutable? #t))))

    (define (hashtable-equivalence-function ht) (ht-equiv ht))

    (define (hashtable-hash-function ht) (ht-hashfn ht))

    )

  (begin

;;; SPDX-License-Identifier: LicenseRef-Clinger
;;; Copyright 2015 William D Clinger.
;;;
;;; Permission to copy this software, in whole or in part, to use this
;;; software for any lawful purpose, and to redistribute this software
;;; is granted subject to the restriction that all copies made of this
;;; software must include this copyright and permission notice in full.
;;;
;;; I also request that you send me a copy of any improvements that you
;;; make to this software so that they may be incorporated within it to
;;; the benefit of the Scheme community.

;;; Private stuff, not exported.

;;; Ten of the SRFI 125 procedures are deprecated, and another
;;; two allow alternative arguments that are deprecated.

(define (issue-deprecated-warnings?) #t)

(define (issue-warning-deprecated name-of-deprecated-misfeature)
  (if (not (memq name-of-deprecated-misfeature already-warned))
      (begin
       (set! already-warned
             (cons name-of-deprecated-misfeature already-warned))
       (if (issue-deprecated-warnings?)
           (let ((out (current-error-port)))
             (display "WARNING: " out)
             (display name-of-deprecated-misfeature out)
             (newline out)
             (display "    is deprecated by SRFI 125.  See" out)
             (newline out)
             (display "    " out)
             (display url:deprecated out)
             (newline out))))))

(define url:deprecated
  "http://srfi.schemers.org/srfi-125/srfi-125.html")

; List of deprecated features for which a warning has already
; been issued.

(define already-warned '())

;;; If %enforce-comparator-type-tests is true, then make-hash-table,
;;; when passed a comparator, will use a hash function that enforces
;;; the comparator's type test.

(define %enforce-comparator-type-tests #t)

;;; Given a comparator, return its hash function, possibly augmented
;;; by the comparator's type test.

(define (%comparator-hash-function comparator)
  (let ((okay? (comparator-type-test-predicate comparator))
        (hash-function (comparator-hash-function comparator)))
    (if %enforce-comparator-type-tests
        (lambda (x . rest)
          (cond ((not (okay? x))
                 (error "key rejected by hash-table comparator"
                        x
                        comparator))
                ((null? rest)
                 (hash-function x))
                (else
                 (apply hash-function x rest))))
        hash-function)))

;;; A unique (in the sense of eq?) value that will never be found
;;; within a hash-table.

(define %not-found (list '%not-found))

;;; A unique (in the sense of eq?) value that escapes only as an irritant
;;; when a hash-table key is not found.

(define %not-found-irritant (list 'not-found))

;;; The error message used when a hash-table key is not found.

(define %not-found-message "hash-table key not found")

;;; FIXME: thread-safe, weak-keys, ephemeral-keys, weak-values,
;;; and ephemeral-values are not supported by this portable
;;; reference implementation.

(define (%check-optional-arguments procname args)
  (if (or (memq 'thread-safe args)
          (memq 'weak-keys args)
          (memq 'weak-values args)
          (memq 'ephemeral-keys args)
          (memq 'ephemeral-values args))
      (error (string-append (symbol->string procname)
                            ": unsupported optional argument(s)")
             args)))

;;; This was exported by an earlier draft of SRFI 125,
;;; and is still used by hash-table=?

(define (hash-table-every proc ht)
  (call-with-values
   (lambda () (hash-table-entries ht))
   (lambda (keys vals)
     (let loop ((keys keys)
                (vals vals))
       (if (null? keys)
           #t
           (let* ((key (car keys))
                  (val (car vals))
                  (x   (proc key val)))
             (and x
                  (loop (cdr keys)
                        (cdr vals)))))))))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;;
;;; Exported procedures
;;;
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

;;; Constructors.

(define (make-hash-table comparator/equiv . rest)
  (if (comparator? comparator/equiv)
      (let ((equiv (comparator-equality-predicate comparator/equiv))
            (hash-function (%comparator-hash-function comparator/equiv)))
        (%make-hash-table equiv hash-function rest))
      (let* ((equiv comparator/equiv)
             (hash-function (if (and (not (null? rest))
                                     (procedure? (car rest)))
                                (car rest)
                                #f))
             (rest (if hash-function (cdr rest) rest)))
        (issue-warning-deprecated 'srfi-69-style:make-hash-table)
        (%make-hash-table equiv hash-function rest))))

(define (%make-hash-table equiv hash-function opts)
  (%check-optional-arguments 'make-hash-table opts)
  (cond ((equal? equiv eq?)
         (make-eq-hashtable))
        ((equal? equiv eqv?)
         (make-eqv-hashtable))
        (hash-function
         (make-hashtable hash-function equiv))
        ((equal? equiv equal?)
         (make-hashtable equal-hash equiv))
        ((equal? equiv string=?)
         (make-hashtable string-hash equiv))
        ((equal? equiv string-ci=?)
         (make-hashtable string-ci-hash equiv))
        ((equal? equiv symbol=?)
         (make-hashtable symbol-hash equiv))
        (else
         (error "make-hash-table: unable to infer hash function"
                equiv))))

(define (hash-table comparator . rest)
  (let ((ht (apply make-hash-table comparator rest)))
    (let loop ((kvs rest))
      (cond
       ((null? kvs) #f)
       ((null? (cdr kvs)) (error "hash-table: wrong number of arguments"))
       ((hashtable-contains? ht (car kvs))
        (error "hash-table: two equivalent keys were provided"
               (car kvs)))
       (else (hashtable-set! ht (car kvs) (cadr kvs))
             (loop (cddr kvs)))))
    (hashtable-copy ht #f)))

(define (hash-table-unfold stop? mapper successor seed comparator . rest)
  (let ((ht (apply make-hash-table comparator rest)))
    (let loop ((seed seed))
      (if (stop? seed)
          ht
          (call-with-values
           (lambda () (mapper seed))
           (lambda (key val)
             (hash-table-set! ht key val)
             (loop (successor seed))))))))

(define (alist->hash-table alist comparator/equiv . rest)
  (if (and (not (null? rest))
           (procedure? (car rest)))
      (issue-warning-deprecated 'srfi-69-style:alist->hash-table))
  (let ((ht (apply make-hash-table comparator/equiv rest))
        (entries (reverse alist)))
    (for-each (lambda (entry)
                (hash-table-set! ht (car entry) (cdr entry)))
              entries)
    ht))

;;; Predicates.

(define (hash-table? obj)
  (hashtable? obj))

(define (hash-table-contains? ht key)
  (hashtable-contains? ht key))

(define (hash-table-empty? ht)
  (= 0 (hashtable-size ht)))

;;; FIXME: walks both hash tables because their key comparators
;;; might be different

(define (hash-table=? value-comparator ht1 ht2)
  (let ((val=? (comparator-equality-predicate value-comparator))
        (n1 (hash-table-size ht1))
        (n2 (hash-table-size ht2)))
    (and (= n1 n2)
         (hash-table-every (lambda (key val1)
                             (and (hashtable-contains? ht2 key)
                                  (val=? val1
                                         (hashtable-ref ht2 key 'ignored))))
                           ht1)
         (hash-table-every (lambda (key val2)
                             (and (hashtable-contains? ht1 key)
                                  (val=? val2
                                         (hashtable-ref ht1 key 'ignored))))
                           ht2))))

(define (hash-table-mutable? ht)
  (hashtable-mutable? ht))

;;; Accessors.

(define (hash-table-ref ht key . rest)
  (let ((failure (if (null? rest) #f (car rest)))
        (success (if (or (null? rest) (null? (cdr rest))) #f (cadr rest)))
        (val (hashtable-ref ht key %not-found)))
    (cond ((eq? val %not-found)
           (if (and failure (procedure? failure))
               (failure)
               (error %not-found-message ht key %not-found-irritant)))
          (success
           (success val))
          (else
           val))))

(define (hash-table-ref/default ht key default)
  (hashtable-ref ht key default))

;;; Mutators.

(define (hash-table-set! ht . rest)
  (if (= 2 (length rest))
      (hashtable-set! ht (car rest) (cadr rest))
      (let loop ((kvs rest))
        (cond ((and (not (null? kvs))
                    (not (null? (cdr kvs))))
               (hashtable-set! ht (car kvs) (cadr kvs))
               (loop (cddr kvs)))
              ((not (null? kvs))
               (error "hash-table-set!: wrong number of arguments"
                      (cons ht rest)))))))

(define (hash-table-delete! ht . keys)
  (let loop ((keys keys) (cnt 0))
    (cond ((null? keys) cnt)
	  ((hash-table-contains? ht (car keys))
	   (hashtable-delete! ht (car keys))
	   (loop (cdr keys) (+ cnt 1)))
	  (else
	   (loop (cdr keys) cnt)))))

(define (hash-table-intern! ht key failure)
  (if (hashtable-contains? ht key)
      (hash-table-ref ht key)
      (let ((val (failure)))
        (hash-table-set! ht key val)
        val)))

(define (hash-table-update! ht key updater . rest)
  (hash-table-set! ht
                   key
                   (updater (apply hash-table-ref ht key rest))))

(define (hash-table-update!/default ht key updater default)
  (hash-table-set! ht key (updater (hashtable-ref ht key default))))

(define (hash-table-pop! ht)
  (call/cc
    (lambda (return)
      (hash-table-for-each
        (lambda (key value)
          (hash-table-delete! ht key)
          (return key value))
        ht)
      (error "hash-table-pop!: hash table is empty" ht))))

(define (hash-table-clear! ht)
  (hashtable-clear! ht))

;;; The whole hash table.

(define (hash-table-size ht)
  (hashtable-size ht))

(define (hash-table-keys ht)
  (vector->list (hashtable-keys ht)))

(define (hash-table-values ht)
  (call-with-values
   (lambda () (hashtable-entries ht))
   (lambda (keys vals)
     (vector->list vals))))

(define (hash-table-entries ht)
  (call-with-values
   (lambda () (hashtable-entries ht))
   (lambda (keys vals)
     (values (vector->list keys)
             (vector->list vals)))))

(define (hash-table-find proc ht failure)
  (call-with-values
   (lambda () (hash-table-entries ht))
   (lambda (keys vals)
     (let loop ((keys keys)
                (vals vals))
       (if (null? keys)
           (failure)
           (let* ((key (car keys))
                  (val (car vals))
                  (x   (proc key val)))
             (or x
                 (loop (cdr keys)
                       (cdr vals)))))))))

(define (hash-table-count pred ht)
  (call-with-values
   (lambda () (hash-table-entries ht))
   (lambda (keys vals)
     (let loop ((keys keys)
                (vals vals)
                (n 0))
       (if (null? keys)
           n
           (let* ((key (car keys))
                  (val (car vals))
                  (x   (pred key val)))
             (loop (cdr keys)
                   (cdr vals)
                   (if x (+ n 1) n))))))))

;;; Mapping and folding.

(define (hash-table-map proc comparator ht)
  (let ((result (make-hash-table comparator)))
    (hash-table-for-each
     (lambda (key val)
       (hash-table-set! result key (proc val)))
     ht)
    result))

(define (hash-table-map->list proc ht)
  (call-with-values
   (lambda () (hash-table-entries ht))
   (lambda (keys vals)
     (map proc keys vals))))

;;; With this particular implementation, the proc can safely mutate ht.
;;; That property is not guaranteed by the specification, but can be
;;; relied upon by procedures defined in this file.

(define (hash-table-for-each proc ht)
  (call-with-values
   (lambda () (hashtable-entries ht))
   (lambda (keys vals)
     (vector-for-each proc keys vals))))

(define (hash-table-map! proc ht)
  (hash-table-for-each (lambda (key val)
                         (hashtable-set! ht key (proc key val)))
                       ht))

(define (hash-table-fold proc init ht)
  (if (hashtable? proc)
      (deprecated:hash-table-fold proc init ht)
      (call-with-values
       (lambda () (hash-table-entries ht))
       (lambda (keys vals)
         (let loop ((keys keys)
                    (vals vals)
                    (x    init))
           (if (null? keys)
               x
               (loop (cdr keys)
                     (cdr vals)
                     (proc (car keys) (car vals) x))))))))

(define (hash-table-prune! proc ht)
  (hash-table-for-each (lambda (key val)
                         (if (proc key val)
                             (hashtable-delete! ht key)))
                       ht))

;;; Copying and conversion.

(define (hash-table-copy ht . rest)
  (apply hashtable-copy ht rest))

(define (hash-table-empty-copy ht)
  (let* ((ht2 (hashtable-copy ht #t))
         (ignored (hashtable-clear! ht2)))
     ht2))

(define (hash-table->alist ht)
  (call-with-values
   (lambda () (hash-table-entries ht))
   (lambda (keys vals)
     (map cons keys vals))))

;;; Hash tables as sets.

(define (hash-table-union! ht1 ht2)
  (hash-table-for-each
   (lambda (key2 val2)
     (if (not (hashtable-contains? ht1 key2))
         (hashtable-set! ht1 key2 val2)))
   ht2)
  ht1)

(define (hash-table-intersection! ht1 ht2)
  (hash-table-for-each
   (lambda (key1 val1)
     (if (not (hashtable-contains? ht2 key1))
         (hashtable-delete! ht1 key1)))
   ht1)
  ht1)

(define (hash-table-difference! ht1 ht2)
  (hash-table-for-each
   (lambda (key1 val1)
     (if (hashtable-contains? ht2 key1)
         (hashtable-delete! ht1 key1)))
   ht1)
  ht1)

(define (hash-table-xor! ht1 ht2)
  (hash-table-for-each
   (lambda (key2 val2)
     (if (hashtable-contains? ht1 key2)
         (hashtable-delete! ht1 key2)
         (hashtable-set! ht1 key2 val2)))
   ht2)
  ht1)

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;;
;;; The following procedures are deprecated by SRFI 125, but must
;;; be exported nonetheless.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define (deprecated:hash obj . rest)
  (issue-warning-deprecated 'hash)
  (default-hash obj))

(define (deprecated:string-hash obj . rest)
  (issue-warning-deprecated 'srfi-125:string-hash)
  (string-hash obj))

(define (deprecated:string-ci-hash obj . rest)
  (issue-warning-deprecated 'srfi-125:string-ci-hash)
  (string-ci-hash obj))

(define (deprecated:hash-by-identity obj . rest)
  (issue-warning-deprecated 'hash-by-identity)
  (deprecated:hash obj))

(define (deprecated:hash-table-equivalence-function ht)
  (issue-warning-deprecated 'hash-table-equivalence-function)
  (hashtable-equivalence-function ht))

(define (deprecated:hash-table-hash-function ht)
  (issue-warning-deprecated 'hash-table-hash-function)
  (hashtable-hash-function ht))

(define (deprecated:hash-table-exists? ht key)
  (issue-warning-deprecated 'hash-table-exists?)
  (hash-table-contains? ht key))

(define (deprecated:hash-table-walk ht proc)
  (issue-warning-deprecated 'hash-table-walk)
  (hash-table-for-each proc ht))

(define (deprecated:hash-table-fold ht proc seed)
  (issue-warning-deprecated 'srfi-69-style:hash-table-fold)
  (hash-table-fold proc seed ht))

(define (deprecated:hash-table-merge! ht1 ht2)
  (issue-warning-deprecated 'hash-table-merge!)
  (hash-table-union! ht1 ht2))

))
