# ADR 0013 — The browser playground: a Worker, streamed output, Stop

> **Update (2026-11-17, nscheme-yd0.2.5): versioned Worker files.** GitHub Pages serves every file with `cache-control: max-age=600`. After the REPL deploy, a browser loaded the new page with a cached Worker from the previous deploy, which spoke the older protocol. That Worker ignored the page's `eval` messages, so the first entry hung forever. A hard reload didn't help, since the Worker's own fetches don't get the page's cache bypass; a private window did. Two fixes:
> - `web/build.sh` writes `pkg/build.json`, holding a hash of `worker.js`, the wasm glue and the wasm. The page fetches it uncached and loads `worker.js?v=<id>`, and the Worker loads the glue and the wasm with the same `?v=`. Each build's files therefore have their own URLs, and no cache can mix builds.
> - As a backstop, the Worker reports a protocol number in `ready` and answers any request it doesn't understand. If the page and the Worker disagree, the page says they are out of step and asks for a reload, instead of hanging.
>
> The browser test checks both: the URLs the page requests carry the build id, and an old-protocol Worker, which its test server can serve on demand, is reported.

> **Update (2026-11-17, nscheme-yd0.2.3):** the page is now a REPL session. The input box sits below the transcript, and every entry is evaluated in the same environment, which the Worker keeps for its whole life, so definitions build up. The `run` message became `eval`. A new `reset` message gives a fresh environment and calls `library::forget_user_libraries`: libraries the user supplied (saved files, inline `define-library`) are forgotten so they reload, while the standard ones stay cached, because reloading those costs up to a second. A three-level Reset menu was considered and judged over-engineered; Stop remains the full restart.
>
> What Reset does not undo: imported bindings share the library's own cells ([0006]), so an entry that changes a standard library (`(set! iota 5)` after importing `(scheme list)`) changes the cached library, and the change outlasts Reset. This is documented in the Reset tooltip rather than fixed, by David's choice: it is rare, and undoing it would mean reloading the standard libraries on every Reset. Stop is the full restart. `exit` now ends the session the way Reset does (fresh definitions, user libraries forgotten), but keeps the transcript, matching the CLI REPL, where `exit` ends the session.
>
> The page now has two output limits: at most 1,000,000 characters per entry, and 2,000,000 for the whole transcript, beyond which the oldest output is removed and a marker says so. Without the second, a few stopped floods added up to several megabytes again.
>
> Enter evaluates only when the cursor is at the end of the entry and its parentheses balance; anywhere else it adds a line, because an example or a paste is already balanced and Enter in the middle of one must not evaluate it. The page checks that itself (`isComplete`, which skips strings, comments, character literals and `|symbols|`), the same rule the CLI REPL applies to its input. Entries are echoed into the transcript, which scrolls inside its box and follows the end. The browser test grew to 24 steps: definitions accumulate, the Enter rule, history (with edits kept), Stop forgets definitions, two floods stay bounded, Reset clears the transcript, definitions and user libraries, and `exit` resets the session.

**Status:** Accepted (2026-11-17) **Bead:** nscheme-yd0.2.2 (Browser demo runs in a Web Worker with a Stop button), parent nscheme-yd0.2 (Browser playground) **Related:** [0012 — Running in a web browser](0012-browser-target.md)

## Context

The tier-1 demo ([0012]) ran each program on the page's main thread, as one synchronous call into wasm. That had three consequences:
- A long or endless program froze the tab, and nothing could interrupt it.
- Output appeared only when the run ended.
- A trap (a Rust panic or a stack overflow) left the module unusable, so the page re-imported a fresh copy under a new `?instance=` query, leaking the old one.

A wasm call can't be interrupted from JavaScript on the same thread, and the interpreter has no cooperative yield point. So the only way to stop a run is to stop the thread it's on.

## Decision

### 1. The interpreter lives in a Web Worker

