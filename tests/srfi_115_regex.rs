//! (scheme regex) / (srfi 115) tests — bead nscheme-lul.13.
//!
//! Cases are mined and translated from the SRFI 115 reference test
//! suite (contrib/duy-nguyen/srfi/115/test.sld, Alex Shinn, BSD-3-Clause).
//! Each `(test EXPECTED EXPR)` is checked as an equality on `run("EXPR")`;
//! each `(test-assert EXPR)`-style case is asserted via `assert_true`.
//!
//! IMPORTANT — these tests are currently EXPECTED TO FAIL TO LOAD.
//! (srfi 115) is BLOCKED in nscheme: its reference implementation is
//! built on SRFI 14 (char-sets, ~141 uses) and a bitwise SRFI
//! (60/33/151), neither of which nscheme provides. The cases below are
//! the integrator's target once those dependencies are vendored. Until
//! then `run(...)` returns an import/eval error and these tests fail.
//! They are retained (not #[ignore]d) so the failure is loud and the
//! intended behaviour is documented in-tree.

use std::path::PathBuf;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::library::set_search_path;
use nscheme::value::{Value, equal};

/// The library lives in the repo's real `lib/` tree.
fn lib_dir() -> PathBuf {
    PathBuf::from(format!("{}/lib", env!("CARGO_MANIFEST_DIR")))
}

const PRELUDE: &str = r"
(import (scheme base) (scheme regex))

;; Helpers mirroring the SRFI 115 test suite's test-re / test-re-search.
(define (maybe-match->list rx str . o)
  (let ((res (apply regexp-matches rx str o)))
    (and res (regexp-match->list res))))
(define (maybe-search->list rx str . o)
  (let ((res (apply regexp-search rx str o)))
    (and res (regexp-match->list res))))
";

fn run(expr: &str) -> Result<Value, EvalError> {
    set_search_path(vec![lib_dir()]);
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&format!("{PRELUDE}\n{expr}"), env)
}

/// Assert that a Scheme expression evaluates to #t.
fn assert_true(expr: &str) {
    match run(expr) {
        Ok(Value::Bool(true)) => {}
        Ok(other) => panic!("expected #t from `{expr}`, got {other}"),
        Err(e) => panic!("error evaluating `{expr}`: {e:?}"),
    }
}

/// Assert that a Scheme expression evaluates to a value `equal?` to the
/// value of `expected`.
fn assert_equal(expected: &str, expr: &str) {
    let want = run(expected).expect("evaluating expected");
    match run(expr) {
        Ok(got) if equal(&got, &want) => {}
        Ok(other) => panic!("expected `{expected}` from `{expr}`, got {other}"),
        Err(e) => panic!("error evaluating `{expr}`: {e:?}"),
    }
}

#[test]
fn match_capture_group() {
    // (test-re '("ababc" "abab") '(: ($ (* "ab")) "c") "ababc")
    assert_equal(
        "'(\"ababc\" \"abab\")",
        "(maybe-match->list '(: ($ (* \"ab\")) \"c\") \"ababc\")",
    );
}

#[test]
fn match_with_start_offset() {
    // (test-re '("ababc" "abab") '(: ($ (* "ab")) "c") "xababc" 1)
    assert_equal(
        "'(\"ababc\" \"abab\")",
        "(maybe-match->list '(: ($ (* \"ab\")) \"c\") \"xababc\" 1)",
    );
}

#[test]
fn simple_search() {
    // (test-re-search '("y") '(: "y") "xy")
    assert_equal("'(\"y\")", "(maybe-search->list '(: \"y\") \"xy\")");
}

#[test]
fn search_capture() {
    // (test-re-search '("ababc" "abab") '(: ($ (* "ab")) "c") "xababc")
    assert_equal(
        "'(\"ababc\" \"abab\")",
        "(maybe-search->list '(: ($ (* \"ab\")) \"c\") \"xababc\")",
    );
}

