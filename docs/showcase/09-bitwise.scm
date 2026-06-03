;; 09-bitwise.scm — a full bit-twiddling toolkit  (SRFI 151)
;;
;; What's cool: arbitrary-precision integers AND a complete bitwise vocabulary
;; — popcount, bit-fields, shifts — that work on bignums, not just machine
;; words. `bit-count` (population count) and `bit-field` (extract a slice of
;; bits) are the kind of primitives you usually drop to C for. Here they're
;; standard library, and they never overflow.

(import (scheme base) (scheme write) (scheme bitwise))

(display
  (list
    (bit-count 255)           ; 8   — number of 1 bits (popcount)
    (bitwise-and 12 10)       ; 8   — 1100 AND 1010
    (arithmetic-shift 1 8)    ; 256 — 1 << 8
    (bit-field 255 2 6)       ; 15  — bits [2,6) of 11111111 = 1111
    (bit-count (expt 2 100))  ; 1   — popcount of a 101-bit bignum, no overflow
    ))
(newline)
