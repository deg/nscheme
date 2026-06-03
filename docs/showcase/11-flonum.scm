;; 11-flonum.scm — a numerics library with the special functions  (SRFI 144)
;;
;; What's cool: the gamma function, the error function, and Bessel functions
;; ship in the *standard* library. These are the functions you'd normally
;; pull in SciPy or Boost for. `flgamma 5.0` is 4! = 24; `flerf` is the
;; statistician's error function; `flfirst-bessel` is J0. A Lisp that knows
;; its analysis.

(import (scheme base) (scheme write) (scheme flonum))

(display
  (list
    (flgamma 5.0)            ; 24.0  — Γ(5) = 4!
    (flerf 1.0)             ; 0.8427... — the error function erf(1)
    (flfirst-bessel 0 1.0)  ; 0.7651... — Bessel J0(1)
    (flerfc 2.0)            ; 0.0046... — complementary error function erfc(2)
    ))
(newline)
