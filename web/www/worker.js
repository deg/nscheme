// The interpreter's Web Worker (bead nscheme-yd0.2.2, ADR 0013).
//
// The page never runs Scheme itself: it posts programs here, so a long
// or endless run blocks only this worker, never the page, and the page
// can end it with worker.terminate(). Messages:
//
//   page -> worker   {type: "run", id, source}
//   worker -> page   {type: "ready"}                 once, after loading
//                    {type: "failed", message}       it could not load
//                    {type: "output", id, text}      while run `id` prints
//                    {type: "done", id, value, error, exitCode}
//                    {type: "crashed", id, message}  the instance trapped
//
// After "crashed" this worker's wasm instance is unusable (a Rust panic
// or a stack overflow leaves it mid-call), so the page terminates it and
// starts a new one.

import init, { Interpreter } from "./pkg/nscheme_web.js";

// Batching. The interpreter hands us every write (runStreaming); posting
// each one would send the page a message per character. A run is one
// synchronous call, so no timer can fire to flush a quiet buffer: the
// decision is made at each write. Text is posted once FLUSH_MS have
// passed since the last post, so a write after a quiet spell goes out at
// once; and also once FLUSH_CHARS have built up, so a tight printing loop
// sends one message per FLUSH_CHARS. The buffer lives here, not in wasm,
// so if the run traps, what it printed last still reaches the page.
const FLUSH_MS = 50;
const FLUSH_CHARS = 4096;
let pending = "";
let lastPost = -Infinity;

function post(id) {
  if (pending) {
    self.postMessage({ type: "output", id, text: pending });
    pending = "";
  }
  lastPost = performance.now();
}

self.onmessage = (event) => {
  const { type, id, source } = event.data;
  if (type !== "run") return;
  pending = "";
  lastPost = -Infinity;
  let interp;
  try {
    // A fresh environment per run.
    interp = new Interpreter();
    const r = interp.runStreaming(source, (text) => {
      pending += text;
      if (performance.now() - lastPost >= FLUSH_MS || pending.length >= FLUSH_CHARS) post(id);
    });
    post(id);
    self.postMessage({ type: "done", id, value: r.value, error: r.error, exitCode: r.exitCode });
    r.free();
  } catch (e) {
    post(id);
    self.postMessage({ type: "crashed", id, message: String(e && e.message || e) });
  } finally {
    try { interp?.free(); } catch { /* a trapped instance may refuse */ }
  }
};

try {
  await init();
  self.postMessage({ type: "ready" });
} catch (e) {
  self.postMessage({ type: "failed", message: String(e && e.message || e) });
}