#[test]
fn no_match_returns_false() {
    // (test-re #f '(: (* any) ($ "foo" (* any)) ($ "bar" (* any))) "fooxbafba")
    assert!(matches!(
        run(
            "(maybe-match->list '(: (* any) ($ \"foo\" (* any)) ($ \"bar\" (* any))) \"fooxbafba\")"
        ),
        Ok(Value::Bool(false))
    ));
}

#[test]
fn greedy_alternation() {
    // (test-re '("abcd" "abcd") '($ (* (or "ab" "cd"))) "abcd")
    assert_equal(
        "'(\"abcd\" \"abcd\")",
        "(maybe-match->list '($ (* (or \"ab\" \"cd\"))) \"abcd\")",
    );
}

#[test]
fn named_submatch() {
    // (test "ab" (regexp-match-submatch (regexp-matches '(or (-> foo "ab") (-> foo "cd")) "ab") 'foo))
    assert_equal(
        "\"ab\"",
        "(regexp-match-submatch (regexp-matches '(or (-> foo \"ab\") (-> foo \"cd\")) \"ab\") 'foo)",
    );
}

#[test]
fn anchors_bos_eos() {
    // (test-re '("ababc" "abab") '(: bos ($ (* "ab")) "c" eos) "ababc")
    assert_equal(
        "'(\"ababc\" \"abab\")",
        "(maybe-match->list '(: bos ($ (* \"ab\")) \"c\" eos) \"ababc\")",
    );
}

#[test]
fn non_greedy_quantifier() {
    // (test-re '("<em>Hello World</em>" "em") '(: "<" ($ (*? any)) ">" (* any)) "<em>Hello World</em>")
    assert_equal(
        "'(\"<em>Hello World</em>\" \"em\")",
        "(maybe-match->list '(: \"<\" ($ (*? any)) \">\" (* any)) \"<em>Hello World</em>\")",
    );
}

#[test]
fn char_range_class() {
    // (test-re '("beef") '(* (/"af")) "beef")
    assert_equal(
        "'(\"beef\")",
        "(maybe-match->list '(* (/ \"af\")) \"beef\")",
    );
}

#[test]
fn nocase_modifier() {
    // (test-re '("abcD") '(w/nocase (* lower)) "abcD")
    assert_equal(
        "'(\"abcD\")",
        "(maybe-match->list '(w/nocase (* lower)) \"abcD\")",
    );
}

#[test]
fn regexp_extract_numeric() {
    // (test '("123" "456" "789") (regexp-extract '(+ numeric) "abc123def456ghi789"))
    assert_equal(
        "'(\"123\" \"456\" \"789\")",
        "(regexp-extract '(+ numeric) \"abc123def456ghi789\")",
    );
}

#[test]
fn regexp_split_numeric() {
    // (test '("abc" "def" "ghi" "") (regexp-split '(+ numeric) "abc123def456ghi789"))
    assert_equal(
        "'(\"abc\" \"def\" \"ghi\" \"\")",
        "(regexp-split '(+ numeric) \"abc123def456ghi789\")",
    );
}

#[test]
fn regexp_replace_single() {
    // (test "abc def" (regexp-replace '(+ space) "abc \t\n def" " "))
    assert_equal(
        "\"abc def\"",
        "(regexp-replace '(+ space) \"abc \\t\\n def\" \" \")",
    );
}

#[test]
fn regexp_replace_all() {
    // (test " abc d ef " (regexp-replace-all '(+ space) "  abc \t\n d ef  " " "))
    assert_equal(
        "\" abc d ef \"",
        "(regexp-replace-all '(+ space) \"  abc \\t\\n d ef  \" \" \")",
    );
}

#[test]
fn regexp_partition() {
    // (test '("abc" "123" "def" "456" "ghi") (regexp-partition '(* numeric) "abc123def456ghi"))
    assert_equal(
        "'(\"abc\" \"123\" \"def\" \"456\" \"ghi\")",
        "(regexp-partition '(* numeric) \"abc123def456ghi\")",
    );
}

#[test]
fn predicate_regexp() {
    // (regexp? (regexp '(: "a"))) should be #t
    assert_true("(regexp? (regexp '(: \"a\")))");
}
