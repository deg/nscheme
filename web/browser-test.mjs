// Browser test for the demo page (bead nscheme-yd0.2.2): drive the real
// web/www/index.html in headless Chrome, the way a person would.
//
//   web/build.sh && node web/browser-test.mjs
//
// web/smoke.mjs checks the wasm module in Node; this checks what only a
// browser can run: the page, its Web Worker, streaming output, Stop and
// crash recovery. It serves web/www itself, adds a harness page (only in
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
  const out = {};
  const f = document.getElementById("f");
  await until(() => f.contentDocument && f.contentDocument.getElementById("run"));
  const d = f.contentDocument;
  const run = d.getElementById("run"), stop = d.getElementById("stop");
  const src = d.getElementById("source"), output = d.getElementById("output");
  const go = async (program) => { await until(() => !run.disabled); src.value = program; run.click(); };
  out.ready = await until(() => !run.disabled && run.textContent === "Run");
  await go('(display "hello") (newline) (+ 1 2)');
  out.hello = (await until(() => /ms\\]/.test(output.textContent))) && output.textContent;
  await go("(let loop ((i 0)) (when (= 0 (remainder i 200000)) (display i) (newline)) (loop (+ i 1)))");
  out.streamsWhileRunning = await until(() => /200000/.test(output.textContent));
  out.stopEnabledWhileRunning = !stop.disabled;
  stop.click();
  out.stopped = await until(() => /\\[stopped\\]/.test(output.textContent));
  await go("(* 6 7)");
  out.runsAfterStop = await until(() => /⇒ 42/.test(output.textContent));
  // A tight printing loop sends a full batch every few milliseconds. The
  // page must keep only a bounded amount of it, and Stop must still get
  // through after a long flood. (A 2-second flood hid an unbounded page
  // that, after 45 seconds, had grown to gigabytes and ignored Stop.)
  await go('(let loop () (display "x") (loop))');
  await sleep(10000);
  const clicked = Date.now();
  stop.click();
  out.stopsAFlood = await until(() => /\\[stopped\\]/.test(output.textContent), 5000);
  out.floodStopMs = Date.now() - clicked;
  out.floodChars = output.textContent.length;
  out.floodIsCapped = out.floodChars < 1_001_000 && /more characters not shown/.test(output.textContent);
  // Crash trigger: 100,000 levels of nesting overflow the stack while the
  // result is printed and freed. It relies on nscheme-m6i (Deeply nested
  // data overflows the stack in the reader) being unfixed; whoever fixes
  // that needs a new way to trap the instance here.
  // The second write lands within the batching window, so it is still
  // pending when the instance traps; it must reach the page anyway.
  await go('(display "first ") (display "last words") ' +
    "(let loop ((i 0) (x '())) (if (= i 100000) x (loop (+ i 1) (list x))))");
  out.crashReported = await until(() => /crashed/.test(output.textContent), 40000);
  out.outputBeforeCrashShown = /first last words/.test(output.textContent);
  await go("(* 7 8)");
  out.runsAfterCrash = await until(() => /⇒ 56/.test(output.textContent), 40000);
  document.getElementById("result").textContent = "RESULT " + JSON.stringify(out);
}
main().catch(e => { document.getElementById("result").textContent = "RESULT " + JSON.stringify({ error: String(e) }); });
</script>`;

const EXPECT = {
  ready: true,
  hello: "hello\n⇒ 3\n",            // prefix; the timing line follows
  streamsWhileRunning: true,
  stopEnabledWhileRunning: true,
  stopped: true,
  runsAfterStop: true,
  stopsAFlood: true,
  floodIsCapped: true,
  crashReported: true,
  outputBeforeCrashShown: true,
  runsAfterCrash: true,
};

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
    const r = await send("Runtime.evaluate", { expression: "document.getElementById('result')?.textContent" });
    const text = r.result?.result?.value ?? "";
    if (text.startsWith("RESULT ")) result = JSON.parse(text.slice(7));
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
  console.log("FAIL: the harness never finished");
  process.exit(1);
}
let failures = 0;
for (const [step, want] of Object.entries(EXPECT)) {
  const got = result[step];
  const ok = typeof want === "string" ? typeof got === "string" && got.startsWith(want) : got === want;
  console.log(`${ok ? "ok  " : "FAIL"} ${step}${ok ? "" : `: got ${JSON.stringify(got)}`}`);
  if (!ok) failures++;
}
if (result.error) { console.log(`FAIL harness error: ${result.error}`); failures++; }
if (result.floodStopMs !== undefined) {
  console.log(`     (flood: Stop took ${result.floodStopMs} ms with ${result.floodChars} characters on the page)`);
}
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
