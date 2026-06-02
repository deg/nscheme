;;; (srfi 146 hash) — Hashmaps (SRFI 146, tangerine edition).
;;;
;;; Vendored from the SRFI 146 reference implementation by Marc
;;; Nieper-Wißkirchen, with the underlying persistent HAMT by Arthur
;;; A. Gleckler (MIT licence; texts preserved below). Flattened into a
;;; single file: the upstream library is split across
;;;
;;;   srfi/146/hash.sld          -> (include "hash.scm")
;;;   gleckler/hamt-map.sld      -> (include "hamt-map.scm")
;;;   gleckler/hamt.sld          -> (include "hamt.scm")
;;;   gleckler/hamt-misc.sld     -> (include "hamt-misc.scm")
;;;   gleckler/vector-edit.sld   -> (include "vector-edit.scm")
;;;
;;; nscheme resolves `include` relative to the process directory, not the
;;; library file, so each body is inlined here inside `begin`, in
;;; dependency order, instead of via `include`.
;;;
;;; Upstream imports replaced for nscheme:
;;;   (srfi 8)   receive            -> inlined (define-syntax receive ...)
;;;   (srfi 16)  case-lambda        -> (scheme case-lambda) built-in
;;;   (srfi 145) assume             -> inlined (define-syntax assume ...)
;;; The gleckler `assert` macro and `do-list` macro (from hamt-misc) are
;;; inlined too. Everything else maps to a library nscheme already has:
;;;   (srfi 1) (srfi 128) (srfi 143) (srfi 151).
;;; Upstream also imports (srfi 125) and (srfi 158); the only code that
;;; used them (hamt-misc's test-only hash-table helpers and the dead
;;; `tree-generator`) is dropped, so those two imports are dropped too.
;;;
;;; SPDX-License-Identifier: MIT
;;; Copyright (C) Marc Nieper-Wißkirchen (2016, 2018).
;;; Copyright (C) Arthur A. Gleckler (2004, 2015, 2021).
;;;
;;; Permission is hereby granted, free of charge, to any person
;;; obtaining a copy of this software and associated documentation files
;;; (the "Software"), to deal in the Software without restriction,
;;; including without limitation the rights to use, copy, modify, merge,
;;; publish, distribute, sublicense, and/or sell copies of the Software,
;;; and to permit persons to whom the Software is furnished to do so,
;;; subject to the following conditions:
;;;
;;; The above copyright notice and this permission notice shall be
;;; included in all copies or substantial portions of the Software.
;;;
;;; THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
;;; EXPRESS OR IMPLIED. See the SRFI 146 document for full text.

