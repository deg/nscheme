;;; (srfi 159) — Combinator Formatting (SRFI 159, "show").
;;;
;;; Vendored from the SRFI 159 reference implementation by Alex Shinn,
;;; in the portable reorganisation contributed by Duy Nguyen (which
;;; removes the dependencies on (chibi show), (chibi string) and the
;;; (chibi monad environment) macro — the environment monad is expanded
;;; inline in monad.scm). BSD-style license (text preserved below).
;;;
;;; The upstream is split across many .sld files that (include "X.scm")
;;; their bodies. nscheme resolves `include` relative to the process
;;; directory rather than the library file, so every body is inlined
;;; here inside `begin` instead of being included.
;;;
;;; SCOPE FOR nscheme: this file vendors the (srfi 159 base) and
;;; (srfi 159 color) sub-libraries — the pure-Scheme combinator core
;;; (show / each / displayed / written / numeric / padded / trimmed /
;;; fitted / joined / escaped / pretty) plus the ANSI colour
;;; combinators. The (srfi 159 columnar) and (srfi 159 unicode)
;;; sub-libraries are NOT vendored: they import (srfi 117), (srfi 130)
;;; and (srfi 151) unconditionally and ship large Unicode width tables,
;;; none of which nscheme has. See missing_features.
;;;
;;; Two upstream dependencies are absent in nscheme and supplied here by
;;; a small portability shim inside the first `begin`:
;;;   * SRFI 130 string cursors — shimmed with integer indices (a string
;;;     cursor is simply an index), exactly as SRFI 115's own else-branch
;;;     does in this repo (lib/srfi/115.sld).
;;;   * SRFI 69 / SRFI 125 hash tables — only the `written`/`pretty`
;;;     shared-structure detector needs them, with eq? keys; shimmed with
;;;     a tiny mutable eq?-alist so we depend on neither SRFI.
;;;
;;; SPDX-FileCopyrightText: 2006 - 2020 Alex Shinn
;;; SPDX-License-Identifier: BSD-3-Clause
;;;
;;; Permission to use, copy, modify, and/or distribute this software for
;;; any purpose with or without fee is hereby granted, provided the above
;;; copyright notice and this permission notice appear in all copies.
;;; THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL
;;; WARRANTIES WITH REGARD TO THIS SOFTWARE. See the SRFI 159 document
;;; and http://synthcode.com/license.txt for the full BSD-style text.

