# ADR 0010 — Current ports as parameters

**Status:** Accepted (2026-06-07) **Bead:** [nscheme-oge](../.beads/issues.jsonl) **Related:** [0005 — Exception handling](0005-exception-handling.md) (parameter objects / `parameterize`)

## Context

`current-input-port`, `current-output-port`, and `current-error-port` were plain procedures that returned fixed `$stdin` / `$stdout` / `$stderr` port objects. That made the R7RS §6.13 **redirection family impossible**:

- `(parameterize ((current-output-port p)) …)` errored with "first item must be a parameter object" — the accessor wasn't a parameter.
- `with-output-to-file`, `with-input-from-file`, `with-output-to-string`, `with-input-from-string` all build on parameterizing the current port, so none could exist.

Surfaced by the `nscheme-488` audit; tracked as `nscheme-oge`.

## Decision

Make the three accessors real **parameter objects** (`Procedure::Parameter`), and have the I/O primitives consult them for their default port.

### One shared set of cells per thread

A thread-local singleton ([`CURRENT_PORTS`](../src/io.rs)) holds the three `ParameterCell`s plus the canonical stdio port values. It is created once and bound into **every** environment's `install_base` — shared, **not** per-env. This mirrors the `PRIMITIVE_CACHE` decision in `builtins.rs`.

The sharing is essential, not incidental: `parameterize` rebinds the cell that `current-output-port` *names in the user's environment*, while the I/O primitives read a cell to find their default port. Those must be the **same** cell, or redirection silently does nothing. A singleton also guarantees the program environment and the hermetic library-loader root agree on where output goes.

### Primitives read the cells for their default port

- `write_to_port(None, …)` (the no-port path shared by `display` / `write` / `newline` / `write-char` / `write-string`) routes to the current value of `current-output-port`.
- The no-argument forms of `read` / `read-char` / `read-line` / `peek-char` read from `current-input-port`.

So `parameterize`-ing a current port redirects all default I/O for its dynamic extent. The redirection family is then a few lines of Scheme bootstrap (`with-output-to-file` = `call-with-output-file` + `parameterize`, etc.).

## Consequences

### Positive

- The full §6.13 redirection family works: `with-output-to-file` / `-from-file` / `with-output-to-string` / `with-input-from-string`, plus direct `parameterize` on a current port. Redirection is correctly dynamically-scoped (restored on extent exit) and nests.
- `char-ready?` / `u8-ready?` now validate their (optional) port argument and default to `current-input-port`.
- The default-port code path now flows through the same `Port::StdIn` handling as explicit ports, so reading from real stdin gained proper UTF-8 line buffering (the bespoke byte-wise `*_from_stdin` helpers were removed).

### Negative

- The current ports are thread-global dynamic state. This is inherent to parameters (they *are* dynamic), and `parameterize`'s save/restore keeps it disciplined, but a primitive's default port is no longer a compile-time constant.
- There is no `with-error-to-*` form (R7RS defines none); redirecting `current-error-port` requires explicit `parameterize`.
