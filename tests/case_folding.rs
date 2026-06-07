//! Unicode case folding (bead nscheme-vfp).
//!
//! `string-foldcase` does full folding (CaseFolding.txt C+F);
//! `char-foldcase` does simple folding (C+S, always one char). These
//! exercise cases the old ad-hoc to_lowercase+3-entry table missed.

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::value::{Value, equal};

fn run(src: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(&format!("(import (scheme base) (scheme char))\n{src}"), env)
}

fn folds(input: &str, expected: &str) {
    let v = run(&format!("(string-foldcase {input})")).unwrap();
    assert!(
        equal(&v, &Value::string(expected)),
        "string-foldcase {input} -> {v:?}, expected {expected:?}"
    );
}

#[test]
fn full_folding_one_to_many() {
    folds("\"ABC\"", "abc");
    folds("\"Fu\u{df}\"", "fuss"); // ß -> ss
    folds("\"\u{fb00}\u{fb01}\u{fb02}\"", "fffifl"); // ﬀﬁﬂ ligatures
    folds("\"\u{130}\"", "i\u{307}"); // İ -> i + combining dot above
    folds("\"\u{3a3}\"", "\u{3c3}"); // Σ -> σ
}

#[test]
fn simple_char_folding_stays_one_char() {
    // ß's simple fold is itself (its full fold "ss" is multi-char).
    assert!(equal(&run("(char-foldcase #\\x00df)").unwrap(), &Value::Char('\u{df}')));
    assert!(equal(&run("(char-foldcase #\\A)").unwrap(), &Value::Char('a')));
    // A Greek capital folds 1->1.
    assert!(equal(&run("(char-foldcase #\\x3a3)").unwrap(), &Value::Char('\u{3c3}')));
}

#[test]
fn ci_comparisons_use_folding() {
    // The canonical CaseFolding.txt example: "Fuß" matches "FUSS".
    assert!(equal(&run("(string-ci=? \"Stra\u{df}e\" \"STRASSE\")").unwrap(), &Value::Bool(true)));
    assert!(equal(&run("(char-ci=? #\\A #\\a)").unwrap(), &Value::Bool(true)));
}

#[test]
fn unlisted_code_points_fold_to_themselves() {
    folds("\"123 !@#\"", "123 !@#");
    folds("\"\u{4e2d}\u{6587}\"", "\u{4e2d}\u{6587}"); // CJK: no case
}
