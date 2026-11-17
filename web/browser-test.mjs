// Browser test for the demo page (beads nscheme-yd0.2.2, nscheme-yd0.2.3): drive the real
// web/www/index.html in headless Chrome, the way a person would.
//
//   web/build.sh && node web/browser-test.mjs
//
// web/smoke.mjs checks the wasm module in Node; this checks what only a
// browser can run: the page, its Web Worker, the session (entries that
// build on each other, Enter, Reset), streaming output, Stop and crash
// recovery. It serves web/www itself, adds a harness page (only in
// memory, so nothing extra is deployed) that loads index.html in an
// iframe and clicks through it, and reads the harness's verdict over the
// Chrome DevTools protocol. Set CHROME to the browser binary if it isn't
// found. Exits non-zero on any failure.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, normalize } from "node:path";

const www = new URL("./www/", import.meta.url).pathname;

// The steps, run inside the page. Each records a value; the Node side
// prints them and checks the expectations below.
const HARNESS = `<!doctype html><meta charset="utf-8"><title>harness</title>
<iframe id="f" src="index.html" style="width:900px;height:900px"></iframe>
<pre id="result">pending</pre>
<script>
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(pred, ms = 20000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (pred()) return true; await sleep(50); }
  return false;
}
async function main() {
  // Each step's result, also mirrored onto #result so a run that never
  // finishes still shows how far it got.
  const out = new Proxy({}, { set(target, key, value) {
    target[key] = value;
    document.getElementById("result").dataset.partial = JSON.stringify(target);
    return true;
  } });
  const f = document.getElementById("f");
  await until(() => f.contentDocument && f.contentDocument.getElementById("evaluate"));
  const d = f.contentDocument, w = f.contentWindow;
  const evalButton = d.getElementById("evaluate"), stop = d.getElementById("stop");
  const reset = d.getElementById("reset");
  const src = d.getElementById("source"), output = d.getElementById("output");
  const has = (re) => re.test(output.textContent);
  const idle = () => until(() => !evalButton.disabled);
  // Type an entry and press Evaluate.
  const go = async (entry) => { await idle(); src.value = entry; evalButton.click(); };
  // Type an entry and press Enter, as a person would.
  const enter = async (entry) => {
    await idle();
    src.value = entry;
    src.dispatchEvent(new w.KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  };

  out.ready = await until(() => !evalButton.disabled && evalButton.textContent === "Evaluate");
  await go('(display "hello") (newline) (+ 1 2)');
  out.hello = await until(() => has(/hello\\n⇒ 3\\n/));
  out.entryEchoedAndCleared = has(/› \\(display "hello"\\)/) && src.value === "";

  // Definitions build up across entries.
  await go("(define (sq x) (* x x))");
  await go("(sq 12)");
  out.definitionsAccumulate = await until(() => has(/⇒ 144/));

  // Enter submits only a complete entry.
  await enter("(+ 1");
  await sleep(300);
  out.enterWaitsForBalance = src.value === "(+ 1" && !has(/› \\(\\+ 1\\n/);
  // In the middle of a balanced entry (an example, a paste), Enter must
  // add a line rather than evaluate it.
  await idle();
  src.value = "(+ 100 200)";
  src.setSelectionRange(3, 3);
  src.dispatchEvent(new w.KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  await sleep(300);
  out.enterMidEntryAddsALine = src.value === "(+ 100 200)" && !has(/⇒ 300/);
  await enter("(+ 1 2 3 4)");
  out.enterSubmitsBalanced = await until(() => has(/⇒ 10\\n/));

  // History: Mod+Up/Down steps through earlier entries, keeps edits made
  // to a recalled one, and comes back to the draft.
  await go("(define h1 1)");
  await go("(define h2 2)");
  await idle();
  const key = (k) => src.dispatchEvent(new w.KeyboardEvent("keydown",
    { key: k, ctrlKey: true, bubbles: true, cancelable: true }));
  src.value = "my draft";
  key("ArrowUp"); key("ArrowUp");
  const recalled = src.value;
  src.value = "(define h1 100)";
  key("ArrowUp"); key("ArrowDown");
  const keptEdit = src.value;
  key("ArrowDown"); key("ArrowDown");
  out.historySteps = recalled === "(define h1 1)" && keptEdit === "(define h1 100)" && src.value === "my draft";
  src.value = "";

  // Streaming and Stop; Stop starts a new session.
  await go("(let loop ((i 0)) (when (= 0 (remainder i 200000)) (display i) (newline)) (loop (+ i 1)))");
  out.streamsWhileRunning = await until(() => has(/\\n200000\\n/));
  out.stopEnabledWhileRunning = !stop.disabled && reset.disabled;
  stop.click();
  out.stopped = await until(() => has(/\\[stopped; the session's definitions are gone\\]/));
  await go("(sq 3)");
  out.stopForgetsDefinitions = await until(() => has(/undefined variable: sq/));

  // A tight printing loop sends a full batch every few milliseconds. The
  // page must keep only a bounded amount of it, and Stop must still get
  // through after a long flood. (A 2-second flood hid an unbounded page
  // that, after 45 seconds, had grown to gigabytes and ignored Stop.)
  await go('(let loop () (display "x") (loop))');
  await sleep(10000);
  const clicked = Date.now();
  stop.click();
  out.stopsAFlood = await until(() => /\\[stopped/.test(output.textContent.slice(-200)), 5000);
  out.floodStopMs = Date.now() - clicked;
  out.floodChars = output.textContent.length;
  out.floodIsCapped = out.floodChars < 1_003_000 && has(/more characters not shown/);
  // A second flood: each entry is capped, and the transcript as a whole
  // drops its oldest output rather than growing past its own limit.
  await go('(let loop () (display "y") (loop))');
  await sleep(6000);
  stop.click();
  out.secondFloodStopped = await until(() => /\\[stopped/.test(output.textContent.slice(-200)), 5000);
  out.transcriptBounded = output.textContent.length < 2_100_000 && has(/\\[earlier output removed\\]/);

  // Reset: a fresh environment, an empty transcript, and the user's
  // libraries forgotten (an inline one can no longer be imported).
  await idle();
  await go("(define-library (scratch v) (export v) (import (scheme base)) (begin (define v 1))) (define kept 'yes)");
  await go("(import (scratch v)) v");
  out.inlineLibraryWorks = await until(() => has(/⇒ 1\\n/));
  await idle();
  reset.click();
  out.resetClearsTranscript = await until(() => output.textContent.length < 200 && has(/\\[reset/));
  await go("kept");
  out.resetForgetsDefinitions = await until(() => has(/undefined variable: kept/));
  await go("(import (scratch v))");
  out.resetForgetsUserLibraries = await until(() => has(/unknown library: \\(scratch v\\)/));

  // exit ends the session, as in the CLI REPL.
  await go("(define before-exit 1)");
  await go("(exit 2)");
  out.exitNoted = await until(() => has(/\\[exited with status 2; the session was reset\\]/));
  await go("before-exit");
  out.exitResetsSession = await until(() => has(/undefined variable: before-exit/));

  // Crash trigger: 100,000 levels of nesting overflow the stack while the
  // result is printed and freed. It relies on nscheme-m6i (Deeply nested
  // data overflows the stack in the reader) being unfixed; whoever fixes
  // that needs a new way to trap the instance here.
  // The second write lands within the batching window, so it is still
  // pending when the instance traps; it must reach the page anyway.
  await go('(display "first ") (display "last words") ' +
    "(let loop ((i 0) (x '())) (if (= i 100000) x (loop (+ i 1) (list x))))");
  out.crashReported = await until(() => has(/crashed/), 40000);
  out.outputBeforeCrashShown = has(/first last words/);
  await go("(* 7 8)");
  out.runsAfterCrash = await until(() => has(/⇒ 56/), 40000);
  document.getElementById("result").textContent = "RESULT " + JSON.stringify(out);
}
main().catch(e => { document.getElementById("result").textContent = "RESULT " + JSON.stringify({ error: String(e) }); });
</script>`;

