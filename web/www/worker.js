// The interpreter's Web Worker (beads nscheme-yd0.2.2, nscheme-yd0.2.3;
// ADR 0013).
//
// The page never runs Scheme itself: it posts entries here, so a long or
// endless one blocks only this worker, never the page, and the page can
// end it with worker.terminate(). The page is a REPL: every entry is
// evaluated in the same Scheme environment, so definitions accumulate,
// until Reset starts a new one. Messages:
//
//   page -> worker   {type: "eval", id, source}   evaluate one entry
//                    {type: "reset"}              fresh environment, and
//                                                 forget the user's libraries
//   worker -> page   {type: "ready"}                 once, after loading
//                    {type: "failed", message}       it could not load
//                    {type: "output", id, text}      while entry `id` prints
//                    {type: "done", id, value, error, exitCode}
//                    {type: "crashed", id, message}  the instance trapped
//                    {type: "broken", message}       it trapped during a reset
//
// After "crashed" this worker's wasm instance is unusable (a Rust panic
// or a stack overflow leaves it mid-call), so the page terminates it and
// starts a new one, which also loses the environment.

import init, { Interpreter, forgetUserLibraries } from "./pkg/nscheme_web.js";

// Batching. The interpreter hands us every write (runStreaming); posting
// each one would send the page a message per character. An entry runs as
// one synchronous call, so no timer can fire to flush a quiet buffer:
// the decision is made at each write. Text is posted once FLUSH_MS have
// passed since the last post, so a write after a quiet spell goes out at
// once; and also once FLUSH_CHARS have built up, so a tight printing loop
// sends one message per FLUSH_CHARS. The buffer lives here, not in wasm,
// so if the entry traps, what it printed last still reaches the page.
const FLUSH_MS = 50;
const FLUSH_CHARS = 4096;
let pending = "";
let lastPost = -Infinity;

// The one Scheme environment, made on first use and replaced by reset.
let interp = null;

function post(id) {
  if (pending) {
    self.postMessage({ type: "output", id, text: pending });
    pending = "";
  }
  lastPost = performance.now();
}

function evaluate(id, source) {
  pending = "";
  lastPost = -Infinity;
  try {
    interp ??= new Interpreter();
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
  }
}

function reset() {
  try {
    interp?.free();
    interp = null;
    forgetUserLibraries();
  } catch (e) {
    // Freeing an environment can trap (very deep data); the instance is
    // then unusable, so ask the page for a new worker.
    self.postMessage({ type: "broken", message: String(e && e.message || e) });
  }
}

self.onmessage = (event) => {
  const msg = event.data;
  if (msg.type === "eval") evaluate(msg.id, msg.source);
  else if (msg.type === "reset") reset();
};

try {
  await init();
  self.postMessage({ type: "ready" });
} catch (e) {
  self.postMessage({ type: "failed", message: String(e && e.message || e) });
}