(define-library (srfi 146 hash)
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
          comparator?)
  ;; Upstream also imports (srfi 125) and (srfi 158), but the only code
  ;; that used them — hamt-misc's hash-table helpers (test-only) and the
  ;; dead `tree-generator` — is dropped below, so those imports are gone.
  (import (scheme base)
          (scheme case-lambda)
          (srfi 1)
          (srfi 128)
          (srfi 143)
          (srfi 151))

  ;; --- (srfi 8) receive ---------------------------------------------
  (begin
    (define-syntax receive
      (syntax-rules ()
        ((receive formals expression body ...)
         (call-with-values (lambda () expression)
           (lambda formals body ...))))))

  ;; --- (srfi 145) assume --------------------------------------------
  ;; Reduced to a checking form: if the expression is false, error.
  (begin
    (define-syntax assume
      (syntax-rules ()
        ((assume expression message ...)
         (or expression
             (error "assumption violated" 'expression message ...))))))

  ;; --- gleckler/hamt-misc.scm (assert / do-list) --------------------
  ;; string-comparator / make-string-hash-table / with-output-to-string
  ;; from hamt-misc are only used by the test harness, so they are
  ;; omitted; only the two macros that hamt.scm needs are inlined.
  (begin
    (define-syntax assert
      (syntax-rules ()
        ((_ (operator argument ...))
         (unless (operator argument ...)
           (error "Assertion failed:"
                  '(operator argument ...)
                  (list 'operator argument ...))))
        ((_ expression)
         (unless expression
           (error "Assertion failed:" 'expression)))))

    (define-syntax do-list
      (syntax-rules ()
        ((_ (variable list) body ...)
         (do ((remaining list (cdr remaining)))
             ((null? remaining))
           (let ((variable (car remaining)))
             body ...)))
        ((_ (element-variable index-variable list) body ...)
         (do ((remaining list (cdr remaining))
              (index-variable 0 (+ index-variable 1)))
             ((null? remaining))
           (let ((element-variable (car remaining)))
             body ...))))))

  ;; --- gleckler/vector-edit.scm -------------------------------------
  (begin
    (define (vector-without v start end)
      (let* ((size (vector-length v))
             (gap-size (- end start))
             (new-size (- size gap-size))
             (result (make-vector new-size)))
        (vector-copy! result 0 v 0 start)
        (vector-copy! result start v end size)
        result))

    (define (vector-replace-one v i e)
      (let ((result (vector-copy v)))
        (vector-set! result i e)
        result))

    (define-syntax vector-edit-total-skew
      (syntax-rules (add drop)
        ((_ s) s)
        ((_ s (add i e) . rest)
         (vector-edit-total-skew (+ s 1) . rest))
        ((_ s (drop i c) . rest)
         (vector-edit-total-skew (- s c) . rest))))

    (define-syntax vector-edit-code
      (syntax-rules (add drop)
        ((_ v r o s)
         (let ((index (vector-length v)))
           (vector-copy! r (+ o s) v o index)
           r))
        ((_ v r o s (add i e) . rest)
         (let ((index i))
           (vector-copy! r (+ o s) v o index)
           (vector-set! r (+ s index) e)
           (let ((skew (+ s 1)))
             (vector-edit-code v r index skew . rest))))
        ((_ v r o s (drop i c) . rest)
         (let ((index i))
           (vector-copy! r (+ o s) v o index)
           (let* ((dropped c)
                  (offset (+ index dropped))
                  (skew (- s dropped)))
             (vector-edit-code v r offset skew . rest))))))

    (define-syntax vector-edit
      (syntax-rules ()
        ((_ v . rest)
         (let ((result (make-vector (+ (vector-length v)
                                       (vector-edit-total-skew 0 . rest)))))
           (vector-edit-code v result 0 0 . rest))))))

  ;; --- gleckler/hamt.scm --------------------------------------------
  (begin
    (define hamt-hash-slice-size 5)
    (define hamt-hash-size
      (let ((word-size fx-width))
        (- word-size
           (remainder word-size hamt-hash-slice-size))))
    (define hamt-hash-modulus (expt 2 hamt-hash-size))
    (define hamt-bucket-size (expt 2 hamt-hash-slice-size))
    (define hamt-null (cons 'hamt 'null))

    (define-record-type hash-array-mapped-trie
        (%make-hamt = count hash mutable? payload? root)
        hash-array-mapped-trie?
      (=        hamt/=)
      (count    hamt/count set-hamt/count!)
      (hash     hamt/hash)
      (mutable? hamt/mutable?)
      (payload? hamt/payload?)
      (root     hamt/root  set-hamt/root!))

    (define (make-hamt = hash payload?)
      (%make-hamt = 0 hash #f payload? (make-empty-narrow)))

    (define-record-type collision
        (make-collision entries hash)
        collision?
      (entries collision/entries)
      (hash  collision/hash))

    (define-record-type narrow
        (make-narrow array children leaves)
        narrow?
      (array    narrow/array)
      (children narrow/children)
      (leaves   narrow/leaves))

    (define-record-type wide
        (make-wide array children leaves)
        wide?
      (array    wide/array)
      (children wide/children set-wide/children!)
      (leaves   wide/leaves   set-wide/leaves!))

    (define (hamt/empty? hamt)
      (zero? (hamt/count hamt)))

    (define (hamt/immutable-inner hamt replace)
      (if (hamt/mutable? hamt)
          (let ((payload? (hamt/payload? hamt)))
            (%make-hamt (hamt/= hamt)
                        (hamt/count hamt)
                        (hamt/hash hamt)
                        #f
                        payload?
                        (->immutable (hamt/root hamt) payload? replace)))
          hamt))

    (define hamt/immutable
      (case-lambda
       ((hamt) (hamt/immutable-inner hamt (lambda (k d) d)))
       ((hamt replace) (hamt/immutable-inner hamt replace))))

    (define (hamt/mutable hamt)
      (if (hamt/mutable? hamt)
          hamt
          (%make-hamt (hamt/= hamt)
                      (hamt/count hamt)
                      (hamt/hash hamt)
                      #t
                      (hamt/payload? hamt)
                      (hamt/root hamt))))

    (define (hamt/replace hamt key dp)
      (assert (not (hamt/mutable? hamt)))
      (let*-values (((payload?) (hamt/payload? hamt))
                    ((root) (hamt/root hamt))
                    ((==) (hamt/= hamt))
                    ((hp) (hamt/hash hamt))
                    ((hash) (hash-bits hp key))
                    ((change node) (modify-pure hamt root 0 dp hash key)))
        (if (eq? node root)
            hamt
            (let ((count (+ (hamt/count hamt) change)))
              (%make-hamt == count hp #f payload? node)))))

    (define (hamt/put hamt key datum)
      (hamt/replace hamt key (lambda (x) datum)))

    (define (hamt/replace! hamt key dp)
      (assert (hamt/mutable? hamt))
      (let*-values (((root) (hamt/root hamt))
                    ((hp) (hamt/hash hamt))
                    ((hash) (hash-bits hp key))
                    ((change node) (mutate hamt root 0 dp hash key)))
        (unless (zero? change)
          (set-hamt/count! hamt (+ (hamt/count hamt) change)))
        (unless (eq? node root)
          (set-hamt/root! hamt node))
        hamt))

    (define (hamt/put! hamt key datum)
      (hamt/replace! hamt key (lambda (x) datum)))

    (define (make-empty-narrow)
      (make-narrow (vector) 0 0))

    (define (hamt-null? n)
      (eq? n hamt-null))

    (define (collision-single-leaf? n)
      (let ((elements (collision/entries n)))
        (and (not (null? elements))
             (null? (cdr elements)))))

    (define (narrow-single-leaf? n)
      (and (zero? (narrow/children n))
           (= 1 (bit-count (narrow/leaves n)))))

    (define (wide-single-leaf? n)
      (and (zero? (wide/children n))
           (= 1 (bit-count (wide/leaves n)))))

    (define (hash-bits hp key)
      (remainder (hp key) hamt-hash-modulus))

    (define (next-set-bit i start end)
      (let ((index (first-set-bit (bit-field i start end))))
        (and (not (= index -1))
             (+ index start))))

    (define (narrow->wide n payload?)
      (let* ((c (narrow/children n))
             (l (narrow/leaves n))
             (stride (leaf-stride payload?))
             (a-in (narrow/array n))
             (a-out (make-vector (* stride hamt-bucket-size))))
        (let next-leaf ((start 0) (count 0))
          (let ((i (next-set-bit l start hamt-bucket-size)))
            (when i
              (let ((j (* stride i)))
                (vector-set! a-out j (vector-ref a-in count))
                (when payload?
                  (vector-set! a-out (+ j 1) (vector-ref a-in (+ count 1)))))
              (next-leaf (+ i 1) (+ stride count)))))
        (let next-child ((start 0) (offset (* stride (bit-count l))))
          (let ((i (next-set-bit c start hamt-bucket-size)))
            (when i
              (vector-set! a-out (* stride i) (vector-ref a-in offset))
              (next-child (+ i 1) (+ offset 1)))))
        (make-wide a-out c l)))

    (define (->immutable n payload? replace)
      (cond ((collision? n) n)
            ((narrow? n) n)
            ((wide? n)
             (let* ((c (wide/children n))
                    (l (wide/leaves n))
                    (stride (leaf-stride payload?))
                    (l-count (bit-count l))
                    (a-in (wide/array n))
                    (a-out (make-vector
                            (+ (* stride l-count) (bit-count c)))))
               (let next-leaf ((start 0) (count 0))
                 (let ((i (next-set-bit l
                                       start
                                       hamt-bucket-size)))
                   (when i
                     (let* ((j (* stride i))
                            (key (vector-ref a-in j)))
                       (vector-set! a-out count key)
                       (when payload?
                         (vector-set! a-out
                                      (+ count 1)
                                      (replace
                                       key
                                       (vector-ref a-in (+ j 1))))))
                     (next-leaf (+ i 1) (+ stride count)))))
               (let next-child ((start 0) (offset (* stride l-count)))
                 (let ((i (next-set-bit c
                                        start
                                        hamt-bucket-size)))
                   (when i
                     (vector-set! a-out
                                  offset
                                  (->immutable (vector-ref a-in (* stride i))
                                              payload?
                                              replace))
                     (next-child (+ i 1) (+ offset 1)))))
               (make-narrow a-out c l)))
            (else (error "Unexpected type of node."))))

    (define (hash-fragment shift hash)
      (bit-field hash shift (+ shift hamt-hash-slice-size)))

    (define (fragment->mask fragment)
      (- (expt 2 fragment) 1))

    (define (mutate hamt n shift dp h k)
      (cond ((collision? n) (modify-collision hamt n shift dp h k))
            ((narrow? n)
             (modify-wide hamt
                          (narrow->wide n (hamt/payload? hamt))
                          shift
                          dp
                          h
                          k))
            ((wide? n) (modify-wide hamt n shift dp h k))
            (else (error "Unknown HAMT node type." n))))

    (define (modify-wide hamt n shift dp h k)
      (let ((fragment (hash-fragment shift h)))
        (cond ((bit-set? fragment (wide/children n))
               (modify-wide-child hamt n shift dp h k))
              ((bit-set? fragment (wide/leaves n))
               (modify-wide-leaf hamt n shift dp h k))
              (else
               (let ((d (dp hamt-null)))
                 (if (hamt-null? d)
                     (values 0 n)
                     (modify-wide-new hamt n shift d h k)))))))

    (define (modify-wide-child hamt n shift dp h k)
      (let*-values (((fragment) (hash-fragment shift h))
                    ((array) (wide/array n))
                    ((payload?) (hamt/payload? hamt))
                    ((stride) (leaf-stride payload?))
                    ((i) (* stride fragment))
                    ((child) (vector-ref array i))
                    ((change new-child)
                     (mutate hamt
                             child
                             (+ shift hamt-hash-slice-size)
                             dp
                             h
                             k)))
        (define (coalesce key datum)
          (vector-set! array i key)
          (when payload?
            (vector-set! array (+ i 1) datum))
          (set-wide/children! n (copy-bit fragment (wide/children n) #f))
          (set-wide/leaves! n (copy-bit fragment (wide/leaves n) #t))
          (values change n))
        (define (replace)
          (vector-set! array i new-child)
          (values change n))
        (cond ((eq? new-child child) (values change n))
              ((hamt-null? new-child)
               (error "Child cannot become null." n))
              ((collision? new-child)
               (if (collision-single-leaf? new-child)
                   (let ((a (car (collision/entries new-child))))
                     (if payload?
                         (coalesce (car a) (cdr a))
                         (coalesce a #f)))
                   (replace)))
              ((wide? new-child)
               (if (wide-single-leaf? new-child)
                   (let ((a (wide/array new-child))
                         (j (* stride (next-set-bit (wide/leaves new-child)
                                                    0
                                                    hamt-bucket-size))))
                     (coalesce (vector-ref a j)
                               (and payload? (vector-ref a (+ j 1)))))
                   (replace)))
              ((narrow? new-child)
               (replace))
              (else (error "Unexpected type of child node.")))))

    (define (modify-wide-leaf hamt n shift dp h k)
      (let* ((fragment (hash-fragment shift h))
             (array (wide/array n))
             (payload? (hamt/payload? hamt))
             (stride (leaf-stride payload?))
             (i (* stride fragment))
             (key (vector-ref array i)))
        (if ((hamt/= hamt) k key)
            (let* ((existing (if payload? (vector-ref array (+ i 1)) hamt-null))
                   (d (dp existing)))
              (cond ((hamt-null? d)
                     (vector-set! array i #f)
                     (when payload? (vector-set! array (+ i 1) #f))
                     (set-wide/leaves! n (copy-bit fragment (wide/leaves n) #f))
                     (values -1 n))
                    (else
                     (when payload? (vector-set! array (+ i 1) d))
                     (values 0 n))))
            (let ((d (dp hamt-null)))
              (if (hamt-null? d)
                  (values 0 n)
                  (add-wide-leaf-key hamt n shift d h k))))))

    (define (add-wide-leaf-key hamt n shift d h k)
      (define payload? (hamt/payload? hamt))
      (define make-entry
        (if payload? cons (lambda (k d) k)))
      (let* ((fragment (hash-fragment shift h))
             (array (wide/array n))
             (stride (leaf-stride payload?))
             (i (* stride fragment))
             (key (vector-ref array i))
             (hash (hash-bits (hamt/hash hamt) key))
             (datum (and payload? (vector-ref array (+ i 1)))))
        (vector-set! array
                     i
                     (if (= h hash)
                         (make-collision (list (make-entry k d)
                                               (make-entry key datum))
                                         h)
                         (make-wide-with-two-keys
                          payload?
                          (+ shift hamt-hash-slice-size)
                          h
                          k
                          d
                          hash
                          key
                          datum)))
        (when payload?
          (vector-set! array (+ i 1) #f))
        (set-wide/children! n (copy-bit fragment (wide/children n) #t))
        (set-wide/leaves! n (copy-bit fragment (wide/leaves n) #f))
        (values 1 n)))

    (define (modify-wide-new hamt n shift d h k)
      (let* ((fragment (hash-fragment shift h))
             (array (wide/array n))
             (payload? (hamt/payload? hamt))
             (stride (leaf-stride payload?))
             (i (* stride fragment)))
        (vector-set! array i k)
        (when payload?
          (vector-set! array (+ i 1) d))
        (set-wide/leaves! n (copy-bit fragment (wide/leaves n) #t))
        (values 1 n)))

    (define (make-narrow-with-two-keys payload? shift h1 k1 d1 h2 k2 d2)
      (define (two-leaves f1 k1 d1 f2 k2 d2)
        (make-narrow
         (if payload?
             (vector k1 d1 k2 d2)
             (vector k1 k2))
         0
         (copy-bit f2 (copy-bit f1 0 #t) #t)))
      (assert (not (= h1 h2)))
      (let ((f1 (hash-fragment shift h1))
            (f2 (hash-fragment shift h2)))
        (cond ((= f1 f2)
               (make-narrow
                (vector (make-narrow-with-two-keys payload?
                                                   (+ shift hamt-hash-slice-size)
                                                   h1
                                                   k1
                                                   d1
                                                   h2
                                                   k2
                                                   d2))
                (copy-bit f1 0 #t)
                0))
              ((< f1 f2)
               (two-leaves f1 k1 d1 f2 k2 d2))
              (else
               (two-leaves f2 k2 d2 f1 k1 d1)))))

    (define (make-wide-with-two-keys payload? shift h1 k1 d1 h2 k2 d2)
      (assert (not (= h1 h2)))
      (let* ((stride (leaf-stride payload?))
             (f1 (hash-fragment shift h1))
             (f2 (hash-fragment shift h2))
             (array (make-vector (* stride hamt-bucket-size))))
        (cond ((= f1 f2)
               (vector-set! array
                            (* stride f1)
                            (make-wide-with-two-keys payload?
                                                     (+ shift hamt-hash-slice-size)
                                                     h1
                                                     k1
                                                     d1
                                                     h2
                                                     k2
                                                     d2))
               (make-wide array (copy-bit f1 0 #true) 0))
              (else (let* ((i1 (* stride f1))
                           (i2 (* stride f2)))
                      (vector-set! array i1 k1)
                      (vector-set! array i2 k2)
                      (when payload?
                        (vector-set! array (+ i1 1) d1)
                        (vector-set! array (+ i2 1) d2))
                      (make-wide array
                                 0
                                 (copy-bit f2 (copy-bit f1 0 #true) #true)))))))

    (define (modify-pure hamt n shift dp h k)
      (cond ((collision? n) (modify-collision hamt n shift dp h k))
            ((narrow? n) (modify-narrow hamt n shift dp h k))
            ((wide? n) (error "Should have been converted to narrow before here."))
            (else (error "Unknown HAMT node type." n))))

    (define (lower-collision hamt n shift dp h k)
      (let ((collision-hash (collision/hash n))
            (d (dp hamt-null)))
        (if (hamt-null? d)
            (values 0 n)
            (values
             1
             (let descend ((shift shift))
               (let ((collision-fragment (hash-fragment shift collision-hash))
                     (leaf-fragment (hash-fragment shift h)))
                 (if (= collision-fragment leaf-fragment)
                     (let ((child (descend (+ shift hamt-hash-slice-size))))
                       (make-narrow
                        (vector child)
                        (copy-bit collision-fragment 0 #t)
                        0))
                     (make-narrow
                      (if (hamt/payload? hamt)
                          (vector k d n)
                          (vector k n))
                      (copy-bit collision-fragment 0 #t)
                      (copy-bit leaf-fragment 0 #t)))))))))

    (define (modify-collision hamt n shift dp h k)
      (if (= h (collision/hash n))
          (let ((payload? (hamt/payload? hamt)))
            (let next ((entries (collision/entries n))
                       (checked '()))
              (if (null? entries)
                  (let ((d (dp hamt-null)))
                    (if (hamt-null? d)
                        (values 0 n)
                        (values 1
                                (make-collision (if payload?
                                                    (cons (cons k d) checked)
                                                    (cons k checked))
                                                h))))
                  (let* ((entry (car entries))
                         (key (if payload? (car entry) entry)))
                    (if ((hamt/= hamt) k key)
                        (let* ((existing (if payload? (cdr entry) hamt-null))
                               (d (dp existing))
                               (delete? (hamt-null? d))
                               (others (append checked (cdr entries))))
                          (values
                           (if delete? -1 0)
                           (make-collision (cond (delete? others)
                                                 (payload? (cons (cons k d) others))
                                                 (else (cons k others)))
                                           h)))
                        (next (cdr entries)
                              (cons (car entries) checked)))))))
          (lower-collision hamt n shift dp h k)))

    (define (leaf-stride payload?)
      (if payload? 2 1))

    (define (narrow-child-index l c mask payload?)
      (+ (* (leaf-stride payload?) (bit-count l))
         (bit-count (bitwise-and c mask))))

    (define (narrow-leaf-index l mask payload?)
      (* (leaf-stride payload?) (bit-count (bitwise-and l mask))))

    (define (modify-narrow hamt n shift dp h k)
      (let ((fragment (hash-fragment shift h)))
        (cond ((bit-set? fragment (narrow/children n))
               (modify-narrow-child hamt n shift dp h k))
              ((bit-set? fragment (narrow/leaves n))
               (modify-narrow-leaf hamt n shift dp h k))
              (else
               (let ((d (dp hamt-null)))
                 (if (hamt-null? d)
                     (values 0 n)
                     (modify-narrow-new hamt n shift d h k)))))))

    (define (modify-narrow-child hamt n shift dp h k)
      (let*-values (((fragment) (hash-fragment shift h))
                    ((mask) (fragment->mask fragment))
                    ((c) (narrow/children n))
                    ((l) (narrow/leaves n))
                    ((array) (narrow/array n))
                    ((payload?) (hamt/payload? hamt))
                    ((child-index)
                     (narrow-child-index l c mask payload?))
                    ((child) (vector-ref array child-index))
                    ((change new-child)
                     (modify-pure hamt
                                  child
                                  (+ shift hamt-hash-slice-size)
                                  dp
                                  h
                                  k)))
        (define (coalesce key datum)
          (let ((leaf-index (narrow-leaf-index l mask payload?)))
            (values change
                    (make-narrow (if payload?
                                     (vector-edit array
                                                  (add leaf-index key)
                                                  (add leaf-index datum)
                                                  (drop child-index 1))
                                     (vector-edit array
                                                  (add leaf-index key)
                                                  (drop child-index 1)))
                                 (copy-bit fragment c #f)
                                 (copy-bit fragment l #t)))))
        (define (replace)
          (values change
                  (make-narrow (vector-replace-one array child-index new-child)
                               c
                               l)))
        (cond ((eq? new-child child) (values 0 n))
              ((hamt-null? new-child)
               (error "Child cannot become null." n))
              ((collision? new-child)
               (if (collision-single-leaf? new-child)
                   (let ((a (car (collision/entries new-child))))
                     (if payload?
                         (coalesce (car a) (cdr a))
                         (coalesce a #f)))
                   (replace)))
              ((narrow? new-child)
               (if (narrow-single-leaf? new-child)
                   (let ((a (narrow/array new-child)))
                     (coalesce (vector-ref a 0)
                               (and payload? (vector-ref a 1))))
                   (replace)))
              ((wide? new-child)
               (error "New child should be collision or narrow."))
              (else (error "Unexpected type of child node.")))))

    (define (modify-narrow-leaf hamt n shift dp h k)
      (let* ((fragment (hash-fragment shift h))
             (mask (fragment->mask fragment))
             (c (narrow/children n))
             (l (narrow/leaves n))
             (array (narrow/array n))
             (payload? (hamt/payload? hamt))
             (stride (leaf-stride payload?))
             (leaf-index (narrow-leaf-index l mask payload?))
             (key (vector-ref array leaf-index)))
        (if ((hamt/= hamt) k key)
            (let* ((existing (if payload?
                                 (vector-ref array (+ leaf-index 1))
                                 hamt-null))
                   (d (dp existing)))
              (cond ((hamt-null? d)
                     (values -1
                             (make-narrow (vector-without array
                                                          leaf-index
                                                          (+ leaf-index stride))
                                          c
                                          (copy-bit fragment l #f))))
                    (payload?
                     (values
                      0
                      (make-narrow (vector-replace-one array (+ leaf-index 1) d)
                                   c
                                   l)))
                    (else (values 0 n))))
            (let ((d (dp hamt-null)))
              (if (hamt-null? d)
                  (values 0 n)
                  (add-narrow-leaf-key hamt n shift d h k))))))

    (define (add-narrow-leaf-key hamt n shift d h k)
      (define payload? (hamt/payload? hamt))
      (define make-entry
        (if payload? cons (lambda (k d) k)))
      (let* ((fragment (hash-fragment shift h))
             (mask (fragment->mask fragment))
             (c (narrow/children n))
             (l (narrow/leaves n))
             (array (narrow/array n))
             (payload? (hamt/payload? hamt))
             (stride (leaf-stride payload?))
             (leaf-index (narrow-leaf-index l mask payload?))
             (key (vector-ref array leaf-index))
             (child-index (narrow-child-index l c mask payload?))
             (hash (hash-bits (hamt/hash hamt) key))
             (datum (and payload? (vector-ref array (+ leaf-index 1)))))
        (values 1
                (make-narrow (if (= h hash)
                                 (vector-edit
                                  array
                                  (drop leaf-index stride)
                                  (add child-index
                                       (make-collision (list (make-entry k d)
                                                             (make-entry key datum))
                                                       h)))
                                 (vector-edit
                                  array
                                  (drop leaf-index stride)
                                  (add child-index
                                       (make-narrow-with-two-keys
                                        payload?
                                        (+ shift hamt-hash-slice-size)
                                        h
                                        k
                                        d
                                        hash
                                        key
                                        datum))))
                             (copy-bit fragment c #t)
                             (copy-bit fragment l #f)))))

    (define (modify-narrow-new hamt n shift d h k)
      (let* ((fragment (hash-fragment shift h))
             (mask (fragment->mask fragment))
             (c (narrow/children n))
             (l (narrow/leaves n))
             (array (narrow/array n))
             (payload? (hamt/payload? hamt))
             (leaf-index (narrow-leaf-index l mask payload?))
             (delete? (hamt-null? d)))
        (values 1
                (make-narrow (if payload?
                                 (vector-edit array
                                              (add leaf-index k)
                                              (add leaf-index d))
                                 (vector-edit array
                                              (add leaf-index k)))
                             c
                             (copy-bit fragment l #t)))))

    (define (hamt-fetch hamt key)
      (let ((h (hash-bits (hamt/hash hamt) key))
            (payload? (hamt/payload? hamt)))
        (let descend ((n (hamt/root hamt))
                      (shift 0))
          (cond ((collision? n)
                 (let ((entries (collision/entries n))
                       (key= (hamt/= hamt)))
                   (if payload?
                       (cond ((assoc key entries key=) => cdr)
                             (else hamt-null))
                       (if (find-tail (lambda (e) (key= key e)) entries)
                           'present
                           hamt-null))))
                ((narrow? n)
                 (let ((array (narrow/array n))
                       (c (narrow/children n))
                       (l (narrow/leaves n))
                       (fragment (hash-fragment shift h)))
                   (cond ((bit-set? fragment c)
                          (let* ((mask (fragment->mask fragment))
                                 (child-index (narrow-child-index
                                               l
                                               c
                                               mask
                                               (hamt/payload? hamt))))
                            (descend (vector-ref array child-index)
                                     (+ shift hamt-hash-slice-size))))
                         ((bit-set? fragment l)
                          (let* ((mask (fragment->mask fragment))
                                 (leaf-index
                                  (narrow-leaf-index l mask (hamt/payload? hamt)))
                                 (k (vector-ref array leaf-index)))
                            (if ((hamt/= hamt) k key)
                                (if payload?
                                    (vector-ref array (+ leaf-index 1))
                                    'present)
                                hamt-null)))
                         (else hamt-null))))
                ((wide? n)
                 (let ((array (wide/array n))
                       (stride (leaf-stride (hamt/payload? hamt)))
                       (c (wide/children n))
                       (l (wide/leaves n))
                       (i (hash-fragment shift h)))
                   (cond ((bit-set? i c)
                          (descend (vector-ref array (* stride i))
                                   (+ shift hamt-hash-slice-size)))
                         ((bit-set? i l)
                          (let* ((j (* stride i))
                                 (k (vector-ref array j)))
                            (if ((hamt/= hamt) k key)
                                (if payload?
                                    (vector-ref array (+ j 1))
                                    'present)
                                hamt-null)))
                         (else hamt-null))))
                (else (error "Unexpected type of child node."))))))

    (define (collision/for-each procedure node payload?)
      (if payload?
          (do-list (e (collision/entries node))
            (procedure (car e) (cdr e)))
          (do-list (e (collision/entries node))
            (procedure e #f))))

    (define (narrow/for-each procedure node payload?)
      (let ((array (narrow/array node))
            (stride (leaf-stride payload?))
            (c (narrow/children node))
            (l (narrow/leaves node)))
        (let next-leaf ((count 0)
                        (start 0))
          (let ((i (next-set-bit l start hamt-bucket-size)))
            (if i
                (let* ((j (* stride count))
                       (k (vector-ref array j))
                       (d (and payload? (vector-ref array (+ j 1)))))
                  (procedure k d)
                  (next-leaf (+ count 1) (+ i 1)))
                (let next-child ((start 0)
                                 (offset (* stride count)))
                  (let ((i (next-set-bit c start hamt-bucket-size)))
                    (when i
                      (let ((child (vector-ref array offset)))
                        (hamt-node/for-each child payload? procedure)
                        (next-child (+ i 1) (+ offset 1)))))))))))

    (define (wide/for-each procedure node payload?)
      (let ((array (wide/array node))
            (stride (leaf-stride payload?))
            (c (wide/children node))
            (l (wide/leaves node)))
        (do ((i 0 (+ i 1)))
            ((= i hamt-bucket-size))
          (let ((j (* stride i)))
            (cond ((bit-set? i l)
                   (let ((k (vector-ref array j))
                         (d (and payload? (vector-ref array (+ j 1)))))
                     (procedure k d)))
                  ((bit-set? i c)
                   (let ((child (vector-ref array j)))
                     (hamt-node/for-each child payload? procedure))))))))

    (define (hamt-node/for-each node payload? procedure)
      (cond ((collision? node) (collision/for-each procedure node payload?))
            ((narrow? node) (narrow/for-each procedure node payload?))
            ((wide? node) (wide/for-each procedure node payload?))
            (else (error "Invalid type of node." node))))

    (define (hamt/for-each procedure hamt)
      (hamt-node/for-each (hamt/root hamt)
                          (hamt/payload? hamt)
                          procedure))

    (define (hamt->list hamt procedure)
      (let ((accumulator '()))
        (hamt/for-each (lambda (k v)
                         (set! accumulator
                               (cons (procedure k v)
                                     accumulator)))
                       hamt)
        accumulator)))

  ;; --- gleckler/hamt-map.scm ----------------------------------------
  (begin
    (define (phm? datum)
      (and (hash-array-mapped-trie? datum)
           (hamt/payload? datum)))

    (define (make-phm-inner hash = alist)
      (let ((phm (make-hamt = hash #t)))
        (if (null? alist)
            phm
            (let ((phm-1 (phm/mutable phm)))
              (phm/add-alist! phm-1 alist)
              (phm/immutable phm-1)))))

    (define make-phm
      (case-lambda
       ((hash =) (make-phm-inner hash = '()))
       ((hash = alist) (make-phm-inner hash = alist))))

    (define (phm/count phm)
      (assert (phm? phm))
      (hamt/count phm))

    (define (phm/empty? phm)
      (assert (phm? phm))
      (hamt/empty? phm))

    (define phm/immutable
      (case-lambda
       ((phm)
        (assert (phm? phm))
        (hamt/immutable phm))
       ((phm replace)
        (assert (phm? phm))
        (hamt/immutable phm replace))))

    (define (phm/mutable phm)
      (assert (phm? phm))
      (hamt/mutable phm))

    (define (phm/mutable? phm)
      (assert (phm? phm))
      (hamt/mutable? phm))

    (define (phm/put phm key datum)
      (assert (phm? phm))
      (hamt/put phm key datum))

    (define (phm/put! phm key datum)
      (assert (phm? phm))
      (hamt/put! phm key datum))

    (define (phm/replace phm key replace)
      (assert (phm? phm))
      (hamt/replace phm key replace))

    (define (phm/replace! phm key replace)
      (assert (phm? phm))
      (hamt/replace! phm key replace))

    (define (phm/get-inner phm key default)
      (assert (phm? phm))
      (let ((result (hamt-fetch phm key)))
        (if (hamt-null? result)
            default
            result)))

    (define phm/get
      (case-lambda
       ((phm key) (phm/get-inner phm key #f))
       ((phm key default) (phm/get-inner phm key default))))

    (define (phm/contains? phm key)
      (assert (phm? phm))
      (not (hamt-null? (hamt-fetch phm key))))

    (define (phm/remove phm key)
      (assert (phm? phm))
      (phm/put phm key hamt-null))

    (define (phm/remove! phm key)
      (assert (phm? phm))
      (assert (hamt/mutable? phm))
      (phm/put! phm key hamt-null))

    (define (phm/add-alist phm alist)
      (assert (phm? phm))
      (fold (lambda (a phm) (phm/put phm (car a) (cdr a))) phm alist))

    (define (phm/add-alist! phm alist)
      (assert (phm? phm))
      (do-list (a alist)
        (phm/put! phm (car a) (cdr a)))
      phm)

    (define (phm->alist phm)
      (assert (phm? phm))
      (hamt->list phm cons))

    (define (phm/data phm)
      (assert (phm? phm))
      (hamt->list phm (lambda (k d) d)))

    (define (phm/keys phm)
      (assert (phm? phm))
      (hamt->list phm (lambda (k d) k)))

    (define (phm/for-each procedure phm)
      (assert (phm? phm))
      (hamt/for-each procedure phm)))

  ;; --- srfi/146/hash.scm --------------------------------------------
  (begin
    ;;; Implementation layer

    (define (tree-search comparator tree obj failure success)
      (let ((entry (phm/get tree obj)))
        (if entry
            (success (car entry) (cdr entry)
                     (lambda (new-key new-datum ret)
                       (let ((tree (phm/remove tree obj)))
                         (values (phm/put tree new-key (cons new-key new-datum))
                                 ret)))
                     (lambda (ret)
                       (values (phm/remove tree obj) ret)))
            (failure (lambda (new-key new-datum ret)
                       (values (phm/put tree new-key (cons new-key new-datum))
                               ret))
                     (lambda (ret)
                       (values tree ret))))))

    (define (tree-fold proc seed tree)
      (phm/for-each (lambda (key entry)
                      (set! seed (proc (car entry) (cdr entry) seed)))
                    tree)
      seed)

    (define (tree-for-each proc tree)
      (phm/for-each (lambda (key entry)
                      (proc (car entry) (cdr entry)))
                    tree))

    ;; Upstream defines `tree-generator` here via (srfi 158)
    ;; make-coroutine-generator, but it is unused by every exported
    ;; procedure, so it is omitted along with the (srfi 158) import.

    ;;; New types

    (define-record-type <hashmap>
      (%make-hashmap comparator tree)
      hashmap?
      (comparator hashmap-key-comparator)
      (tree hashmap-tree))

    (define (make-empty-hashmap comparator)
      (assume (comparator? comparator))
      (%make-hashmap comparator
                     (make-phm (comparator-hash-function comparator)
                               (comparator-equality-predicate comparator))))

    ;;; Exported procedures

    ;; Constructors

    (define (hashmap comparator . args)
      (assume (comparator? comparator))
      (hashmap-unfold null?
                  (lambda (args)
                    (values (car args)
                            (cadr args)))
                  cddr
                  args
                  comparator))

    (define (hashmap-unfold stop? mapper successor seed comparator)
      (assume (procedure? stop?))
      (assume (procedure? mapper))
      (assume (procedure? successor))
      (assume (comparator? comparator))
      (let loop ((hashmap (make-empty-hashmap comparator))
                 (seed seed))
        (if (stop? seed)
            hashmap
            (receive (key value)
                (mapper seed)
              (loop (hashmap-adjoin hashmap key value)
                    (successor seed))))))

    ;; Predicates

    (define (hashmap-empty? hashmap)
      (assume (hashmap? hashmap))
      (not (hashmap-any? (lambda (key value) #t) hashmap)))

    (define (hashmap-contains? hashmap key)
      (assume (hashmap? hashmap))
      (call/cc
       (lambda (return)
         (hashmap-search hashmap
                     key
                     (lambda (insert ignore)
                       (return #f))
                     (lambda (key value update remove)
                       (return #t))))))

    (define (hashmap-disjoint? hashmap1 hashmap2)
      (assume (hashmap? hashmap1))
      (assume (hashmap? hashmap2))
      (call/cc
       (lambda (return)
         (hashmap-for-each (lambda (key value)
                         (when (hashmap-contains? hashmap2 key)
                           (return #f)))
                       hashmap1)
         #t)))

    ;; Accessors

    (define hashmap-ref
      (case-lambda
        ((hashmap key)
         (assume (hashmap? hashmap))
         (hashmap-ref hashmap key (lambda ()
                            (error "hashmap-ref: key not in hashmap" key))))
        ((hashmap key failure)
         (assume (hashmap? hashmap))
         (assume (procedure? failure))
         (hashmap-ref hashmap key failure (lambda (value)
                                    value)))
        ((hashmap key failure success)
         (assume (hashmap? hashmap))
         (assume (procedure? failure))
         (assume (procedure? success))
         ((call/cc
           (lambda (return-thunk)
             (hashmap-search hashmap
                             key
                             (lambda (insert ignore)
                               (return-thunk failure))
                             (lambda (key value update remove)
                               (return-thunk (lambda () (success value)))))))))))

    (define (hashmap-ref/default hashmap key default)
      (assume (hashmap? hashmap))
      (hashmap-ref hashmap key (lambda () default)))

    ;; Updaters

    (define (hashmap-adjoin hashmap . args)
      (assume (hashmap? hashmap))
      (let loop ((args args)
                 (hashmap hashmap))
        (if (null? args)
            hashmap
            (receive (hashmap value)
                (hashmap-intern hashmap (car args) (lambda () (cadr args)))
              (loop (cddr args) hashmap)))))

    (define hashmap-adjoin! hashmap-adjoin)

    (define (hashmap-set hashmap . args)
      (assume (hashmap? hashmap))
      (let loop ((args args)
                 (hashmap hashmap))
        (if (null? args)
            hashmap
            (receive (hashmap)
                (hashmap-update hashmap (car args) (lambda (value) (cadr args)) (lambda () #f))
              (loop (cddr args)
                    hashmap)))))

    (define hashmap-set! hashmap-set)

    (define (hashmap-replace hashmap key value)
      (assume (hashmap? hashmap))
      (receive (hashmap obj)
          (hashmap-search hashmap
                      key
                      (lambda (insert ignore)
                        (ignore #f))
                      (lambda (old-key old-value update remove)
                        (update key value #f)))
        hashmap))

    (define hashmap-replace! hashmap-replace)

    (define (hashmap-delete hashmap . keys)
      (assume (hashmap? hashmap))
      (hashmap-delete-all hashmap keys))

    (define hashmap-delete! hashmap-delete)

    (define (hashmap-delete-all hashmap keys)
      (assume (hashmap? hashmap))
      (assume (list? keys))
      (fold (lambda (key hashmap)
              (receive (hashmap obj)
                  (hashmap-search hashmap
                              key
                              (lambda (insert ignore)
                                (ignore #f))
                              (lambda (old-key old-value update remove)
                                (remove #f)))
                hashmap))
            hashmap keys))

    (define hashmap-delete-all! hashmap-delete-all)

    (define (hashmap-intern hashmap key failure)
      (assume (hashmap? hashmap))
      (assume (procedure? failure))
      (call/cc
       (lambda (return)
         (hashmap-search hashmap
                     key
                     (lambda (insert ignore)
                       (receive (value)
                           (failure)
                         (insert value value)))
                     (lambda (old-key old-value update remove)
                       (return hashmap old-value))))))

    (define hashmap-intern! hashmap-intern)

    (define hashmap-update
      (case-lambda
       ((hashmap key updater)
        (hashmap-update hashmap key updater (lambda ()
                                      (error "hashmap-update: key not found in hashmap" key))))
       ((hashmap key updater failure)
        (hashmap-update hashmap key updater failure (lambda (value)
                                              value)))
       ((hashmap key updater failure success)
        (assume (hashmap? hashmap))
        (assume (procedure? updater))
        (assume (procedure? failure))
        (assume (procedure? success))
        (receive (hashmap obj)
            (hashmap-search hashmap
                        key
                        (lambda (insert ignore)
                          (insert (updater (failure)) #f))
                        (lambda (old-key old-value update remove)
                          (update key (updater (success old-value)) #f)))
          hashmap))))

    (define hashmap-update! hashmap-update)

    (define (hashmap-update/default hashmap key updater default)
      (hashmap-update hashmap key updater (lambda () default)))

    (define hashmap-update!/default hashmap-update/default)

    (define hashmap-pop
      (case-lambda
        ((hashmap)
         (hashmap-pop hashmap (lambda ()
                                (error "hashmap-pop: hashmap has no association"))))
        ((hashmap failure)
         (assume (hashmap? hashmap))
         (assume (procedure? failure))
         ((call/cc
           (lambda (return-thunk)
             (receive (key value)
                 (hashmap-find (lambda (key value) #t) hashmap (lambda () (return-thunk failure)))
               (lambda ()
                 (values (hashmap-delete hashmap key) key value)))))))))

    (define hashmap-pop! hashmap-pop)

    (define (hashmap-search hashmap key failure success)
      (assume (hashmap? hashmap))
      (assume (procedure? failure))
      (assume (procedure? success))
      (call/cc
       (lambda (return)
         (let*-values
             (((comparator)
               (hashmap-key-comparator hashmap))
              ((tree obj)
               (tree-search comparator
                            (hashmap-tree hashmap)
                            key
                            (lambda (insert ignore)
                              (failure (lambda (value obj)
                                         (insert key value obj))
                                       (lambda (obj)
                                         (return hashmap obj))))
                            success)))
           (values (%make-hashmap comparator tree)
                   obj)))))

    (define hashmap-search! hashmap-search)

    ;; The whole hashmap

    (define (hashmap-size hashmap)
      (assume (hashmap? hashmap))
      (hashmap-count (lambda (key value)
                   #t)
                 hashmap))

    (define (hashmap-find predicate hashmap failure)
      (assume (procedure? predicate))
      (assume (hashmap? hashmap))
      (assume (procedure? failure))
      (call/cc
       (lambda (return)
         (hashmap-for-each (lambda (key value)
                         (when (predicate key value)
                           (return key value)))
                       hashmap)
         (failure))))

    (define (hashmap-count predicate hashmap)
      (assume (procedure? predicate))
      (assume (hashmap? hashmap))
      (hashmap-fold (lambda (key value count)
                  (if (predicate key value)
                      (+ 1 count)
                      count))
                0 hashmap))

    (define (hashmap-any? predicate hashmap)
      (assume (procedure? predicate))
      (assume (hashmap? hashmap))
      (call/cc
       (lambda (return)
         (hashmap-for-each (lambda (key value)
                         (when (predicate key value)
                           (return #t)))
                       hashmap)
         #f)))

    (define (hashmap-every? predicate hashmap)
      (assume (procedure? predicate))
      (assume (hashmap? hashmap))
      (not (hashmap-any? (lambda (key value)
                       (not (predicate key value)))
                     hashmap)))

    (define (hashmap-keys hashmap)
      (assume (hashmap? hashmap))
      (hashmap-fold (lambda (key value keys)
                      (cons key keys))
                    '() hashmap))

    (define (hashmap-values hashmap)
      (assume (hashmap? hashmap))
      (hashmap-fold (lambda (key value values)
                      (cons value values))
                    '() hashmap))

    (define (hashmap-entries hashmap)
      (assume (hashmap? hashmap))
      (values (hashmap-keys hashmap)
              (hashmap-values hashmap)))

    ;; Hashmap and folding

    (define (hashmap-map proc comparator hashmap)
      (assume (procedure? proc))
      (assume (comparator? comparator))
      (assume (hashmap? hashmap))
      (hashmap-fold (lambda (key value hashmap)
                  (receive (key value)
                      (proc key value)
                    (hashmap-set hashmap key value)))
                (make-empty-hashmap comparator)
                hashmap))

    (define (hashmap-for-each proc hashmap)
      (assume (procedure? proc))
      (assume (hashmap? hashmap))
      (tree-for-each proc (hashmap-tree hashmap)))

    (define (hashmap-fold proc acc hashmap)
      (assume (procedure? proc))
      (assume (hashmap? hashmap))
      (tree-fold proc acc (hashmap-tree hashmap)))

    (define (hashmap-map->list proc hashmap)
      (assume (procedure? proc))
      (assume (hashmap? hashmap))
      (hashmap-fold (lambda (key value lst)
                      (cons (proc key value) lst))
                    '()
                    hashmap))

    (define (hashmap-filter predicate hashmap)
      (assume (procedure? predicate))
      (assume (hashmap? hashmap))
      (hashmap-fold (lambda (key value hashmap)
                  (if (predicate key value)
                      (hashmap-set hashmap key value)
                      hashmap))
                (make-empty-hashmap (hashmap-key-comparator hashmap))
                hashmap))

    (define hashmap-filter! hashmap-filter)

    (define (hashmap-remove predicate hashmap)
      (assume (procedure? predicate))
      (assume (hashmap? hashmap))
      (hashmap-filter (lambda (key value)
                    (not (predicate key value)))
                  hashmap))

    (define hashmap-remove! hashmap-remove)

    (define (hashmap-partition predicate hashmap)
      (assume (procedure? predicate))
      (assume (hashmap? hashmap))
      (values (hashmap-filter predicate hashmap)
              (hashmap-remove predicate hashmap)))

    (define hashmap-partition! hashmap-partition)

    ;; Copying and conversion

    (define (hashmap-copy hashmap)
      (assume (hashmap? hashmap))
      hashmap)

    (define (hashmap->alist hashmap)
      (assume (hashmap? hashmap))
      (hashmap-fold (lambda (key value alist)
                      (cons (cons key value) alist))
                    '() hashmap))

    (define (alist->hashmap comparator alist)
      (assume (comparator? comparator))
      (assume (list? alist))
      (hashmap-unfold null?
                  (lambda (alist)
                    (let ((key (caar alist))
                          (value (cdar alist)))
                      (values key value)))
                  cdr
                  alist
                  comparator))

    (define (alist->hashmap! hashmap alist)
      (assume (hashmap? hashmap))
      (assume (list? alist))
      (fold (lambda (association hashmap)
              (let ((key (car association))
                    (value (cdr association)))
                (hashmap-set hashmap key value)))
            hashmap
            alist))

    ;; Subhashmaps

    (define hashmap=?
      (case-lambda
        ((comparator hashmap)
         (assume (hashmap? hashmap))
         #t)
        ((comparator hashmap1 hashmap2) (%hashmap=? comparator hashmap1 hashmap2))
        ((comparator hashmap1 hashmap2 . hashmaps)
         (and (%hashmap=? comparator hashmap1 hashmap2)
              (apply hashmap=? comparator hashmap2 hashmaps)))))
    (define (%hashmap=? comparator hashmap1 hashmap2)
      (and (eq? (hashmap-key-comparator hashmap1) (hashmap-key-comparator hashmap2))
           (%hashmap<=? comparator hashmap1 hashmap2)
           (%hashmap<=? comparator hashmap2 hashmap1)))

    (define hashmap<=?
      (case-lambda
        ((comparator hashmap)
         (assume (hashmap? hashmap))
         #t)
        ((comparator hashmap1 hashmap2)
         (assume (comparator? comparator))
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (%hashmap<=? comparator hashmap1 hashmap2))
        ((comparator hashmap1 hashmap2 . hashmaps)
         (assume (comparator? comparator))
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (and (%hashmap<=? comparator hashmap1 hashmap2)
              (apply hashmap<=? comparator hashmap2 hashmaps)))))

    (define (%hashmap<=? comparator hashmap1 hashmap2)
      (assume (comparator? comparator))
      (assume (hashmap? hashmap1))
      (assume (hashmap? hashmap2))
      (hashmap-every? (lambda (key value)
                        (hashmap-ref hashmap2 key
                                     (lambda ()
                                       #f)
                                     (lambda (stored-value)
                                       (=? comparator value stored-value))))
                      hashmap1))

    (define hashmap>?
      (case-lambda
        ((comparator hashmap)
         (assume (hashmap? hashmap))
         #t)
        ((comparator hashmap1 hashmap2)
         (assume (comparator? comparator))
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (%hashmap>? comparator hashmap1 hashmap2))
        ((comparator hashmap1 hashmap2 . hashmaps)
         (assume (comparator? comparator))
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (and (%hashmap>? comparator  hashmap1 hashmap2)
              (apply hashmap>? comparator hashmap2 hashmaps)))))

    (define (%hashmap>? comparator hashmap1 hashmap2)
      (assume (comparator? comparator))
      (assume (hashmap? hashmap1))
      (assume (hashmap? hashmap2))
      (not (%hashmap<=? comparator hashmap1 hashmap2)))

    (define hashmap<?
      (case-lambda
        ((comparator hashmap)
         (assume (hashmap? hashmap))
         #t)
        ((comparator hashmap1 hashmap2)
         (assume (comparator? comparator))
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (%hashmap<? comparator hashmap1 hashmap2))
        ((comparator hashmap1 hashmap2 . hashmaps)
         (assume (comparator? comparator))
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (and (%hashmap<? comparator  hashmap1 hashmap2)
              (apply hashmap<? comparator hashmap2 hashmaps)))))

    (define (%hashmap<? comparator hashmap1 hashmap2)
         (assume (comparator? comparator))
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (%hashmap>? comparator hashmap2 hashmap1))

    (define hashmap>=?
      (case-lambda
        ((comparator hashmap)
         (assume (hashmap? hashmap))
         #t)
        ((comparator hashmap1 hashmap2)
         (assume (comparator? comparator))
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (%hashmap>=? comparator hashmap1 hashmap2))
        ((comparator hashmap1 hashmap2 . hashmaps)
         (assume (comparator? comparator))
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (and (%hashmap>=? comparator hashmap1 hashmap2)
              (apply hashmap>=? comparator hashmap2 hashmaps)))))

    (define (%hashmap>=? comparator hashmap1 hashmap2)
      (assume (comparator? comparator))
      (assume (hashmap? hashmap1))
      (assume (hashmap? hashmap2))
      (not (%hashmap<? comparator hashmap1 hashmap2)))

    ;; Set theory operations

    (define (%hashmap-union hashmap1 hashmap2)
      (hashmap-fold (lambda (key2 value2 hashmap)
                      (receive (hashmap obj)
                          (hashmap-search hashmap
                                          key2
                                          (lambda (insert ignore)
                                            (insert value2 #f))
                                          (lambda (key1 value1 update remove)
                                            (update key1 value1 #f)))
                        hashmap))
                    hashmap1 hashmap2))

    (define (%hashmap-intersection hashmap1 hashmap2)
      (hashmap-filter (lambda (key1 value1)
                    (hashmap-contains? hashmap2 key1))
                  hashmap1))

    (define (%hashmap-difference hashmap1 hashmap2)
      (hashmap-fold (lambda (key2 value2 hashmap)
                  (receive (hashmap obj)
                      (hashmap-search hashmap
                                  key2
                                  (lambda (insert ignore)
                                    (ignore #f))
                                  (lambda (key1 value1 update remove)
                                    (remove #f)))
                    hashmap))
                hashmap1 hashmap2))

    (define (%hashmap-xor hashmap1 hashmap2)
      (hashmap-fold (lambda (key2 value2 hashmap)
                  (receive (hashmap obj)
                      (hashmap-search hashmap
                                  key2
                                  (lambda (insert ignore)
                                    (insert value2 #f))
                                  (lambda (key1 value1 update remove)
                                    (remove #f)))
                    hashmap))
                hashmap1 hashmap2))

    (define hashmap-union
      (case-lambda
        ((hashmap)
         (assume (hashmap? hashmap))
         hashmap)
        ((hashmap1 hashmap2)
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (%hashmap-union hashmap1 hashmap2))
        ((hashmap1 hashmap2 . hashmaps)
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (apply hashmap-union (%hashmap-union hashmap1 hashmap2) hashmaps))))
    (define hashmap-union! hashmap-union)

    (define hashmap-intersection
      (case-lambda
        ((hashmap)
         (assume (hashmap? hashmap))
         hashmap)
        ((hashmap1 hashmap2)
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (%hashmap-intersection hashmap1 hashmap2))
        ((hashmap1 hashmap2 . hashmaps)
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (apply hashmap-intersection (%hashmap-intersection hashmap1 hashmap2) hashmaps))))
    (define hashmap-intersection! hashmap-intersection)

    (define hashmap-difference
      (case-lambda
        ((hashmap)
         (assume (hashmap? hashmap))
         hashmap)
        ((hashmap1 hashmap2)
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (%hashmap-difference hashmap1 hashmap2))
        ((hashmap1 hashmap2 . hashmaps)
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (apply hashmap-difference (%hashmap-difference hashmap1 hashmap2) hashmaps))))
    (define hashmap-difference! hashmap-difference)

    (define hashmap-xor
      (case-lambda
        ((hashmap)
         (assume (hashmap? hashmap))
         hashmap)
        ((hashmap1 hashmap2)
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (%hashmap-xor hashmap1 hashmap2))
        ((hashmap1 hashmap2 . hashmaps)
         (assume (hashmap? hashmap1))
         (assume (hashmap? hashmap2))
         (apply hashmap-xor (%hashmap-xor hashmap1 hashmap2) hashmaps))))
    (define hashmap-xor! hashmap-xor)

    ;; Comparators

    (define (hashmap-equality comparator)
      (assume (comparator? comparator))
      (lambda (hashmap1 hashmap2)
        (hashmap=? comparator hashmap1 hashmap2)))

    (define (hashmap-hash-function comparator)
      (assume (comparator? comparator))
      (lambda (hashmap)
        0))

    (define (make-hashmap-comparator comparator)
      (make-comparator hashmap?
                       (hashmap-equality comparator)
                       #f
                       (hashmap-hash-function comparator)))

    (define hashmap-comparator (make-hashmap-comparator (make-default-comparator)))

    (comparator-register-default! hashmap-comparator)))
