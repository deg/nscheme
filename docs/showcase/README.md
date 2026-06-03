# nscheme showcase

A baker's dozen of tiny programs (each well under 25 lines) that show off
the R7RS-large libraries nscheme ships. They're picked to surprise someone
who already knows Common Lisp and Clojure — not "look, a `for` loop," but
the things those languages make you reach outside the standard library for:
infinite lazy sieves, persistent ordered maps, regexes that are data,
gamma functions, composable formatters.

Every program here has been run and produces the output shown in its
comments.

## Running them

From the repository root (`nscheme/`):

```sh
cargo build                                   # build target/debug/nscheme
./target/debug/nscheme docs/showcase/01-receipt.scm
```

The interpreter finds the `lib/` libraries automatically (the default
search path is baked in at build time). If you've moved the binary or the
libraries, point it explicitly:

```sh
NSCHEME_LIB_PATH=lib ./target/debug/nscheme docs/showcase/01-receipt.scm
```

Run the whole set:

```sh
for f in docs/showcase/*.scm; do
  echo "=== $f ==="; ./target/debug/nscheme "$f"
done
```

### Poke at one in the REPL

To run a file and then *stay* in the REPL with all its definitions live,
pass `-i` (one or more files):

```sh
./target/debug/nscheme -i docs/showcase/01-receipt.scm
> (show #f (money 4999))      ; reuse the file's `money` formatter
"$49.99"
```

Or load from inside a running REPL with the `load` procedure — its
filename is evaluated, so computed paths work:

```sh
> (load "docs/showcase/05-comparators.scm")
> (comparator? ranked)        ; `ranked` is now defined
#t
```

## The programs

| # | File | Library (SRFI) | The surprise |
|---|------|----------------|--------------|
| 01 | `01-receipt.scm` | show (159) | Formatting is composable first-class values — no `FORMAT` string DSL. Columns auto-align. |
| 02 | `02-prime-sieve.scm` | streams (41) | A genuinely *infinite* Sieve of Eratosthenes; ask for the 100th prime on demand. |
| 03 | `03-generators.scm` | generators (158) | Lazy map/filter/take pipelines — and accumulators, their exact mirror image. |
| 04 | `04-persistent-map.scm` | mapping (146) | A persistent map that's also *ordered*; updates return new maps, keys come out sorted. |
| 05 | `05-comparators.scm` | comparator (128) | Ordering is a first-class object bundling =, <, and hash; build a composite order once. |
| 06 | `06-regex-sre.scm` | regex (115) | Regular expressions written as S-expressions — patterns are data, not backslash soup. |
| 07 | `07-char-sets.scm` | charset (14) | Character classes are real sets: "consonants" = letters minus vowels, as set algebra. |
| 08 | `08-sets.scm` | set (113) | A proper Set type with union/intersection/difference, parameterized by a comparator. |
| 09 | `09-bitwise.scm` | bitwise (151) | popcount, bit-fields, shifts — on arbitrary-precision bignums that never overflow. |
| 10 | `10-ideque.scm` | ideque (134) | A persistent double-ended queue; O(1) at both ends, every op returns a new deque. |
| 11 | `11-flonum.scm` | flonum (144) | The gamma, error, and Bessel functions in the *standard* library (the SciPy stuff). |
| 12 | `12-numeric-format.scm` | show (159) | Thousands separators, SI suffixes, radix, padding — each a named combinator. |
| 13 | `13-word-histogram.scm` | hash-table + sort + show (125/132/159) | Capstone: count, rank, and draw an aligned bar chart in a dozen lines. |

## A note on what's *not* fast

nscheme is a tree-walking interpreter (no bytecode VM yet), so these are
toys, not benchmarks. The infinite sieve in `02` computes the 100th prime
happily; don't ask it for the millionth. The point is expressiveness — how
much these libraries let you say in a handful of lines — not throughput.