const EXPECT = {
  ready: true,
  hello: true,
  entryEchoedAndCleared: true,
  definitionsAccumulate: true,
  enterWaitsForBalance: true,
  enterMidEntryAddsALine: true,
  enterSubmitsBalanced: true,
  historySteps: true,
  streamsWhileRunning: true,
  stopEnabledWhileRunning: true,
  stopped: true,
  stopForgetsDefinitions: true,
  stopsAFlood: true,
  floodIsCapped: true,
  secondFloodStopped: true,
  transcriptBounded: true,
  inlineLibraryWorks: true,
  resetClearsTranscript: true,
  resetForgetsDefinitions: true,
  resetForgetsUserLibraries: true,
  exitNoted: true,
  exitResetsSession: true,
  crashReported: true,
  outputBeforeCrashShown: true,
  runsAfterCrash: true,
};

// The harness is a string, so a typo in it would only show as a run
// that never finishes. Parse its script up front instead.
try {
  new Function(HARNESS.slice(HARNESS.indexOf("<script>") + 8, HARNESS.lastIndexOf("</script>")));
} catch (e) {
  console.log(`FAIL: the harness script does not parse: ${e.message}`);
  process.exit(1);
}

const TYPES = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm" };

function findChrome() {
  const candidates = [
    process.env.CHROME,
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/usr/bin/google-chrome", "/usr/bin/google-chrome-stable", "/usr/bin/chromium",
  ];
  return candidates.find((p) => p && existsSync(p));
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const chrome = findChrome();
if (!chrome) {
  console.log("FAIL: no Chrome found; set CHROME to its binary");
  process.exit(1);
}
if (!existsSync(join(www, "pkg", "nscheme_web_bg.wasm"))) {
  console.log("FAIL: web/www/pkg is missing; run web/build.sh first");
  process.exit(1);
}

const server = createServer((req, res) => {
  const path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (path === "/harness.html") {
    res.writeHead(200, { "content-type": "text/html" });
    return res.end(HARNESS);
  }
  const file = normalize(join(www, path === "/" ? "index.html" : path));
  if (!file.startsWith(www) || !existsSync(file)) { res.writeHead(404); return res.end(); }
  res.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" });
  res.end(readFileSync(file));
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const { port } = server.address();

const profile = mkdtempSync(join(tmpdir(), "nscheme-chrome-"));
const debugPort = 9000 + Math.floor(Math.random() * 900);
const browser = spawn(chrome, ["--headless=new", "--disable-gpu", "--no-first-run",
  `--user-data-dir=${profile}`, `--remote-debugging-port=${debugPort}`, "about:blank"],
  { stdio: ["ignore", "ignore", "pipe"] });
// Kept so a failed launch (say, in CI) can say why.
let chromeStderr = "";
browser.stderr.on("data", (d) => { chromeStderr = (chromeStderr + d).slice(-4000); });

let result;
let partial;
let setupError;
try {
  let targets;
  for (let i = 0; i < 100 && !targets; i++) {
    try { targets = await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json(); }
    catch { await sleep(200); }
  }
  if (!targets) throw new Error(`Chrome did not start (${chrome}). Its stderr:\n${chromeStderr}`);
  const page = targets.find((t) => t.type === "page");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener("open", r));
  let id = 0;
  const pending = new Map();
  ws.addEventListener("message", (m) => {
    const d = JSON.parse(m.data);
    pending.get(d.id)?.(d);
    pending.delete(d.id);
  });
  const send = (method, params = {}) => new Promise((r) => {
    pending.set(++id, r);
    ws.send(JSON.stringify({ id, method, params }));
  });
  await send("Page.navigate", { url: `http://127.0.0.1:${port}/harness.html` });
  for (let i = 0; i < 360 && !result; i++) {
    await sleep(500);
    const r = await send("Runtime.evaluate", {
      expression: "JSON.stringify([document.getElementById('result')?.textContent, document.getElementById('result')?.dataset.partial])",
    });
    const [text, soFar] = JSON.parse(r.result?.result?.value ?? "[]");
    partial = soFar;
    if (text?.startsWith("RESULT ")) result = JSON.parse(text.slice(7));
  }
  ws.close();
} catch (e) {
  setupError = e;
} finally {
  browser.kill();
  server.close();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}

if (setupError) {
  console.log(`FAIL: ${setupError.message}`);
  process.exit(1);
}
if (!result) {
  console.log(`FAIL: the harness never finished. Steps done: ${partial || "none"}`);
  process.exit(1);
}
let failures = 0;
for (const [step, want] of Object.entries(EXPECT)) {
  const got = result[step];
  const ok = got === want;
  console.log(`${ok ? "ok  " : "FAIL"} ${step}${ok ? "" : `: got ${JSON.stringify(got)}`}`);
  if (!ok) failures++;
}
if (result.error) { console.log(`FAIL harness error: ${result.error}`); failures++; }
if (result.floodStopMs !== undefined) {
  console.log(`     (flood: Stop took ${result.floodStopMs} ms with ${result.floodChars} characters on the page)`);
}
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