`web/www/worker.js` is a module Worker that loads the wasm package and runs programs; the page never runs Scheme itself. The protocol is small:
- page to worker: `{type: "run", id, source}`;
- worker to page: `ready` once loaded (or `failed` if it can't load), then `output`, `done` and `crashed`, each carrying the `id` of the run it belongs to. The page ignores any that belong to a run it has already stopped.

A run still blocks its Worker, but never the page.

### 2. Stop and crash recovery both replace the Worker

Stop calls `worker.terminate()` and starts a new Worker. A trap inside the Worker is caught there and reported as `crashed`, and the page does the same as for Stop. This replaces the re-import, and nothing leaks, because a terminated Worker takes its whole wasm instance with it. The cost is that everything the Worker held goes: the environment, and any libraries it had loaded. For a playground that is the right trade, since Stop means "start over".

### 3. Output streams in batches, and the page keeps a bounded amount

`Interpreter::run_streaming(source, on_output)` hands each console write to a JS callback as it happens. Batching happens in `worker.js`, not in Rust. A wasm instance that has trapped can run nothing more, so text pending in Rust would be lost at exactly the moment it's most useful, just before a crash. Held in JS, the pending text is still sent before the `crashed` message. A call into JS per write is cheap; it's a `postMessage` per write that would flood the page.

While a program runs, its Worker is inside one synchronous call, so no timer can fire to flush a buffer. The batching decision therefore happens at each write: text is posted once 50ms have passed since the last post, or once 4,096 characters have built up.
- A write after a quiet spell goes out at once, so "Working…" before a long computation shows up.
- A loop that prints now and then sends roughly one message per 50ms. A tight printing loop is limited by size instead, to one message per 4,096 characters.
- The tail of a burst waits for the next write or for the end of the run. That's at most 50ms or 4,096 characters' worth, and it's never lost, even in a crash.

A runaway printing loop still produces megabytes a second. The page appends each batch as its own text node, because growing a single node copies all the text so far on every batch. It keeps at most 1,000,000 characters of a run's output, and below them a note counts the rest ("[… N more characters not shown]"). An earlier version kept everything. After 45 seconds of `(let loop () (display "x") (loop))` its renderer had grown to 5.4GB, and Stop no longer got through. With the cap, after a 10-second flood, the page holds just over 1,000,000 characters and Stop takes a millisecond or so.

`Interpreter::run` still collects all output into the result, and `web/smoke.mjs` uses it.

### 4. A browser test drives the real page

`web/browser-test.mjs` serves `web/www` and loads `index.html` in an iframe inside a harness page. The harness exists only in memory, so it is never deployed. The test then clicks through it in headless Chrome, read over the DevTools protocol. It runs a program, checks that a never-ending program streams output, and presses Stop. It floods the page for 10 seconds and checks that the output stays capped and Stop still works. Then it triggers a crash, checks that the output written just before it was shown, and runs once more. CI runs it before deploying.

Node can test the wasm module (`web/smoke.mjs`) but cannot run a browser Worker, so this is the only automatic check of the page itself. Each of these steps was checked by breaking what it guards:
- making Stop not restart the Worker;
- removing the output cap;
- dropping the flush in the crash path.

Each break fails its step.

The crash step overflows the stack with 100,000 levels of nesting, which works only while nscheme-m6i (Deeply nested data overflows the stack in the reader) is unfixed; its fix needs a new crash trigger here.

## Consequences

### Positive
- An endless loop is harmless, even one that prints as fast as it can. Stop works at any point, and output appears while the program runs.
- Crash recovery is a Worker restart: simpler than the re-import, and it doesn't leak.
- The page is tested automatically, in a real browser, locally and in CI.

### Negative / limitations
- Stopping is all-or-nothing: the Worker's environment and loaded libraries are lost. The REPL (nscheme-yd0.2.3) and the saved files (nscheme-yd0.2.4) are designed around that. Saved files will live on the page, not in the Worker.
- Startup pays for a Worker, and a Stop pays for a fresh one, which reloads the wasm module. The browser's cache makes the reload cheap.
- A run that prints more than 1,000,000 characters shows only the first million on the page.
- Console input is still end-of-file (nscheme-yd0.3).

[0006]: 0006-library-system.md
[0012]: 0012-browser-target.md