(define-library (srfi 159)
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
   as-black as-bold as-underline)
  (import (scheme base)
          (scheme write)
          (scheme char)
          (scheme complex)
          (scheme inexact)
          (srfi 1))

  ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
  ;; Portability shim: SRFI 130 string cursors as integer indices, and a
  ;; tiny eq?-keyed hash table, neither of which nscheme provides.
  (begin

    ;; --- string cursors (a cursor is just an integer index) ---
    (define (string-cursor-start str) 0)
    (define (string-cursor-end str) (string-length str))
    (define (string-cursor? x) (integer? x))
    (define string-cursor=? =)
    (define string-cursor<? <)
    (define string-cursor<=? <=)
    (define string-cursor>? >)
    (define string-cursor>=? >=)
    (define (string-cursor-next str i) (+ i 1))
    (define (string-cursor-prev str i) (- i 1))
    (define (string-cursor-forward str i n) (+ i n))
    (define (string-cursor-back str i n) (- i n))
    (define (string-cursor->index str i) i)
    (define (string-index->cursor str i) i)
    (define (string-ref/cursor str i) (string-ref str i))
    (define (substring/cursors str start end) (substring str start end))
    (define (string-copy/cursors str start . o)
      (substring str start (if (pair? o) (car o) (string-length str))))

    ;; Coerce a char or predicate to a one-argument predicate.
    (define (char-pred x)
      (if (procedure? x) x (lambda (c) (eqv? c x))))

    ;; SRFI 130 string-index: index of the first char (>= optional start)
    ;; matching pred/char; the end cursor if none match.
    (define (string-index str pred . o)
      (let ((p (char-pred pred))
            (end (string-length str)))
        (let loop ((i (if (pair? o) (car o) 0)))
          (cond ((>= i end) end)
                ((p (string-ref str i)) i)
                (else (loop (+ i 1)))))))

    ;; SRFI 130 string-index-right: index of the char following the last
    ;; match, or the start cursor if none match.
    (define (string-index-right str pred . o)
      (let ((p (char-pred pred)))
        (let loop ((i (if (pair? o) (car o) (string-length str))))
          (cond ((<= i 0) 0)
                ((p (string-ref str (- i 1))) i)
                (else (loop (- i 1)))))))

    ;; Number of chars matching pred/char.
    (define (string-count str pred . o)
      (let ((p (char-pred pred))
            (end (string-length str)))
        (let loop ((i (if (pair? o) (car o) 0)) (n 0))
          (cond ((>= i end) n)
                ((p (string-ref str i)) (loop (+ i 1) (+ n 1)))
                (else (loop (+ i 1) n))))))

    ;; Index of the first occurrence of substring `sub`, or #f.
    (define (string-contains str sub)
      (let ((slen (string-length str))
            (plen (string-length sub)))
        (let loop ((i 0))
          (cond
           ((> (+ i plen) slen) #f)
           ((let cmp ((j 0))
              (or (= j plen)
                  (and (char=? (string-ref str (+ i j)) (string-ref sub j))
                       (cmp (+ j 1)))))
            i)
           (else (loop (+ i 1)))))))

    (define (string-prefix? prefix str)
      (let ((plen (string-length prefix)))
        (and (<= plen (string-length str))
             (let loop ((i 0))
               (or (= i plen)
                   (and (char=? (string-ref prefix i) (string-ref str i))
                        (loop (+ i 1))))))))

    (define (string-suffix? suffix str)
      (let ((slen (string-length suffix))
            (len (string-length str)))
        (and (<= slen len)
             (let loop ((i 0))
               (or (= i slen)
                   (and (char=? (string-ref suffix (- slen i 1))
                                (string-ref str (- len i 1)))
                        (loop (+ i 1))))))))

    ;; --- let-optionals* (portable fallback from compat.sld) ---
    (define-syntax let-optionals*
      (syntax-rules ()
        ((let-optionals* opt-ls () . body)
         (begin . body))
        ((let-optionals* (op . args) vars . body)
         (let ((tmp (op . args)))
           (let-optionals* tmp vars . body)))
        ((let-optionals* tmp ((var default) . rest) . body)
         (let ((var (if (pair? tmp) (car tmp) default))
               (tmp2 (if (pair? tmp) (cdr tmp) '())))
           (let-optionals* tmp2 rest . body)))
        ((let-optionals* tmp tail . body)
         (let ((tail tmp)) . body))))

    ;; --- negative?* (treats -0.0 as negative, from compat.sld) ---
    (define (negative?* n)
      (or (negative? n) (eqv? -0.0 n)))

    ;; --- minimal eq?-keyed hash table (SRFI 69 subset) ---
    ;; Backed by a mutable alist held in a single-slot vector; this is
    ;; only used by the shared-structure detector for written/pretty, so
    ;; performance is not a concern.
    (define (make-hash-table . o)
      (vector '()))
    (define (ht-assq tbl key)
      (let loop ((ls (vector-ref tbl 0)))
        (cond ((null? ls) #f)
              ((eq? (caar ls) key) (car ls))
              (else (loop (cdr ls))))))
    (define (hash-table-ref/default tbl key default)
      (let ((cell (ht-assq tbl key)))
        (if cell (cdr cell) default)))
    (define (hash-table-ref tbl key . o)
      (let ((cell (ht-assq tbl key)))
        (cond (cell (cdr cell))
              ((pair? o) ((car o)))
              (else (error "hash-table-ref: key not found" key)))))
    (define (hash-table-set! tbl key val)
      (let ((cell (ht-assq tbl key)))
        (if cell
            (set-cdr! cell val)
            (vector-set! tbl 0 (cons (cons key val) (vector-ref tbl 0))))))
    (define (hash-table-update!/default tbl key proc default)
      (let ((cell (ht-assq tbl key)))
        (if cell
            (set-cdr! cell (proc (cdr cell)))
            (vector-set! tbl 0
                         (cons (cons key (proc default)) (vector-ref tbl 0))))))
    (define (hash-table-delete! tbl key)
      (vector-set! tbl 0
                   (let loop ((ls (vector-ref tbl 0)))
                     (cond ((null? ls) '())
                           ((eq? (caar ls) key) (cdr ls))
                           (else (cons (car ls) (loop (cdr ls))))))))
    (define (hash-table-walk tbl proc)
      (for-each (lambda (cell) (proc (car cell) (cdr cell)))
                (vector-ref tbl 0))))

  ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
  ;; monad.scm — the environment (reader) monad, hand-expanded.
  (begin

    (define-record-type Show-Env
        (make-state port row col width radix precision
                    pad-char decimal-sep decimal-align
                    string-width ellipsis writer output
                    %props)
        state?
      (port env-port env-port-set!)
      (row env-row env-row-set!)
      (col env-col env-col-set!)
      (width env-width env-width-set!)
      (radix env-radix env-radix-set!)
      (precision env-precision env-precision-set!)
      (pad-char env-pad-char env-pad-char-set!)
      (decimal-sep env-decimal-sep env-decimal-sep-set!)
      (decimal-align env-decimal-align env-decimal-align-set!)
      (string-width env-string-width env-string-width-set!)
      (ellipsis env-ellipsis env-ellipsis-set!)
      (writer env-writer env-writer-set!)
      (output env-output env-output-set!)
      (%props get-props set-props!))

    (define (ask st x)
      (case x
        ((port) (env-port st))
        ((row) (env-row st))
        ((col) (env-col st))
        ((width) (env-width st))
        ((radix) (env-radix st))
        ((precision) (env-precision st))
        ((pad-char) (env-pad-char st))
        ((decimal-sep) (env-decimal-sep st))
        ((decimal-align) (env-decimal-align st))
        ((string-width) (env-string-width st))
        ((ellipsis) (env-ellipsis st))
        ((writer) (env-writer st))
        ((output) (env-output st))
        (else (cond ((assq x (get-props st)) => cdr) (else #f)))))

    (define (tell st x val)
      (case x
        ((port) (env-port-set! st val))
        ((row) (env-row-set! st val))
        ((col) (env-col-set! st val))
        ((width) (env-width-set! st val))
        ((radix) (env-radix-set! st val))
        ((precision) (env-precision-set! st val))
        ((pad-char) (env-pad-char-set! st val))
        ((decimal-sep) (env-decimal-sep-set! st val))
        ((decimal-align) (env-decimal-align-set! st val))
        ((string-width) (env-string-width-set! st val))
        ((ellipsis) (env-ellipsis-set! st val))
        ((writer) (env-writer-set! st val))
        ((output) (env-output-set! st val))
        (else
         (cond
          ((assq x (get-props st))
           => (lambda (cell) (set-cdr! cell val)))
          (else
           (set-props! st (cons (cons x val) (get-props st))))))))

    ;; External API
    ;;
    ;; copy
    (define (c st)
      (make-state
       (env-port st)
       (env-row st)
       (env-col st)
       (env-width st)
       (env-radix st)
       (env-precision st)
       (env-pad-char st)
       (env-decimal-sep st)
       (env-decimal-align st)
       (env-string-width st)
       (env-ellipsis st)
       (env-writer st)
       (env-output st)
       (map (lambda (x)
              (cons (car x) (cdr x)))
            (get-props st))))

    ;; bind - a function
    (define-syntax %fn
      (syntax-rules ()
        ((%fn ("step") (params ...) ((p param) . rest) . body)
         (%fn ("step") (params ... (p param)) rest . body))
        ((%fn ("step") (params ...) ((param) . rest) . body)
         (%fn ("step") (params ... (param param)) rest . body))
        ((%fn ("step") (params ...) (param . rest) . body)
         (%fn ("step") (params ... (param param)) rest . body))
        ((%fn ("step") ((p param) ...) () . body)
         (lambda (st)
           (let ((p (ask st 'param)) ...)
             ((let () . body) st))))
        ((%fn params . body)
         (%fn ("step") () params . body))))

    (define-syntax fn
      (syntax-rules ()
        ((fn vars expr ... fmt)
         (%fn vars expr ... (displayed fmt)))))

    ;; fork - run on a copy of the state
    (define-syntax forked
      (syntax-rules ()
        ((forked a) a)
        ((forked a b) (lambda (st) (a (c st)) (b st)))
        ((forked a b . c) (forked a (forked b . c)))))

    ;; sequence
    (define-syntax sequence
      (syntax-rules ()
        ((sequence f) f)
        ((sequence f . g) (lambda (st) ((sequence . g) (f st))))))

    ;; update in place
    (define-syntax with!
      (syntax-rules ()
        ((with! (prop value) ...)
         (lambda (st)
           (tell st 'prop value) ...
           st))))

    ;; local binding - update temporarily
    (define-syntax %with
      (syntax-rules ()
        ((%with ("step") ((p tmp v) ...) () . b)
         (lambda (st)
           (let ((tmp (ask st 'p)) ...)
             (dynamic-wind
               (lambda () (tell st 'p v) ...)
               (lambda () ((begin . b) st))
               (lambda () (tell st 'p tmp) ...)))))
        ((%with ("step") (props ...) ((p v) . rest) . b)
         (%with ("step") (props ... (p tmp v)) rest . b))
        ((%with ((prop value) ...) . body)
         (%with ("step") () ((prop value) ...) . body))))

    (define-syntax with
      (syntax-rules ()
        ((with params x ... y)
         (%with params (each x ... y)))))

    ;; run
    (define (run proc)
      (proc (make-state #f #f #f #f #f #f #f #f #f #f #f #f #f '())))

    ;; return
    (define (return x)
      (lambda (st) x)))

  ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
  ;; base.scm — base formatting combinators and the show interface.
  (begin

    ;; Utility - default value of string-width.
    (define (substring-length str . o)
      (let ((start (if (pair? o) (car o) 0))
            (end (if (and (pair? o) (pair? (cdr o))) (cadr o) (string-length str))))
        (- end start)))

    ;; Raw output.  All primitive output should go through this.
    (define (output str)
      (fn (output) ((or output output-default) str)))

    (define (show out . args)
      (let ((proc (each-in-list args)))
        (cond
         ((output-port? out)
          (show-run out proc))
         ((eq? #t out)
          (show-run (current-output-port) proc))
         ((eq? #f out)
          (let ((out (open-output-string)))
            (show-run out proc)
            (get-output-string out)))
         (else
          (error "unknown output to show" out)))))

    ;; Run with an output port with initial default values.
    (define (show-run out proc)
      (run (sequence (with! (port out)
                            (col 0)
                            (row 0)
                            (width 78)
                            (radix 10)
                            (pad-char #\space)
                            (output output-default)
                            (string-width substring-length))
                     proc)))

    (define nothing (fn () (with!)))

    (define (displayed x)
      (cond
       ((procedure? x) x)
       ((string? x) (output x))
       ((char? x) (output (string x)))
       (else (written x))))

    (define (written x)
      (fn (writer) ((or writer written-default) x)))

    (define (each-in-list args)
      (if (pair? args)
          (sequence (displayed (car args)) (each-in-list (cdr args)))
          nothing))

    (define (each . args)
      (each-in-list args))

    (define (output-default str)
      (fn (port row col string-width)
        (display str port)
        (let ((nl-index (string-index-right str #\newline)))
          (if (string-cursor>? nl-index (string-cursor-start str))
              (with! (row (+ row (string-count str (lambda (x) (char=? x #\newline)))))
                     (col (string-width str (string-cursor->index str nl-index))))
              (with! (col (+ col (string-width str))))))))

    (define (call-with-output producer consumer)
      (let ((out (open-output-string)))
        (forked (with ((port out) (output output-default)) producer)
                (fn () (consumer (get-output-string out)))))))

  ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
  ;; write.scm — written formatting, numeric formatting, shared structure.
  (begin

    (define (write-to-string x)
      (let ((out (open-output-string)))
        (write x out)
        (get-output-string out)))

    (define (string-replace-all str ch1 ch2)
      (let ((out (open-output-string)))
        (string-for-each
         (lambda (ch) (display (if (eqv? ch ch1) ch2 ch) out))
         str)
        (get-output-string out)))

    (define (string-intersperse-right str sep rule)
      (define (cursor-back str i offset)    ; safe version
        (if (or (zero? offset)
                (string-cursor=? i (string-cursor-start str)))
            i
            (cursor-back str (string-cursor-prev str i) (- offset 1))))
      (let ((start (string-cursor-start str)))
        (let lp ((i (string-cursor-end str))
                 (rule rule)
                 (res '()))
          (let* ((offset (if (pair? rule) (car rule) rule))
                 (i2 (if offset (cursor-back str i offset) start)))
            (if (string-cursor<=? i2 start)
                (apply string-append (cons (substring/cursors str start i) res))
                (lp i2
                    (if (and (pair? rule) (not (null? (cdr rule)))) (cdr rule) rule)
                    (cons sep (cons (substring/cursors str i2 i) res))))))))

    (define (escaped fmt . o)
      (let-optionals* o ((quot #\")
                         (esc #\\)
                         (rename (lambda (x) #f)))
        (let ((esc-str (cond ((char? esc) (string esc))
                             ((not esc) (string quot))
                             (else esc))))
          (fn (output)
            (define (output* str)
              (let ((start (string-cursor-start str))
                    (end (string-cursor-end str)))
                (let lp ((i start) (j start))
                  (define (collect)
                    (if (eq? i j) "" (substring/cursors str i j)))
                  (if (string-cursor>=? j end)
                      (output (collect))
                      (let ((c (string-ref/cursor str j))
                            (j2 (string-cursor-next str j)))
                        (cond
                         ((or (eqv? c quot) (eqv? c esc))
                          (each (output (collect))
                                (output esc-str)
                                (fn () (lp j j2))))
                         ((rename c)
                          => (lambda (c2)
                               (each (output (collect))
                                     (output esc-str)
                                     (output (if (char? c2) (string c2) c2))
                                     (fn () (lp j2 j2)))))
                         (else
                          (lp i j2))))))))
            (with ((output output*))
              fmt)))))

    (define (maybe-escaped fmt pred . o)
      (let-optionals* o ((quot #\")
                         (esc #\\)
                         (rename (lambda (x) #f)))
        (define (esc? c) (or (eqv? c quot) (eqv? c esc) (rename c) (pred c)))
        (call-with-output
         fmt
         (lambda (str)
           (if (string-cursor<? (string-index str esc?) (string-cursor-end str))
               (each quot (escaped str quot esc rename) quot)
               (displayed str))))))

    ;; numeric formatting

    (define (char-mirror c)
      (case c ((#\() #\)) ((#\[) #\]) ((#\{) #\}) ((#\<) #\>) (else c)))

    (define (integer-log a base)
      (if (zero? a)
          0
          (do ((ndigits 1 (+ ndigits 1))
               (p base (* p base)))
              ((> p a) ndigits))))

    (define unspec (list 'unspecified))

    ;; Renamed from upstream `default` to avoid colliding, in this
    ;; flattened single-scope file, with the `default` let-binding in
    ;; pp-with-indent below (upstream they live in separate libraries).
    (define-syntax %default
      (syntax-rules ()
        ((%default var dflt) (if (eq? var unspec) dflt var))))

    (define (numeric n . o)
      (let-optionals* o ((rad unspec) (prec unspec) (sgn unspec)
                         (comma unspec) (commasep unspec) (decsep unspec))
        (fn (radix precision sign-rule
                   comma-rule comma-sep decimal-sep decimal-align)
          (let* ((radix (%default rad radix))
                 (precision (%default prec precision))
                 (sign-rule (%default sgn sign-rule))
                 (comma-rule (%default comma comma-rule))
                 (comma-sep (%default commasep comma-sep))
                 (dec-sep (%default decsep
                            (or decimal-sep (if (eqv? comma-sep #\.) #\, #\.))))
                 (dec-ls (if (char? dec-sep)
                             (list dec-sep)
                             (reverse (string->list dec-sep)))))
            (define (get-scale q)
              (expt radix (- (integer-log q radix) 1)))
            (define (char-digit d)
              (cond ((char? d) d)
                    ((< d 10) (integer->char (+ d (char->integer #\0))))
                    (else (integer->char (+ (- d 10) (char->integer #\a))))))
            (define (digit-value ch)
              (let ((res (- (char->integer ch) (char->integer #\0))))
                (if (<= 0 res 9)
                    res
                    ch)))
            (define (round-up ls)
              (let lp ((ls ls) (res '()))
                (cond
                 ((null? ls)
                  (append-reverse res '(1)))
                 ((not (number? (car ls)))
                  (lp (cdr ls) (cons (car ls) res)))
                 ((= (car ls) (- radix 1))
                  (lp (cdr ls) (cons 0 res)))
                 (else
                  (append (reverse res) (cons (+ 1 (car ls)) (cdr ls)))))))
            (define (maybe-round n d ls)
              (let* ((q (quotient n d))
                     (digit (* 2 (if (>= q radix) (quotient q (get-scale q)) q))))
                (if (or (> digit radix)
                        (and (= digit radix)
                             (let ((prev (find integer? ls)))
                               (and prev (odd? prev)))))
                    (round-up ls)
                    ls)))
            (define (maybe-trim-zeros i res inexact?)
              (if (and (not precision) (positive? i))
                  (let lp ((res res))
                    (cond
                     ((and (pair? res) (eqv? 0 (car res))) (lp (cdr res)))
                     ((and (pair? res)
                           (eqv? (car dec-ls) (car res))
                           (null? (cdr dec-ls)))
                      (if inexact?
                          (cons 0 res)      ; "1.0"
                          (cdr res)))       ; "1"
                     (else res)))
                  res))
            (define (gen-general n-orig)
              (let* ((p (exact n-orig))
                     (n (numerator p))
                     (d (denominator p)))
                (let lp ((n n)
                         (i (if (zero? p) -1 (- (integer-log p radix))))
                         (res '()))
                  (cond
                   ((if precision (< i precision) (< i 16))
                    (let ((res (if (zero? i)
                                   (append dec-ls (if (null? res) (cons 0 res) res))
                                   res))
                          (q (quotient n d)))
                      (cond
                       ((< i -1)
                        (let* ((scale (expt radix (- -1 i)))
                               (digit (quotient q scale))
                               (n2 (- n (* d digit scale))))
                          (lp n2 (+ i 1) (cons digit res))))
                       (else
                        (lp (* (remainder n d) radix)
                            (+ i 1)
                            (cons q res))))))
                   (else
                    (list->string
                     (map char-digit
                          (reverse (maybe-trim-zeros i (maybe-round n d res) (inexact? n-orig))))))))))
            (define (gen-fixed n)
              (cond
               ((and (eqv? radix 10) (zero? precision) (inexact? n))
                (number->string (exact (round n))))
               ((and (eqv? radix 10) (or (integer? n) (inexact? n)))
                (let* ((s (number->string n))
                       (end (string-cursor-end s))
                       (dec (string-index s #\.))
                       (digits (- (string-cursor->index s end)
                                  (string-cursor->index s dec))))
                  (cond
                   ((string-cursor<? (string-index s #\e) end)
                    (gen-general n))
                   ((string-cursor=? dec end)
                    (string-append s (if (char? dec-sep) (string dec-sep) dec-sep)
                                   (make-string precision #\0)))
                   ((<= digits precision)
                    (string-append s (make-string (- precision digits -1) #\0)))
                   (else
                    (let* ((last
                            (string-cursor-back s end (- digits precision 1)))
                           (res (substring/cursors s (string-cursor-start s) last)))
                      (if (and
                           (string-cursor<? last end)
                           (let ((next (digit-value (string-ref/cursor s last))))
                             (or (> next 5)
                                 (and (= next 5)
                                      (string-cursor>? last (string-cursor-start s))
                                      (memv (digit-value
                                             (string-ref/cursor
                                              s (string-cursor-prev s last)))
                                            '(1 3 5 7 9))))))
                          (list->string
                           (reverse
                            (map char-digit
                                 (round-up
                                  (reverse (map digit-value (string->list res)))))))
                          res))))))
               (else
                (gen-general n))))
            (define (gen-positive-real n)
              (cond
               (precision
                (gen-fixed n))
               ((memv radix (if (exact? n) '(2 8 10 16) '(10)))
                (number->string n radix))
               (else
                (gen-general n))))
            (define (insert-commas str)
              (let* ((dec-pos (if (string? dec-sep)
                                  (or (string-contains str dec-sep)
                                      (string-cursor-end str))
                                  (string-index str dec-sep)))
                     (left (substring/cursors str (string-cursor-start str) dec-pos))
                     (right (string-copy/cursors str dec-pos))
                     (sep (cond ((char? comma-sep) (string comma-sep))
                                ((string? comma-sep) comma-sep)
                                ((eqv? #\, dec-sep) ".")
                                (else ","))))
                (string-append
                 (string-intersperse-right left sep comma-rule)
                 right)))
            (define (wrap-comma n)
              (if (and (not precision) (exact? n) (not (integer? n)))
                  (string-append (wrap-comma (numerator n))
                                 "/"
                                 (wrap-comma (denominator n)))
                  (let* ((s0 (gen-positive-real n))
                         (s1 (if (or (eqv? #\. dec-sep)
                                     (equal? "." dec-sep))
                                 s0
                                 (string-replace-all s0 #\. dec-sep))))
                    (if comma-rule (insert-commas s1) s1))))
            (define (wrap-sign n sign-rule)
              (cond
               ((negative?* n)
                (cond
                 ((char? sign-rule)
                  (string-append (string sign-rule)
                                 (wrap-comma (- n))
                                 (string (char-mirror sign-rule))))
                 ((pair? sign-rule)
                  (string-append (car sign-rule)
                                 (wrap-comma (- n))
                                 (cdr sign-rule)))
                 (else
                  (string-append "-" (wrap-comma (- n))))))
               ((eq? #t sign-rule)
                (string-append "+" (wrap-comma n)))
               (else
                (wrap-comma n))))
            (define (format n sign-rule)
              (cond
               ((finite? n)
                (let* ((s (wrap-sign n sign-rule))
                       (dec-pos (if decimal-align
                                    (string-cursor->index
                                     s
                                     (if (char? dec-sep)
                                         (string-index s dec-sep)
                                         (or (string-contains s dec-sep)
                                             (string-cursor-end s))))
                                    0))
                       (diff (- (or decimal-align 0) dec-pos 1)))
                  (if (positive? diff)
                      (string-append (make-string diff #\space) s)
                      s)))
               (else
                (number->string n))))
            (define (write-complex n)
              (cond
               ((and radix (not (and (integer? radix) (<= 2 radix 36))))
                (error "invalid radix for numeric formatting" radix))
               ((zero? (imag-part n))
                (displayed (format (real-part n) sign-rule)))
               (else
                (each (format (real-part n) sign-rule)
                      (format (imag-part n) #t)
                      "i"))))
            (write-complex n)))))

    (define numeric/si
      (let* ((names10 '#("" "k" "M" "G" "T" "E" "P" "Z" "Y"))
             (names-10 '#("" "m" "µ" "n" "p" "f" "a" "z" "y"))
             (names2 (list->vector
                      (cons ""
                            (cons "Ki" (map (lambda (s) (string-append s "i"))
                                            (cddr (vector->list names10)))))))
             (names-2 (list->vector
                       (cons ""
                             (map (lambda (s) (string-append s "i"))
                                  (cdr (vector->list names-10)))))))
        (define (round-to n k)
          (/ (round (* n k)) k))
        (lambda (n . o)
          (let-optionals* o ((base 1024)
                             (separator ""))
            (let* ((log-n (log n))
                   (names  (if (negative? log-n)
                               (if (= base 1024) names-2 names-10)
                               (if (= base 1024) names2 names10)))
                   (k (min (exact ((if (negative? log-n) ceiling floor)
                                   (/ (abs log-n) (log base))))
                           (- (vector-length names) 1)))
                   (n2 (round-to (/ n (expt base (if (negative? log-n) (- k) k)))
                                 10)))
              (each (if (integer? n2)
                        (number->string (exact n2))
                        (inexact n2))
                    separator
                    (vector-ref names k)))))))

    (define (numeric/fitted width n . args)
      (call-with-output
       (apply numeric n args)
       (lambda (str)
         (if (> (string-length str) width)
             (fn (precision decimal-sep comma-sep)
               (let ((prec (if (and (pair? args) (pair? (cdr args)))
                               (cadr args)
                               precision)))
                 (if (and prec (not (zero? prec)))
                     (let* ((dec-sep
                             (or decimal-sep
                                 (if (eqv? #\. comma-sep) #\, #\.)))
                            (diff (- width (+ prec
                                              (if (char? dec-sep)
                                                  1
                                                  (string-length dec-sep))))))
                       (each (if (positive? diff) (make-string diff #\#) "")
                             dec-sep (make-string prec #\#)))
                     (displayed (make-string width #\#)))))
             (displayed str)))))

    (define (numeric/comma n . o)
      (fn (comma-rule)
        (with ((comma-rule (or comma-rule 3)))
          (apply numeric n o))))

    ;;; shared structure utilities

    (define (extract-shared-objects x cyclic-only?)
      (let ((seen (make-hash-table eq?)))
        (let find ((x x))
          (cond
           ((or (pair? x) (vector? x))
            (hash-table-update!/default seen x (lambda (n) (+ n 1)) 0)
            (cond
             ((> (hash-table-ref seen x) 1))
             ((pair? x)
              (find (car x))
              (find (cdr x)))
             ((vector? x)
              (do ((i 0 (+ i 1)))
                  ((= i (vector-length x)))
                (find (vector-ref x i)))))
            (if (and cyclic-only? (<= (hash-table-ref/default seen x 0) 1))
                (hash-table-delete! seen x)))))
        (let ((res (make-hash-table eq?))
              (count 0))
          (hash-table-walk
           seen
           (lambda (k v)
             (cond
              ((> v 1)
               (hash-table-set! res k (cons count #f))
               (set! count (+ count 1))))))
          (cons res 0))))

    (define (maybe-gen-shared-ref cell shares)
      (cond
        ((pair? cell)
         (set-car! cell (cdr shares))
         (set-cdr! cell #t)
         (set-cdr! shares (+ (cdr shares) 1))
         (each "#" (number->string (car cell)) "="))
        (else nothing)))

    (define (call-with-shared-ref obj shares proc)
      (let ((cell (hash-table-ref/default (car shares) obj #f)))
        (if (and (pair? cell) (cdr cell))
            (each "#" (number->string (car cell)) "#")
            (each (maybe-gen-shared-ref cell shares) proc))))

    (define (call-with-shared-ref/cdr obj shares proc . o)
      (let ((sep (displayed (if (pair? o) (car o) "")))
            (cell (hash-table-ref/default (car shares) obj #f)))
        (cond
          ((and (pair? cell) (cdr cell))
           (each sep ". #" (number->string (car cell)) "#"))
          ((pair? cell)
           (each sep ". " (maybe-gen-shared-ref cell shares) "(" proc ")"))
          (else
           (each sep proc)))))

    ;; written

    (define (write-with-shares obj shares)
      (fn (radix precision)
        (let ((write-number
               (cond
                ((and (not precision)
                      (assv radix '((16 . "#x") (10 . "") (8 . "#o") (2 . "#b"))))
                 => (lambda (cell)
                      (lambda (n)
                        (cond
                         ((eqv? radix 10)
                          (displayed (number->string n (car cell))))
                         ((exact? n)
                          (each (cdr cell) (number->string n (car cell))))
                         (else
                          (with ((radix 10)) (numeric n)))))))
                (else (lambda (n) (with ((radix 10)) (numeric n)))))))
          (let wr ((obj obj))
            (call-with-shared-ref
             obj shares
             (fn ()
               (cond
                ((pair? obj)
                 (each "("
                       (fn ()
                         (let lp ((ls obj))
                           (let ((rest (cdr ls)))
                             (each (wr (car ls))
                                   (cond
                                    ((null? rest)
                                     nothing)
                                    ((pair? rest)
                                     (each
                                      " "
                                      (call-with-shared-ref/cdr
                                       rest shares
                                       (fn () (lp rest)))))
                                    (else
                                     (each " . " (wr rest))))))))
                       ")"))
                ((vector? obj)
                 (let ((len (vector-length obj)))
                   (if (zero? len)
                       (displayed "#()")
                       (each "#("
                             (wr (vector-ref obj 0))
                             (fn ()
                               (let lp ((i 1))
                                 (if (>= i len)
                                     nothing
                                     (each " " (wr (vector-ref obj i))
                                           (fn () (lp (+ i 1)))))))
                             ")"))))
                ((number? obj)
                 (write-number obj))
                (else
                 (displayed (write-to-string obj))))))))))

    (define (written-default obj)
      (fn ()
        (write-with-shares obj (extract-shared-objects obj #t))))

    (define (written-simply obj)
      (fn ()
        (write-with-shares obj (extract-shared-objects #f #f)))))

  ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
  ;; util.scm — spacing, padding, trimming, joining combinators.
  (begin

    (define nl (displayed "\n"))

    (define fl
      (fn (col) (if (zero? col) nothing nl)))

    (define (tab-to . o)
      (fn (col pad-char)
        (let* ((tab-width (if (pair? o) (car o) 8))
               (rem (modulo col tab-width)))
          (if (positive? rem)
              (displayed (make-string (- tab-width rem) pad-char))
              nothing))))

    (define (space-to where)
      (fn (col pad-char)
        (displayed (make-string (max 0 (- where col)) pad-char))))

    (define (with-string-transformer proc . ls)
      (fn (output)
        (let ((output* (lambda (str) (fn () (output (proc str))))))
          (with ((output output*)) (each-in-list ls)))))

    (define (upcased . ls) (apply with-string-transformer string-upcase ls))

    (define (downcased . ls) (apply with-string-transformer string-downcase ls))

    (define (padded/both width . ls)
      (call-with-output
       (each-in-list ls)
       (lambda (str)
         (fn (string-width pad-char)
           (let ((diff (- width (string-width str))))
             (if (positive? diff)
                 (let* ((diff/2 (quotient diff 2))
                        (left (make-string diff/2 pad-char))
                        (right (if (even? diff)
                                   left
                                   (make-string (+ 1 diff/2) pad-char))))
                   (each left str right))
                 (displayed str)))))))

    (define (padded/right width . ls)
      (fn ((col1 col))
        (each (each-in-list ls)
              (fn ((col2 col) pad-char)
                (displayed (make-string (max 0 (- width (- col2 col1)))
                                        pad-char))))))

    (define (padded/left width . ls)
      (call-with-output
       (each-in-list ls)
       (lambda (str)
         (fn (string-width pad-char)
           (let ((diff (- width (string-width str))))
             (each (make-string (max 0 diff) pad-char) str))))))

    (define padded padded/left)

    (define (trimmed/buffered width producer proc)
      (call-with-output
       producer
       (lambda (str)
         (fn (string-width)
           (let* ((str-width (string-width str))
                  (diff (- str-width width)))
             (displayed (if (positive? diff)
                            (proc str str-width diff)
                            str)))))))

    (define (trimmed/right width . ls)
      (trimmed/buffered
       width
       (each-in-list ls)
       (lambda (str str-width diff)
         (fn (ellipsis string-width col)
           (let* ((ell (if (char? ellipsis) (string ellipsis) (or ellipsis "")))
                  (ell-len (string-width ell))
                  (diff (- (+ str-width ell-len) width)))
             (each (if (negative? diff)
                       nothing
                       (substring str 0 (- width ell-len)))
                   ell))))))

    (define (trimmed/left width . ls)
      (trimmed/buffered
       width
       (each-in-list ls)
       (lambda (str str-width diff)
         (fn (ellipsis string-width)
           (let* ((ell (if (char? ellipsis) (string ellipsis) (or ellipsis "")))
                  (ell-len (string-width ell))
                  (diff (- (+ str-width ell-len) width)))
             (each ell
                   (if (negative? diff)
                       nothing
                       (string-copy str diff))))))))

    (define trimmed trimmed/left)

    (define (trimmed/both width . ls)
      (trimmed/buffered
       width
       (each-in-list ls)
       (lambda (str str-width diff)
         (fn (ellipsis string-width)
           (let* ((ell (if (char? ellipsis) (string ellipsis) (or ellipsis "")))
                  (ell-len (string-width ell))
                  (diff (- (+ str-width ell-len ell-len) width))
                  (left (quotient diff 2))
                  (right (- (string-width str) (quotient (+ diff 1) 2))))
             (if (negative? diff)
                 ell
                 (each ell (substring str left right) ell)))))))

    (define (trimmed/lazy width . ls)
      (fn ((orig-output output) string-width)
        (call-with-current-continuation
         (lambda (return)
           (let ((chars-written 0)
                 (output (or orig-output output-default)))
             (define (output* str)
               (let ((len (string-width str)))
                 (set! chars-written (+ chars-written len))
                 (if (> chars-written width)
                     (let* ((end (max 0 (- len (- chars-written width))))
                            (s (substring str 0 end)))
                       (each (output s)
                             (with! (output orig-output))
                             (fn () (return nothing))))
                     (output str))))
             (with ((output output*))
               (each-in-list ls)))))))

    (define (fitted/right width . ls)
      (padded/right width (trimmed/right width (each-in-list ls))))

    (define (fitted/left width . ls)
      (padded/left width (trimmed/left width (each-in-list ls))))

    (define fitted fitted/left)

    (define (fitted/both width . ls)
      (padded/both width (trimmed/both width (each-in-list ls))))

    (define (joined/general elt-f last-f dot-f init-ls sep)
      (fn ()
        (let lp ((ls init-ls))
          (cond
           ((pair? ls)
            (each (if (eq? ls init-ls) nothing sep)
                  ((if (and last-f (null? (cdr ls))) last-f elt-f) (car ls))
                  (lp (cdr ls))))
           ((and dot-f (not (null? ls)))
            (each (if (eq? ls init-ls) nothing sep) (dot-f ls)))
           (else
            nothing)))))

    (define (joined elt-f ls . o)
      (joined/general elt-f #f #f ls (if (pair? o) (car o) "")))

    (define (joined/prefix elt-f ls . o)
      (if (null? ls)
          nothing
          (let ((sep (if (pair? o) (car o) "")))
            (each sep (joined elt-f ls sep)))))

    (define (joined/suffix elt-f ls . o)
      (if (null? ls)
          nothing
          (let ((sep (if (pair? o) (car o) "")))
            (each (joined elt-f ls sep) sep))))

    (define (joined/last elt-f last-f ls . o)
      (joined/general elt-f last-f #f ls (if (pair? o) (car o) "")))

    (define (joined/dot elt-f dot-f ls . o)
      (joined/general elt-f #f dot-f ls (if (pair? o) (car o) "")))

    (define (joined/range elt-f start . o)
      (let ((end (and (pair? o) (car o)))
            (sep (if (and (pair? o) (pair? (cdr o))) (cadr o) "")))
        (let lp ((i start))
          (if (and end (>= i end))
              nothing
              (each (if (> i start) sep nothing)
                    (elt-f i)
                    (fn () (lp (+ i 1)))))))))

  ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
  ;; pretty.scm — pretty-printing combinator.
  (begin

    (define (take* ls n)   ; handles dotted lists and n > length
      (cond ((zero? n) '())
            ((pair? ls) (cons (car ls) (take* (cdr ls) (- n 1))))
            (else '())))

    (define (drop* ls n)   ; may return the dot
      (cond ((zero? n) ls)
            ((pair? ls) (drop* (cdr ls) (- n 1)))
            (else ls)))

    (define (make-space n) (make-string n #\space))
    (define (make-nl-space n) (string-append "\n" (make-string n #\space)))

    (define (joined/shares fmt ls shares . o)
      (let ((sep (displayed (if (pair? o) (car o) " "))))
        (fn ()
          (if (null? ls)
              nothing
              (let lp ((ls ls))
                (each
                 (fmt (car ls))
                 (let ((rest (cdr ls)))
                   (cond
                    ((null? rest) nothing)
                    ((pair? rest)
                     (call-with-shared-ref/cdr rest
                                               shares
                                               (fn () (lp rest))
                                               sep))
                    (else (each sep ". " (fmt rest)))))))))))

    (define (string-find/index str pred i)
      (string-cursor->index
       str
       (string-index str pred (string-index->cursor str i))))

    (define (try-fitted2 proc fail)
      (fn (width output)
        (let ((out (open-output-string)))
          (call-with-current-continuation
           (lambda (abort)
             (define (output* str)
               (fn (col)
                 (let lp ((i 0) (col col))
                   (let ((nli (string-find/index str #\newline i))
                         (len (string-length str)))
                     (if (< nli len)
                         (if (> (+ (- nli i) col) width)
                             (abort fail)
                             (lp (+ nli 1) 0))
                         (let ((col (+ (- len i) col)))
                           (cond
                            ((> col width)
                             (abort fail))
                            (else
                             (output-default str)))))))))
             (forked
              (with ((output output*)
                     (port out))
                proc)
              (output (get-output-string out))))))))

    (define (try-fitted proc . fail)
      (if (null? fail)
          proc
          (try-fitted2 proc (apply try-fitted fail))))

    (define (fits-in-width width proc)
      (call-with-current-continuation
       (lambda (abort)
         (show
          #f
          (fn (output)
            (define (output* str)
              (each (output str)
                    (fn (col)
                      (if (>= col width)
                          (abort #f)
                          nothing))))
            (with ((output output*))
              proc))))))

    (define (fits-in-columns width ls writer)
      (let ((max-w (quotient width 2)))
        (let lp ((ls ls) (res '()) (widest 0))
          (cond
           ((pair? ls)
            (let ((str (fits-in-width max-w (writer (car ls)))))
              (and str
                   (lp (cdr ls)
                       (cons str res)
                       (max (string-length str) widest)))))
           ((null? ls) (cons widest (reverse res)))
           (else #f)))))

    ;; style

    (define syntax-abbrevs
      '((quote . "'") (quasiquote . "`")
        (unquote . ",") (unquote-splicing . ",@")
        ))

    (define (pp-let ls pp shares)
      (if (and (pair? (cdr ls)) (symbol? (cadr ls)))
          (pp-with-indent 2 ls pp shares)
          (pp-with-indent 1 ls pp shares)))

    (define indent-rules
      `((lambda . 1) (define . 1)
        (let . ,pp-let) (loop . ,pp-let)
        (let* . 1) (letrec . 1) (letrec* . 1) (and-let* . 1) (let1 . 2)
        (let-values . 1) (let*-values . 1) (receive . 2) (parameterize . 1)
        (let-syntax . 1) (letrec-syntax . 1) (syntax-rules . 1) (syntax-case . 2)
        (match . 1) (match-let . 1) (match-let* . 1)
        (if . 3) (when . 1) (unless . 1) (case . 1) (while . 1) (until . 1)
        (do . 2) (dotimes . 1) (dolist . 1) (test . 1)
        (condition-case . 1) (guard . 1) (rec . 1)
        (call-with-current-continuation . 0)
        ))

    (define indent-prefix-rules
      `(("with-" . -1) ("call-with-" . -1) ("define-" . 1))
      )

    (define indent-suffix-rules
      `(("-case" . 1))
      )

    (define (pp-indentation form)
      (let ((indent
             (cond
              ((assq (car form) indent-rules) => cdr)
              ((and (symbol? (car form))
                    (let ((str (symbol->string (car form))))
                      (or (find (lambda (rx) (string-prefix? (car rx) str))
                                indent-prefix-rules)
                          (find (lambda (rx) (string-suffix? (car rx) str))
                                indent-suffix-rules))))
               => cdr)
              (else #f))))
        (if (and (number? indent) (negative? indent))
            (max 0 (- (+ (or (length+ form) +inf.0) indent) 1))
            indent)))

    (define (with-reset-shares shares proc)
      (let ((orig-count (cdr shares)))
        (fn ()
          (let ((new-count (cdr shares)))
            (cond
             ((> new-count orig-count)
              (hash-table-walk
               (car shares)
               (lambda (k v)
                 (if (and (cdr v) (>= (car v) orig-count))
                     (set-cdr! v #f))))
              (set-cdr! shares orig-count)))
            proc))))

    (define (pp-with-indent indent-rule ls pp shares)
      (fn ((col1 col))
        (each
         "("
         (pp (car ls))
         (fn ((col2 col) width string-width)
           (let ((fixed (take* (cdr ls) (or indent-rule 1)))
                 (tail (drop* (cdr ls) (or indent-rule 1)))
                 (default
                   (let ((sep (make-nl-space (+ col1 1))))
                     (each sep (joined/shares pp (cdr ls) shares sep))))
                 (reset-shares (with-reset-shares shares nothing)))
             (call-with-output
              (trimmed/lazy (- width col2)
                            (each " "
                                  (joined/shares
                                   (lambda (x) (pp-flat x pp shares)) fixed shares " "))
                            )
              (lambda (first-line)
                (cond
                 ((< (+ col2 (string-width first-line)) width)
                  (let ((sep (make-nl-space
                              (if indent-rule (+ col1 2) (+ col2 1)))))
                    (each first-line
                          (cond
                           ((not (or (null? tail) (pair? tail)))
                            (each ". " (pp tail)))
                           ((> (or (length+ (cdr ls)) +inf.0) (or indent-rule 1))
                            (each sep (joined/shares pp tail shares sep)))
                           (else
                            nothing)))))
                 (indent-rule
                  (try-fitted
                   (each
                    reset-shares
                    " "
                    (joined/shares pp fixed shares (make-nl-space (+ col2 1)))
                    (if (pair? tail)
                        (let ((sep (make-nl-space (+ col1 2))))
                          (each sep (joined/shares pp tail shares sep)))
                        nothing))
                   (each reset-shares default)))
                 (else
                  (each reset-shares default)))))))
         ")")))

    (define (pp-app ls pp shares)
      (let ((indent-rule (pp-indentation ls)))
        (if (procedure? indent-rule)
            (indent-rule ls pp shares)
            (pp-with-indent indent-rule ls pp shares))))

    (define (proper-non-shared-list? ls shares)
      (let ((tab (car shares)))
        (let lp ((ls ls))
          (or (null? ls)
              (and (pair? ls)
                   (not (hash-table-ref/default tab ls #f))
                   (lp (cdr ls)))))))

    (define (non-app? x)
      (if (pair? x)
          (or (not (or (null? (cdr x)) (pair? (cdr x))))
              (non-app? (car x)))
          (not (symbol? x))))

    (define (pp-data-list ls pp shares)
      (each
       "("
       (fn (col width string-width)
         (let ((avail (- width col)))
           (cond
            ((and (pair? (cdr ls)) (pair? (cddr ls)) (pair? (cdr (cddr ls)))
                  (fits-in-columns width ls (lambda (x) (pp-flat x pp shares))))
             => (lambda (ls)
                  (let* ((prefix (make-nl-space col))
                         (widest (+ 1 (car ls)))
                         (columns (quotient width widest)))
                    (let lp ((ls (cdr ls)) (i 1))
                      (cond
                       ((null? ls)
                        nothing)
                       ((null? (cdr ls))
                        (displayed (car ls)))
                       ((>= i columns)
                        (each (car ls)
                              prefix
                              (fn () (lp (cdr ls) 1))))
                       (else
                        (let ((pad (- widest (string-width (car ls)))))
                          (each (car ls)
                                (make-space pad)
                                (lp (cdr ls) (+ i 1))))))))))
            (else
             (joined/shares pp ls shares (make-nl-space col))))))
       ")"))

    (define (pp-flat x pp shares)
      (cond
       ((pair? x)
        (cond
         ((and (pair? (cdr x)) (null? (cddr x))
               (assq (car x) syntax-abbrevs))
          => (lambda (abbrev)
               (each (cdr abbrev)
                     (call-with-shared-ref
                      (cadr x)
                      shares
                      (pp-flat (cadr x) pp shares)))))
         (else
          (each "("
                (joined/shares (lambda (x) (pp-flat x pp shares)) x shares " ")
                ")"))))
       ((vector? x)
        (each "#("
              (joined/shares
               (lambda (x) (pp-flat x pp shares)) (vector->list x) shares " ")
              ")"))
       (else
        (pp x))))

    (define (pp-pair ls pp shares)
      (cond
       ((null? (cdr ls))
        (each "(" (pp (car ls)) ")"))
       ((and (pair? (cdr ls)) (null? (cddr ls))
             (assq (car ls) syntax-abbrevs))
        => (lambda (abbrev)
             (each (cdr abbrev) (pp (cadr ls)))))
       (else
        (try-fitted
         (fn () (pp-flat ls pp shares))
         (with-reset-shares
          shares
          (fn ()
            (if (and (non-app? ls)
                     (proper-non-shared-list? ls shares))
                (pp-data-list ls pp shares)
                (pp-app ls pp shares))))))))

    (define (pp-vector vec pp shares)
      (each "#" (pp-data-list (vector->list vec) pp shares)))

    (define (pp obj shares)
      (fn (radix precision)
        (let ((write-number
               (cond
                ((and (not precision)
                      (assv radix '((16 . "#x") (10 . "") (8 . "#o") (2 . "#b"))))
                 => (lambda (cell)
                      (lambda (n)
                        (if (or (exact? n) (eqv? radix 10))
                            (each (cdr cell) (number->string n (car cell)))
                            (with ((radix 10)) (numeric n))))))
                (else (lambda (n) (with ((radix 10)) (numeric n)))))))
          (let pp ((obj obj))
            (call-with-shared-ref
             obj shares
             (fn ()
               (cond
                ((pair? obj)
                 (pp-pair obj pp shares))
                ((vector? obj)
                 (pp-vector obj pp shares))
                ((number? obj)
                 (write-number obj))
                (else
                 (write-with-shares obj shares)))))))))

    (define (pretty obj)
      (fn ()
        (call-with-output
         (each (pp obj (extract-shared-objects obj #t))
               fl)
         displayed)))

    (define (pretty-simply obj)
      (fn ()
        (each (pp obj (extract-shared-objects #f #f))
              fl))))

  ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
  ;; color.scm — ANSI colour combinators.
  (begin

    (define (color->ansi x)
      (case x
        ((bold) "1")
        ((dark) "2")
        ((underline) "4")
        ((black) "30")
        ((red) "31")
        ((green) "32")
        ((yellow) "33")
        ((blue) "34")
        ((magenta) "35")
        ((cyan) "36")
        ((white) "37")
        (else "0")))

    (define (ansi-escape color)
      (string-append (string (integer->char 27)) "[" (color->ansi color) "m"))

    (define (colored new-color . args)
      (fn (color)
        (with ((color new-color))
          (each (ansi-escape new-color)
                (each-in-list args)
                (if (or (memq new-color '(bold underline))
                        (memq color '(bold underline)))
                    (ansi-escape 'reset)
                    nothing)
                (ansi-escape color)))))

    (define (as-red . args) (colored 'red (each-in-list args)))
    (define (as-blue . args) (colored 'blue (each-in-list args)))
    (define (as-green . args) (colored 'green (each-in-list args)))
    (define (as-cyan . args) (colored 'cyan (each-in-list args)))
    (define (as-yellow . args) (colored 'yellow (each-in-list args)))
    (define (as-magenta . args) (colored 'magenta (each-in-list args)))
    (define (as-white . args) (colored 'white (each-in-list args)))
    (define (as-black . args) (colored 'black (each-in-list args)))
    (define (as-bold . args) (colored 'bold (each-in-list args)))
    (define (as-underline . args) (colored 'underline (each-in-list args)))))
