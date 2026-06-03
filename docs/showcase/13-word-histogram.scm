;; 13-word-histogram.scm — the capstone: four libraries in one tiny program
;;
;; What's cool: this is everyday data work, and each tool slots in cleanly:
;;   - (scheme hash-table)  count words with update!/default
;;   - (scheme sort)        order by frequency
;;   - (scheme show)        render an auto-aligned bar chart with columnar
;;   - (srfi 1)             list glue
;; The whole pipeline — count, rank, draw — is a dozen lines, and the bar
;; chart's three columns align themselves from declared widths. No FORMAT,
;; no manual padding, no plotting library.

(import (scheme base) (scheme show) (scheme comparator)
        (scheme hash-table) (scheme sort) (srfi 1))

(define (words str)                        ; split on spaces (no SRFI-13 needed)
  (let loop ((cs (string->list str)) (cur '()) (acc '()))
    (define (flush a) (if (null? cur) a (cons (list->string (reverse cur)) a)))
    (cond ((null? cs) (reverse (flush acc)))
          ((char=? (car cs) #\space) (loop (cdr cs) '() (flush acc)))
          (else (loop (cdr cs) (cons (car cs) cur) acc)))))

(define text "the cat sat on the mat the cat ate the rat the cat ran")

(define counts (make-hash-table (make-default-comparator)))
(for-each (lambda (w) (hash-table-update!/default counts w (lambda (n) (+ n 1)) 0))
          (words text))

(define rows (list-sort (lambda (a b) (> (cdr a) (cdr b)))
                        (hash-table->alist counts)))

(show #t                                   ; word | count | bar — all aligned
  (columnar
    5      (each-in-list (map (lambda (r) (each (car r) nl)) rows))
    'right 2 (each-in-list (map (lambda (r) (each (cdr r) nl)) rows))
    1      (each-in-list (map (lambda (r) (each " " nl)) rows))
    (each-in-list (map (lambda (r) (each (make-string (cdr r) #\█) nl)) rows))))
