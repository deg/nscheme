# nscheme showcase

A baker's dozen of tiny programs (each well under 25 lines) that show off
the R7RS-large libraries nscheme ships. They're picked to surprise someone
who already knows Common Lisp and Clojure — not "look, a `for` loop," but
the things those languages make you reach outside the standard library for:
infinite lazy sieves, persistent ordered maps, regexes that are data,
gamma functions, composable formatters.

Every program — and every REPL line below — has been run; the outputs shown
are real.

## Running them

From the repository root (`nscheme/`), build once:

```sh
cargo build                                   # produces target/debug/nscheme
```

The interpreter finds the bundled `lib/` automatically (the search path is
baked in at build time). If you've moved the binary or the libraries, put
`NSCHEME_LIB_PATH=lib` in front of any command below.

### Explore interactively (recommended)

The fun in these isn't the output that scrolls past — it's poking at the
pieces. Load a program with `-i` and you *stay* in the REPL afterward, with
all of its definitions **and its imports** live:

```console
$ ./target/debug/nscheme -i docs/showcase/01-receipt.scm
…the receipt prints…
> (show #f (money 4999))      ; reuse the file's own `money` formatter
"$49.99"
```

Pass several files to stack their definitions. Already in a REPL? The
`load` procedure does the same thing, and it evaluates its filename
argument, so computed paths work:

```scheme
> (load "docs/showcase/05-comparators.scm")
> (comparator? ranked)        ; `ranked` is now defined
#t
```

Each program below ends with a **Try** line: one expression to start with
once you've `-i`-loaded that file.

### Just run one, or all of them

```sh
./target/debug/nscheme docs/showcase/01-receipt.scm

for f in docs/showcase/*.scm; do
  echo "=== $f ==="; ./target/debug/nscheme "$f"
done
```

## The programs

**01 · `01-receipt.scm` — show (159).**
Formatting is composable first-class values — no `FORMAT` string DSL. The
layout engine auto-aligns columns from declared widths.
*Try:* `(show #f (money 4999))` → `"$49.99"`

**02 · `02-prime-sieve.scm` — streams (41).**
A genuinely *infinite* Sieve of Eratosthenes; ask for any prime on demand.
*Try:* `(stream-ref primes 49)` → `229` (the 50th prime)

**03 · `03-generators.scm` — generators (158).**
Lazy map/filter/take pipelines — and accumulators, their exact mirror image.
*Try:* `(generator->list (gtake (make-iota-generator 10 1 3) 4))` → `(1 4 7 10)`

**04 · `04-persistent-map.scm` — mapping (146).**
A persistent map that's also *ordered*; updates return new maps and keys
come out sorted.
*Try:* `(mapping->alist (mapping-set m0 "date" 1))` → adds `("date" . 1)`, still sorted

**05 · `05-comparators.scm` — comparator (128).**
Ordering is a first-class object bundling `=`, `<`, and hash; build a
composite order once and reuse it.
*Try:* `(list-sort (comparator-ordering-predicate ranked) '("xyz" "a" "to" "be"))` → `("a" "be" "to" "xyz")`

**06 · `06-regex-sre.scm` — regex (115).**
Regular expressions written as S-expressions — patterns are data, not
backslash soup.
*Try:* `(regexp-match-submatch (regexp-search date "2038-01-19") 2)` → `"01"`

**07 · `07-char-sets.scm` — charset (14).**
Character classes are real sets: "consonants" = letters minus vowels, as
set algebra.
*Try:* `(char-set-contains? consonants #\z)` → `#t`

**08 · `08-sets.scm` — set (113).**
A proper Set type with union / intersection / difference, parameterized by
a comparator.
*Try:* `(set->list (set-difference b a))` → `(6 5)`

**09 · `09-bitwise.scm` — bitwise (151).**
popcount, bit-fields, shifts — on arbitrary-precision bignums that never
overflow.
*Try:* `(bit-count (- (expt 2 16) 1))` → `16`

**10 · `10-ideque.scm` — ideque (134).**
A persistent double-ended queue; O(1) at both ends, every op returns a new
deque.
*Try:* `(palindrome? "level")` → `#t`

**11 · `11-flonum.scm` — flonum (144).**
The gamma, error, and Bessel functions in the *standard* library (the
SciPy stuff).
*Try:* `(flgamma 0.5)` → `1.772453850905516` (that's √π)

**12 · `12-numeric-format.scm` — show (159).**
Thousands separators, SI suffixes, radix, padding — each a named combinator.
*Try:* `(show #f (numeric/comma 1234567890))` → `"1,234,567,890"`

**13 · `13-word-histogram.scm` — hash-table + sort + show (125 / 132 / 159).**
Capstone: count, rank, and draw an aligned bar chart in a dozen lines.
*Try:* `(words "fee fie foe fum")` → `("fee" "fie" "foe" "fum")`

## A note on what's *not* fast

nscheme is a tree-walking interpreter (no bytecode VM yet), so these are
toys, not benchmarks. The infinite sieve in `02` computes the 100th prime
happily; don't ask it for the millionth. The point is expressiveness — how
much these libraries let you say in a handful of lines — not throughput.
