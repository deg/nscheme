;;; (srfi 64) — A Scheme API for test suites.
;;;
;;; A self-contained implementation for nscheme covering the forms the
;;; SRFI reference test suites actually use, plus chibi-`test`-style
;;; aliases (`test`, `test-exit`) so suites written against either
;;; framework run unmodified. State lives in a <test-runner> record
;;; reachable via test-runner-current / test-runner-get, so a driver can
;;; read test-runner-fail-count after running a suite.
(define-library (srfi 64)
  (export
   ;; lifecycle
   test-begin test-end test-group test-group-with-cleanup
   ;; assertions
   test-assert test-equal test-eqv test-eq test-approximate
   test-error test-read-error test-not
   ;; skip / expected failure
   test-skip test-expect-fail
   ;; runner
   test-runner? test-runner-current test-runner-get test-runner-create
   test-runner-simple test-runner-null test-runner-factory
   test-runner-reset test-runner-pass-count test-runner-fail-count
   test-runner-xpass-count test-runner-xfail-count test-runner-skip-count
   test-runner-test-name test-runner-group-path test-runner-group-stack
   test-with-runner
   ;; chibi-compat
   test test-exit current-test-comparator)
  (import (scheme base) (scheme write) (scheme complex) (scheme inexact))
  (begin

    (define-record-type <test-runner>
      (%mk pass fail xpass xfail skip groups skips fails name)
      test-runner?
      (pass   tr-pass   tr-pass!)
      (fail   tr-fail   tr-fail!)
      (xpass  tr-xpass  tr-xpass!)
      (xfail  tr-xfail  tr-xfail!)
      (skip   tr-skipc  tr-skipc!)
      (groups tr-groups tr-groups!)   ; stack of group names
      (skips  tr-skips  tr-skips!)    ; active test-skip specifiers
      (fails  tr-xfails tr-xfails!)   ; active test-expect-fail specifiers
      (name   tr-name   tr-name!))    ; name of the test in progress

    (define (test-runner-create) (%mk 0 0 0 0 0 '() '() '() #f))
    (define test-runner-simple test-runner-create)
    (define test-runner-null test-runner-create)

    (define %current #f)
    (define %factory test-runner-create)
    (define (test-runner-get)
      (or %current (error "test-runner-get: no runner; call test-begin first")))
    (define (test-runner-current . a)
      (if (pair? a) (begin (set! %current (car a)) (car a)) %current))
    (define (test-runner-factory . a)
      (if (pair? a) (begin (set! %factory (car a)) (car a)) %factory))

    (define (test-runner-pass-count r) (tr-pass r))
    (define (test-runner-fail-count r) (tr-fail r))
    (define (test-runner-xpass-count r) (tr-xpass r))
    (define (test-runner-xfail-count r) (tr-xfail r))
    (define (test-runner-skip-count r) (tr-skipc r))
    (define (test-runner-test-name r) (or (tr-name r) ""))
    (define (test-runner-group-stack r) (tr-groups r))
    (define (test-runner-group-path r) (reverse (tr-groups r)))
    (define (test-runner-reset r)
      (tr-pass! r 0) (tr-fail! r 0) (tr-xpass! r 0) (tr-xfail! r 0)
      (tr-skipc! r 0) (tr-groups! r '()) (tr-skips! r '()) (tr-xfails! r '())
      (tr-name! r #f))

    ;; ---- lifecycle ----------------------------------------------------
    (define (test-begin . args)
      (unless %current (set! %current (%factory)))
      (when (pair? args)
        (tr-groups! %current (cons (car args) (tr-groups %current)))))

    (define (test-end . args)
      (let ((r (test-runner-get)))
        (when (pair? (tr-groups r)) (tr-groups! r (cdr (tr-groups r))))
        (when (null? (tr-groups r))
          (display "# pass ") (display (tr-pass r))
          (display " / fail ") (display (tr-fail r))
          (when (> (tr-skipc r) 0)
            (display " / skip ") (display (tr-skipc r)))
          (newline))))

    (define-syntax test-group
      (syntax-rules ()
        ((_ name decl ...)
         (begin (test-begin name) decl ... (test-end name)))))

    (define-syntax test-group-with-cleanup
      (syntax-rules ()
        ((_ name body ... cleanup)
         (begin (test-begin name) body ... cleanup (test-end name)))))

    (define-syntax test-with-runner
      (syntax-rules ()
        ((_ runner body ...)
         (let ((saved %current))
           (set! %current runner)
           (let ((result (begin body ...)))
             (set! %current saved)
             result)))))

    ;; ---- skip / expect-fail matching ----------------------------------
    ;; A specifier is a count (skip the next N), a string (match the test
    ;; name), or a predicate on the runner. Returns #t if `name` is
    ;; matched, consuming counts as it goes.
    (define (consume-spec! r get-specs set-specs! name)
      (let loop ((specs (get-specs r)) (kept '()) (hit #f))
        (if (null? specs)
            (begin (set-specs! r (reverse kept)) hit)
            (let ((s (car specs)) (rest (cdr specs)))
              (cond
               ((and (integer? s) (> s 1)) (loop rest (cons (- s 1) kept) #t))
               ((integer? s) (loop rest kept #t)) ; last of the count, drop it
               ((string? s)
                (loop rest (cons s kept) (or hit (and name (string=? s name)))))
               ((procedure? s) (loop rest (cons s kept) (or hit (s r))))
               (else (loop rest (cons s kept) hit)))))))

    (define (test-skip spec)
      (let ((r (test-runner-get))) (tr-skips! r (cons spec (tr-skips r)))))
    (define (test-expect-fail spec)
      (let ((r (test-runner-get))) (tr-xfails! r (cons spec (tr-xfails r)))))

    ;; ---- comparison ---------------------------------------------------
    (define (approx=? a b tol)
      (cond ((and (real? a) (real? b) (nan? a) (nan? b)) #t)
            ((and (real? a) (real? b)) (<= (abs (- a b)) tol))
            (else (and (approx=? (real-part a) (real-part b) tol)
                       (approx=? (imag-part a) (imag-part b) tol)))))

    ;; ---- the core: run one test, tally the outcome --------------------
    (define (raised? thunk)
      (call/cc
       (lambda (k)
         (with-exception-handler
          (lambda (e) (k #t))
          (lambda () (thunk) #f)))))

    (define (run-test name pass?-thunk)
      (let ((r (test-runner-get)))
        (tr-name! r name)
        (cond
         ((consume-spec! r tr-skips tr-skips! name)
          (tr-skipc! r (+ 1 (tr-skipc r))))
         (else
          (let* ((expect-fail (consume-spec! r tr-xfails tr-xfails! name))
                 (ok (call/cc
                      (lambda (k)
                        (with-exception-handler
                         (lambda (e) (k #f))
                         (lambda () (and (pass?-thunk) #t)))))))
            (cond
             ((and ok expect-fail) (tr-xpass! r (+ 1 (tr-xpass r))))
             ((and (not ok) expect-fail) (tr-xfail! r (+ 1 (tr-xfail r))))
             (ok (tr-pass! r (+ 1 (tr-pass r))))
             (else
              (tr-fail! r (+ 1 (tr-fail r)))
              (display "FAIL: ") (write (or name "")) (newline))))))))

    ;; ---- assertion forms ----------------------------------------------
    (define-syntax test-assert
      (syntax-rules ()
        ((_ name expr) (run-test name (lambda () expr)))
        ((_ expr)      (run-test #f   (lambda () expr)))))

    (define-syntax test-not
      (syntax-rules ()
        ((_ name expr) (run-test name (lambda () (not expr))))
        ((_ expr)      (run-test #f   (lambda () (not expr))))))

    (define-syntax %test-cmp
      (syntax-rules ()
        ((_ pred name expected expr)
         (run-test name (lambda () (pred expected expr))))
        ((_ pred expected expr)
         (run-test #f   (lambda () (pred expected expr))))))

    (define-syntax test-equal
      (syntax-rules ()
        ((_ a b c) (%test-cmp equal? a b c))
        ((_ a b)   (%test-cmp equal? a b))))
    (define-syntax test-eqv
      (syntax-rules ()
        ((_ a b c) (%test-cmp eqv? a b c))
        ((_ a b)   (%test-cmp eqv? a b))))
    (define-syntax test-eq
      (syntax-rules ()
        ((_ a b c) (%test-cmp eq? a b c))
        ((_ a b)   (%test-cmp eq? a b))))

    (define-syntax test-approximate
      (syntax-rules ()
        ((_ name expected expr err)
         (run-test name (lambda () (approx=? expected expr err))))
        ((_ expected expr err)
         (run-test #f   (lambda () (approx=? expected expr err))))))

    (define-syntax test-error
      (syntax-rules ()
        ((_ name etype expr) (run-test name (lambda () (raised? (lambda () expr)))))
        ((_ etype expr)      (run-test #f   (lambda () (raised? (lambda () expr)))))
        ((_ expr)            (run-test #f   (lambda () (raised? (lambda () expr)))))))

    (define-syntax test-read-error
      (syntax-rules ()
        ((_ str) (run-test #f (lambda () (raised? (lambda () (read (open-input-string str)))))))))

    ;; ---- chibi-test compatibility -------------------------------------
    ;; chibi's `(test expected expr)` / `(test name expected expr)`.
    ;; chibi's `test` compares with a parameterizable predicate
    ;; (default equal?); suites parameterize it, e.g. to set=?.
    (define current-test-comparator (make-parameter equal?))
    (define-syntax test
      (syntax-rules ()
        ((_ name expected expr)
         (run-test name (lambda () ((current-test-comparator) expected expr))))
        ((_ expected expr)
         (run-test #f (lambda () ((current-test-comparator) expected expr))))))
    (define (test-exit . _)
      (when %current
        (let ((r %current))
          (display "# total pass ") (display (tr-pass r))
          (display " fail ") (display (tr-fail r)) (newline))))))
