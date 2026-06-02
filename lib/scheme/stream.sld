;;; (scheme stream) — R7RS-large (Red Edition) name for SRFI 41.
;;;
;;; The Red Edition adopts SRFI 41 under the name (scheme stream); this
;;; library is a thin re-export of our vendored (srfi 41).
(define-library (scheme stream)
  (import (srfi 41))
  (export stream-null stream-cons stream? stream-null? stream-pair? stream-car
          stream-cdr stream-lambda define-stream list->stream port->stream stream
          stream->list stream-append stream-concat stream-constant stream-drop
          stream-drop-while stream-filter stream-fold stream-for-each stream-from
          stream-iterate stream-length stream-let stream-map stream-match
          stream-of stream-range stream-ref stream-reverse stream-scan stream-take
          stream-take-while stream-unfold stream-unfolds stream-zip))
