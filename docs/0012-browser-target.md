# ADR 0012 — Running in a web browser (WebAssembly)

> **Update (2026-11-17, later):** programs no longer run on the page's main thread. They run in a Web Worker, which Stop and crash recovery replace, and their output streams as it is printed; see [0013 — The browser playground](0013-browser-playground.md). The main-thread limitation and the re-import after a trap, described under Consequences, are history. Bead: nscheme-yd0.2.2.

> **Update (2026-11-17):** file access now follows the same pattern as the console sink (§3). Every file operation a program can cause goes through a replaceable per-thread filesystem (`files::set_file_system`): `open-input-file`, the write on `close-port`, `file-exists?`, `delete-file`, `load`, `include` and the library loader's disk lookup. It defaults to `StdFileSystem`, the disk. `MemoryFileSystem` holds text files keyed by lexically normalized path, and reports each write and delete to an optional observer. This is what the browser playground (nscheme-yd0.2) builds its persistent files on. So "file ports fail with `Unsupported`" below holds only while no other filesystem is installed. Bead: nscheme-yd0.2.1.

**Status:** Accepted (2026-11-16) **Bead:** nscheme-yd0.1 (Minimal browser demo), epic nscheme-yd0 (Run nscheme in a web browser via WebAssembly) **Related:** [0007 — Filesystem-loaded libraries](0007-filesystem-library-loader.md), [0010 — Current ports as parameters](0010-current-ports-as-parameters.md), [0011 — Error diagnostics](0011-error-diagnostics.md)

## Context

We wanted nscheme to run in a web page, compiled to `wasm32-unknown-unknown`. A trial build showed the evaluator was already portable: no threads, no `unsafe`, pure-Rust numeric crates. With `rustyline` removed, the library compiled for wasm unchanged. Compiling was not the problem, though. These parts compiled but would fail at runtime in a browser:

- **Libraries.** `(import (scheme list))` reads `lib/scheme/list.sld` from disk ([0007]), and a browser has no filesystem.
- **Console output.** The stdout port called `print!`, which in wasm writes nowhere.
- **The clock.** `std::time::SystemTime::now()` *panics* on `wasm32-unknown-unknown`.
- **`exit`.** `std::process::exit` traps, which kills the wasm instance.
- **`rustyline`.** It's a CLI-only dependency, but a dependency of the whole crate, and it doesn't compile for wasm.

The rest already behaves acceptably in a browser: `std::fs` returns `Unsupported`, so file ports fail with a catchable `file-error?` condition, and stdin reads return end-of-file. Keyboard input and a file system are later beads.

## Decision

The browser-specific work lives **in the library, on every target**, so native `cargo test` exercises it. A separate thin crate handles only the JavaScript boundary.

### 1. `lib/` is compiled into the binary, as the last fallback

`build.rs` walks `lib/` and writes a table of `(path, include_str!(…))` pairs, sorted by path string for binary search. `library.rs` tries every disk directory first, then the table. Disk wins, so editing a `.sld` file still takes effect without a rebuild. The table backs both `try_load_library` and `library_findable`, so `cond-expand`'s `(library …)` test agrees with `import`. `set_search_path(vec![])` now turns off *disk* lookup only.

The table is compiled into the native binary too (about 816K, by author direction). An installed binary therefore no longer depends on `lib/` staying where it was built.

### 2. `exit` is a result, not a process exit

`exit` and `emergency-exit` return `RuntimeError::Exit(code)`. In `step_apply`, every other primitive error is turned into a `raise` so handlers can catch it ([0005]). `Exit` is matched *before* that conversion and becomes `EvalError::Exit(code)`, which leaves the eval loop directly. So `guard` and `with-exception-handler` cannot catch it. Nested evaluations (library loads, `load`) propagate it with `?`. `force` is the one Rust primitive that runs Scheme code with its own nested `eval`, and it passes `Exit` through instead of wrapping it as an error (its other errors still lose the raised object: nscheme-rsh, force turns a raise inside a promise into an error string).

