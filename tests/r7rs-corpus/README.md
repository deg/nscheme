# R7RS-small conformance corpus

This directory holds the third-party conformance test suite from
[chibi-scheme](https://github.com/ashinn/chibi-scheme), vendored as
the de facto standard for R7RS-small implementations to report
against.

## Files

| File                          | Source                                  | License |
|-------------------------------|-----------------------------------------|---------|
| `chibi-r7rs-tests.scm`        | chibi-scheme `tests/r7rs-tests.scm`     | BSD 3-clause (see `COPYING-chibi-scheme`) |
| `COPYING-chibi-scheme`        | chibi-scheme `COPYING`                  | BSD 3-clause |
| `chibi-test-shim.scm`         | nscheme-original                        | MIT OR Apache-2.0 |

## How it's run

[`tests/r7rs_chibi.rs`](../r7rs_chibi.rs) loads
`chibi-test-shim.scm` (which defines the `(chibi test)` library that
the corpus imports) into a fresh nscheme env, then walks each
top-level datum of `chibi-r7rs-tests.scm`, evaluating each one inside
a `try`/`continue` so a single bad form doesn't abort the run. Pass
and fail counts come from the shim's `$passes` and `$fails`
counters.

```bash
cargo test --test r7rs_chibi -- --nocapture
```

## Expected gaps

The chibi corpus exercises corners of R7RS-small that nscheme v1 does
not yet implement, including:

- Complex numbers (`make-rectangular`, etc.)
- `eval`/the `(scheme eval)` library
- `case-lambda`
- `(scheme time)` (`current-second`, etc.)
- `(scheme process-context)` (`command-line`, etc.)
- Full `read` (we have `read-char`/`read-line` but not full-datum read)
- Various less-common base-library procedures

Failures in these categories are expected and listed in the bead
[`nscheme-i0p`](../../.beads/issues.jsonl). The harness records a
baseline pass/fail count so future changes can be measured against
the same corpus.

## License note

The chibi-scheme files are redistributed under their original BSD
3-clause license. Per the license, the chibi copyright notice
is preserved (see `COPYING-chibi-scheme`) and chibi-scheme is not
used to endorse or promote nscheme.
