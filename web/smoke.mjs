// Smoke test for the browser build (bead nscheme-yd0.1): load the built
// wasm in Node, which runs the same module a browser does, and check what
// only the wasm build can get wrong. Native `cargo test` covers the logic.
//
//   web/build.sh && node web/smoke.mjs
//
// Exits non-zero on any failure.

import { readFileSync } from "node:fs";
import init, { Interpreter } from "./www/pkg/nscheme_web.js";

const wasm = readFileSync(new URL("./www/pkg/nscheme_web_bg.wasm", import.meta.url));
await init({ module_or_path: wasm });

let failures = 0;
function check(name, source, expect) {
  let r;
  try {
    r = new Interpreter().run(source);
  } catch (e) {
    // A Rust panic traps and leaves this instance unusable.
    console.log(`FAIL ${name}: trapped: ${e}`);
    failures++;
    process.exit(1);
  }
  const got = { output: r.output, value: r.value, error: r.error, exitCode: r.exitCode };
  const problem = expect(got);
  if (problem) {
    console.log(`FAIL ${name}: ${problem}\n     got ${JSON.stringify(got)}`);
    failures++;
  } else {
    console.log(`ok   ${name}`);
  }
}

check("display output", '(display "hello, browser") (newline)', (r) =>
  r.output === "hello, browser\n" ? null : "wrong output");

check("value of the last expression", "(define (f n) (if (= n 0) 1 (* n (f (- n 1))))) (f 25)", (r) =>
  r.value === "15511210043330985984000000" ? null : "wrong bignum");

check("import from the compiled-in lib/", "(import (scheme list)) (fold + 0 (iota 10))", (r) =>
  r.value === "45" ? null : "import failed");

check("a library that imports others", "(import (scheme mapping) (scheme comparator)) " +
  "(mapping-ref (mapping (make-default-comparator) 'a 1 'b 2) 'b)", (r) =>
  r.value === "2" ? null : "nested import failed");

check("cond-expand sees compiled-in libraries",
  "(cond-expand ((library (scheme list)) 'found) (else 'missing))", (r) =>
  r.value === "found" ? null : "library test failed");

check("error with line:col", "(define x 1)\n(vector-ref x 0)", (r) =>
  r.error && r.error.includes("line 2:1") ? null : "no location");

check("exit status", '(display "a") (exit 3) (display "b")', (r) =>
  r.output === "a" && r.exitCode === 3 ? null : "exit not reported");

check("guard cannot catch exit", "(guard (e (#t 'caught)) (exit 5))", (r) =>
  r.exitCode === 5 ? null : "exit was caught");

check("clock is wall time", "(current-second)", (r) => {
  const t = Number(r.value);
  return Math.abs(t - Date.now() / 1000) < 60 ? null : "clock is wrong";
});

check("current-jiffy advances", "(let ((a (current-jiffy))) (let loop ((i 0)) (if (< i 20000) (loop (+ i 1)))) (> (current-jiffy) a))", (r) =>
  r.value === "#t" ? null : "jiffies did not advance");

check("file ports fail as file errors", '(guard (e ((file-error? e) \'file-error)) (open-input-file "x.txt"))', (r) =>
  r.value === "file-error" ? null : "not a file error");

check("stdin reads end-of-file", "(eof-object? (read-line))", (r) =>
  r.value === "#t" ? null : "stdin did not report eof");

check("deep tail recursion", "(let loop ((i 0)) (if (< i 1000000) (loop (+ i 1)) i))", (r) =>
  r.value === "1000000" ? null : "tail loop failed");

check("deep non-tail recursion", "(define (count n) (if (= n 0) 0 (+ 1 (count (- n 1))))) (count 100000)", (r) =>
  r.value === "100000" ? null : "deep recursion failed");

// Streaming (nscheme-yd0.2.2): runStreaming hands each console write to a
// callback as it happens, instead of returning the output at the end.
// (worker.js batches them; it needs every write, in order, to do so.)
{
  const chunks = [];
  const interp = new Interpreter();
  const r = interp.runStreaming(
    '(display "a") (newline) (let loop ((i 0)) (when (< i 20000) (display "x") (loop (+ i 1)))) 42',
    (text) => chunks.push(text));
  const all = chunks.join("");
  const problem =
    all !== "a\n" + "x".repeat(20000) ? "streamed text differs" :
    r.output !== "" ? "runStreaming also returned the output" :
    r.value !== "42" ? "value lost" :
    // display "a", newline, then 20,000 one-character writes
    chunks.length !== 20002 ? `${chunks.length} callbacks for 20,002 writes` : null;
  if (problem) { console.log(`FAIL streaming: ${problem}`); failures++; }
  else console.log("ok   streaming");
}

// Freeing a list used to recurse once per element, trapping at about
// 10,000 elements (nscheme-9x7).
check("long flat list is freed", "(length (make-list 1000000 0))", (r) =>
  r.value === "1000000" ? null : "long list failed");

// Nesting depth exercises the recursive reader, which uses the wasm stack
// rather than the heap. 3000 levels traps with wasm's default 1MB stack
// (limit ~2,200) and passes with the 8MB that .cargo/config.toml sets
// (~4,800 in V8), so this fails if that setting is lost, e.g. to a
// RUSTFLAGS variable, which replaces it.
const depth = Number(process.env.NSCHEME_NEST_DEPTH ?? 3000);
check(`nested list ${depth} deep`, `(length '${"(".repeat(depth)}${")".repeat(depth)})`, (r) =>
  r.value === "1" ? null : "nested read failed");

if (failures) {
  console.log(`\n${failures} failed`);
  process.exit(1);
}
console.log("\nall passed");