The host decides what exiting means. The CLI maps it to the process status in `-e`, file runs, `-i` and the REPL, so `(exit 3)` typed at the REPL ends the session with status 3. The browser wrapper reports it as the run's result.

Like the old `process::exit`, it does not run pending `dynamic-wind` after-thunks, and it does not unwind `parameterize`. A host that keeps running after an exit must therefore call `io::reset_current_ports()`.

### 3. Console output goes through a replaceable sink

The stdout and stderr ports write through a per-thread `ConsoleSink` callback (`io::set_console_sink`), which defaults to `print!` / `eprint!`. The wrapper installs one that collects text. It is a callback rather than a buffer so that streaming output (tier 2, nscheme-yd0.2) needs no new mechanism. String and file ports, and `parameterize`d redirection ([0010]), are unaffected.

### 4. The clock comes from `web-time`

`current-second` and `current-jiffy` use the `web-time` crate. Natively it re-exports `std::time`; in a browser it reads `performance.now()`. It costs one small dependency and needs no `cfg`.

### 5. A separate `web/` crate holds the wasm-bindgen glue

`nscheme-web` is a Cargo workspace member and a `cdylib`. It's separate because `#[wasm_bindgen]` generates `unsafe` code and the main crate has `unsafe_code = "forbid"`. Its API is small:

- `new Interpreter()` creates a top-level environment.
- `run(source)` returns `{output, value, error, exitCode}`. Before each run it resets the current ports and any leftover backtrace.

Error text comes from `diagnostic::format_error`, which moved out of `main.rs` ([0011]), so the browser shows the same `line:col`, caret and call trace as the CLI.

Building and testing:

- `web/build.sh` builds the crate and runs the `wasm-bindgen` CLI. The CLI's version must equal the pinned `wasm-bindgen = "=…"` crate version, and the script checks that.
- `web/smoke.mjs` loads the built module in Node and checks what only the wasm build can get wrong.
- GitHub Pages builds and deploys `web/www/` from CI (`.github/workflows/pages.yml`). Built files are never committed.

### 6. The wasm stack is 8MB

The reader and printer recurse once per level of nesting. Wasm's default 1MB stack overflowed at about 2,200 levels of nested list (native release: about 9,500, on an 8MB main thread). `.cargo/config.toml` links the wasm target with `-zstack-size=8388608`. In V8 that raised the limit to about 4,800 levels, where the JS engine's own call stack runs out first, and a page cannot raise that. The real fix is a reader that doesn't recurse, which also helps native (nscheme-m6i, Deeply nested data overflows the stack in the reader).

## Consequences

### Positive

- The same library runs natively and in a browser. Every browser-specific behaviour except the JS clock is covered by native tests (`tests/browser_support.rs`, the wrapper's unit tests).
- `exit` is now testable at all. Previously a test calling it would have killed the test process.
- Embedders get two general hooks: capturing console output, and handling `exit` themselves.

### Negative / limitations

- The program runs on the page's main thread, so a long or infinite loop freezes the tab, and nothing can interrupt it. A Web Worker and a Stop button are nscheme-yd0.2 (Browser playground: run in a Web Worker with a Stop button).
- There is no keyboard input. Console reads see end-of-file. Fixing that is nscheme-yd0.3 (Browser stdin: let read-line wait for typed input).
- A Rust panic or stack overflow traps the instance. The demo page re-imports a fresh module afterwards, and everything defined in the old one is lost.
- The binary is about 816K larger on every target. The release `.wasm` is about 1.7MB (about 460K gzipped).
- `exit` skips `dynamic-wind` after-thunks, as it always did. R7RS asks for them to run; doing that would mean unwinding the frame stack through the `WindJump` machinery ([0004]).

[0004]: 0004-continuations.md
[0005]: 0005-exception-handling.md
[0007]: 0007-filesystem-library-loader.md
[0010]: 0010-current-ports-as-parameters.md
[0011]: 0011-error-diagnostics.md
