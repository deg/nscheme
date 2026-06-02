;;; (srfi 69) — Basic hash tables.
;;;
;;; A compact, self-contained implementation of SRFI 69 for nscheme,
;;; which has no native hash table. Backed by an association list held in
;;; a mutable record, so operations are O(n); this is adequate for the
;;; small caches its consumers use (e.g. SRFI 115 regex). The optional
;;; hash function is accepted and ignored — key comparison uses the
;;; equivalence predicate directly.
(define-library (srfi 69)
  (export make-hash-table hash-table? alist->hash-table
          hash-table-equivalence-function hash-table-hash-function
          hash-table-ref hash-table-ref/default hash-table-set!
          hash-table-delete! hash-table-exists? hash-table-update!
          hash-table-update!/default hash-table-size hash-table-keys
          hash-table-values hash-table->alist hash-table-walk
          hash-table-fold hash-table-copy hash-table-clear!
          hash-table-merge!
          hash hash-by-identity equal-hash string-hash string-ci-hash
          symbol-hash)
  (import (scheme base) (scheme char))
  (begin
    (define-record-type <srfi69-hash-table>
      (%make-ht equiv hashf alist)
      hash-table?
      (equiv ht-equiv)
      (hashf ht-hashf)
      (alist ht-alist ht-alist-set!))

    ;; (make-hash-table [equivalence [hash [size]]])
    (define (make-hash-table . args)
      (let ((equiv (if (pair? args) (car args) equal?))
            (hashf (if (and (pair? args) (pair? (cdr args))) (cadr args) #f)))
        (%make-ht equiv hashf '())))

    (define (hash-table-equivalence-function ht) (ht-equiv ht))
    (define (hash-table-hash-function ht) (ht-hashf ht))

    ;; Return the (key . value) cell for key, or #f.
    (define (ht-cell ht key)
      (let ((same? (ht-equiv ht)))
        (let loop ((p (ht-alist ht)))
          (cond ((null? p) #f)
                ((same? (caar p) key) (car p))
                (else (loop (cdr p)))))))

    (define (hash-table-set! ht key value)
      (let ((cell (ht-cell ht key)))
        (if cell
            (set-cdr! cell value)
            (ht-alist-set! ht (cons (cons key value) (ht-alist ht))))))

    (define (hash-table-ref/default ht key default)
      (let ((cell (ht-cell ht key)))
        (if cell (cdr cell) default)))

    (define (hash-table-ref ht key . thunk)
      (let ((cell (ht-cell ht key)))
        (cond (cell (cdr cell))
              ((pair? thunk) ((car thunk)))
              (else (error "hash-table-ref: key not found" key)))))

    (define (hash-table-exists? ht key)
      (if (ht-cell ht key) #t #f))

    (define (hash-table-delete! ht key)
      (let ((same? (ht-equiv ht)))
        (ht-alist-set!
         ht
         (let loop ((p (ht-alist ht)))
           (cond ((null? p) '())
                 ((same? (caar p) key) (cdr p))
                 (else (cons (car p) (loop (cdr p)))))))))

    (define (hash-table-update! ht key proc . thunk)
      (let ((cell (ht-cell ht key)))
        (if cell
            (set-cdr! cell (proc (cdr cell)))
            (hash-table-set!
             ht key
             (proc (if (pair? thunk)
                       ((car thunk))
                       (error "hash-table-update!: key not found" key)))))))

    (define (hash-table-update!/default ht key proc default)
      (hash-table-set! ht key (proc (hash-table-ref/default ht key default))))

    (define (hash-table-size ht) (length (ht-alist ht)))
    (define (hash-table-keys ht) (map car (ht-alist ht)))
    (define (hash-table-values ht) (map cdr (ht-alist ht)))
    (define (hash-table->alist ht)
      (map (lambda (p) (cons (car p) (cdr p))) (ht-alist ht)))

    (define (hash-table-walk ht proc)
      (for-each (lambda (p) (proc (car p) (cdr p))) (ht-alist ht)))

    (define (hash-table-fold ht proc seed)
      (let loop ((p (ht-alist ht)) (acc seed))
        (if (null? p) acc (loop (cdr p) (proc (caar p) (cdar p) acc)))))

    (define (hash-table-clear! ht) (ht-alist-set! ht '()))

    (define (hash-table-copy ht . _)
      (let ((new (%make-ht (ht-equiv ht) (ht-hashf ht) '())))
        (hash-table-walk ht (lambda (k v) (hash-table-set! new k v)))
        new))

    (define (hash-table-merge! ht other)
      (hash-table-walk other (lambda (k v) (hash-table-set! ht k v)))
      ht)

    (define (alist->hash-table alist . args)
      (let ((ht (apply make-hash-table args)))
        (for-each (lambda (p)
                    (if (not (hash-table-exists? ht (car p)))
                        (hash-table-set! ht (car p) (cdr p))))
                  alist)
        ht))

    ;; Hash functions — simple, bounded, only needed for export
    ;; completeness (the assoc backing never calls them).
    (define hash-bound 33554432)
    (define (string-hash s)
      (let ((n (string-length s)))
        (let loop ((i 0) (acc 0))
          (if (= i n)
              acc
              (loop (+ i 1)
                    (modulo (+ (* acc 31) (char->integer (string-ref s i)))
                            hash-bound))))))
    (define (string-ci-hash s) (string-hash (string-foldcase s)))
    (define (symbol-hash s) (string-hash (symbol->string s)))
    (define (equal-hash obj)
      (cond ((string? obj) (string-hash obj))
            ((symbol? obj) (symbol-hash obj))
            ((char? obj) (char->integer obj))
            ((number? obj) (if (and (integer? obj) (exact? obj))
                               (modulo (abs obj) hash-bound)
                               0))
            (else 0)))
    (define (hash obj . _) (equal-hash obj))
    (define (hash-by-identity obj . _) (equal-hash obj))))
