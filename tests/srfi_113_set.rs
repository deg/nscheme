//! (scheme set) / (srfi 113) tests — bead nscheme-lul.6.
//!
//! Cases are mined and translated from the SRFI 113 reference test
//! suite (sets-test.scm, John Cowan, MIT). Rather than port the
//! SRFI-64 / Chicken `test` harness (not an R7RS-large deliverable),
//! each group is run through `eval_source` and its combined `(and …)`
//! asserted, matching this repo's test convention.

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

/// Element comparators used throughout the SRFI 113 test suite, rebuilt
/// here from (scheme comparator) primitives (the upstream comparators-shim
/// supplies the same set under SRFI 114; we use the SRFI 128 equivalents).
const PRELUDE: &str = r"
(import (scheme base) (scheme comparator) (scheme set))
(define number-comparator
  (make-comparator number? = < number-hash))
(define char-comparator
  (make-comparator char? char=? char<? char-hash))
(define string-ci-comparator
  (make-comparator string? string-ci=? string-ci<? string-ci-hash))
(define eq-comparator (make-eq-comparator))
(define eqv-comparator (make-eqv-comparator))
(define equal-comparator (make-equal-comparator))
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

/// Assert that a Scheme expression evaluates to a given integer.
fn assert_int(expr: &str, expected: i64) {
    match run(expr) {
        Ok(v) if equal(&v, &Value::Int(expected)) => {}
        Ok(other) => panic!("expected {expected} from `{expr}`, got {other}"),
        Err(e) => panic!("error evaluating `{expr}`: {e:?}"),
    }
}

#[test]
fn set_predicates_and_membership() {
    assert_true(
        "(let ((syms (set eq-comparator 'a 'b 'c 'd))
               (esyms (set eq-comparator)))
           (and (set? syms)
                (not (set? 'a))
                (set-empty? esyms)
                (set-contains? syms 'a)
                (not (set-contains? syms 'z))))",
    );
}

#[test]
fn set_size_adjoin_delete() {
    // From sets/simple: adjoining a duplicate does not grow the set.
    assert_int(
        "(let ((nums (set eqv-comparator)))
           (set-adjoin! nums 2)
           (set-adjoin! nums 3)
           (set-adjoin! nums 4)
           (set-adjoin! nums 4)
           (set-size nums))",
        3,
    );
    assert_int("(set-size (set-adjoin (set eqv-comparator 2 3 4) 5))", 4);
    assert_int(
        "(set-size (set-delete (set eq-comparator 'a 'b 'c 'd) 'd))",
        3,
    );
    assert_int(
        "(set-size (set-delete-all (set eq-comparator 'a 'b 'c 'd) '(c d)))",
        2,
    );
}

#[test]
fn set_map_for_each_fold() {
    assert_true(
        "(let ((nums2 (set-map number-comparator
                               (lambda (x) (* 10 x))
                               (set eqv-comparator 3 4))))
           (and (set-contains? nums2 30)
                (not (set-contains? nums2 3))))",
    );
    assert_int(
        "(let ((total 0))
           (set-for-each (lambda (x) (set! total (+ total x)))
                         (set eqv-comparator 30 40))
           total)",
        70,
    );
    assert_int("(set-fold + 3 (set eqv-comparator 3 4))", 10);
}

#[test]
fn set_unfold_matches_literal_set() {
    assert_true(
        "(set=? (set eqv-comparator 10 20 30 40 50)
                (set-unfold
                  (lambda (i) (= i 0))
                  (lambda (i) (* i 10))
                  (lambda (i) (- i 1))
                  5
                  eqv-comparator))",
    );
}

#[test]
fn set_list_conversions() {
    assert_true("(equal? '(a) (set->list (set eq-comparator 'a)))");
    assert_int("(set-size (list->set eq-comparator '(e f)))", 2);
}

#[test]
fn set_subset_relations() {
    // From sets/subsets.
    assert_true(
        "(let ((set2 (set number-comparator 1 2))
               (other-set2 (set number-comparator 1 2))
               (set3 (set number-comparator 1 2 3))
               (set4 (set number-comparator 1 2 3 4)))
           (and (set=? set2 other-set2)
                (not (set=? set2 set3))
                (set<? set2 set3 set4)
                (not (set<? set2 other-set2))
                (set<=? set2 other-set2 set3)
                (set>? set4 set3 set2)
                (set>=? set3 other-set2 set2)))",
    );
}

#[test]
fn set_theory_operations() {
    // From sets/ops.
    assert_true(
        "(let ((abcd (set eq-comparator 'a 'b 'c 'd))
               (efgh (set eq-comparator 'e 'f 'g 'h))
               (abgh (set eq-comparator 'a 'b 'g 'h))
               (all (set eq-comparator 'a 'b 'c 'd 'e 'f 'g 'h))
               (none (set eq-comparator))
               (ab (set eq-comparator 'a 'b))
               (cdgh (set eq-comparator 'c 'd 'g 'h)))
           (and (set-disjoint? abcd efgh)
                (not (set-disjoint? abcd ab))
                (set=? all (set-union abcd efgh))
                (set=? none (set-intersection abcd efgh))
                (set=? ab (set-intersection abcd abgh))
                (set=? cdgh (set-xor abcd abgh))))",
    );
}

#[test]
fn set_search_insert_and_remove() {
    // From sets/search: insert into a copy yields a larger set, with obj 1.
    assert_true(
        "(let ((yam (set char-comparator #\\y #\\a #\\m))
               (yam! (set char-comparator #\\y #\\a #\\m #\\!)))
           (call-with-values
             (lambda ()
               (set-search! (set-copy yam) #\\!
                            (lambda (insert ignore) (insert 1))
                            error))
             (lambda (s obj) (and (set=? yam! s) (= obj 1)))))",
    );
    assert_true(
        "(let ((yam (set char-comparator #\\y #\\a #\\m))
               (ym (set char-comparator #\\y #\\m)))
           (call-with-values
             (lambda ()
               (set-search! (set-copy yam) #\\a
                            error
                            (lambda (elt update remove) (remove 4))))
             (lambda (s obj) (and (set=? ym s) (= obj 4)))))",
    );
}

#[test]
fn set_filter_partition_find() {
    // From sets/whole.
    assert_true(
        "(let ((whole (set eqv-comparator 1 2 3 4 5 6 7 8 9 10))
               (top (set eqv-comparator 6 7 8 9 10))
               (bottom (set eqv-comparator 1 2 3 4 5))
               (big (lambda (x) (> x 5))))
           (and (set=? top (set-filter big whole))
                (set=? bottom (set-remove big whole))
                (= 5 (set-count big whole))))",
    );
    assert_true(
        "(let ((hetero (set eqv-comparator 1 2 'a 3 4)))
           (and (eqv? 'a (set-find symbol? hetero (lambda () (error \"wrong\"))))
                (set-any? symbol? hetero)
                (not (set-every? symbol? hetero))))",
    );
}

#[test]
fn set_member_with_distinct_equal_element() {
    // From sets/lowlevel: case-insensitive comparator finds stored element.
    assert_true(
        "(let ((bucket (set string-ci-comparator \"abc\" \"def\")))
           (and (set-contains? bucket \"ABC\")
                (string=? \"def\" (set-member bucket \"DEF\" \"fqz\"))
                (string=? \"fqz\" (set-member bucket \"lmn\" \"fqz\"))))",
    );
}

#[test]
fn bag_counts_and_size() {
    // From bags/simple and bags/elemcount: bags track multiplicity.
    assert_int(
        "(let ((nums (bag eqv-comparator)))
           (bag-adjoin! nums 2)
           (bag-adjoin! nums 3)
           (bag-adjoin! nums 4)
           (bag-size (bag-adjoin nums 5)))",
        4,
    );
    assert_int(
        "(bag-element-count (bag eqv-comparator 1 1 1 1 1 2 2) 1)",
        5,
    );
    assert_int(
        "(bag-element-count (bag eqv-comparator 1 1 1 1 1 2 2) 3)",
        0,
    );
    assert_int("(bag-unique-size (bag eqv-comparator 1 1 2))", 2);
}

#[test]
fn bag_subbag_relations() {
    // From bags/subbags: multiplicity matters for the ordering relations.
    assert_true(
        "(let ((bagx (bag number-comparator 10 20 30 40))
               (bagy (bag number-comparator 10 20 20 30 40)))
           (and (bag<? bagx bagy)
                (not (bag<? bagy bagx))
                (bag<=? bagx bagy)
                (bag>=? bagy bagx)))",
    );
}

#[test]
fn bag_sum_and_product() {
    // From bags/sumprod.
    assert_int(
        "(let* ((abb (bag eq-comparator 'a 'b 'b))
                (aab (bag eq-comparator 'a 'a 'b))
                (total (bag-sum abb aab)))
           (bag-count (lambda (x) (eqv? x 'a)) total))",
        3,
    );
    assert_int(
        "(let* ((abb (bag eq-comparator 'a 'b 'b))
                (aab (bag eq-comparator 'a 'a 'b))
                (total (bag-sum abb aab)))
           (bag-size (bag-product 2 total)))",
        12,
    );
}

#[test]
fn bag_set_conversions() {
    // From bags/convert.
    assert_true(
        "(let ((multi (bag eqv-comparator 1 2 2 3 3 3))
               (single (bag eqv-comparator 1 2 3))
               (singleset (set eqv-comparator 1 2 3)))
           (and (set=? singleset (bag->set multi))
                (bag=? single (set->bag singleset))
                (not (bag=? multi (set->bag singleset)))))",
    );
    assert_true("(equal? '((a . 2)) (bag->alist (bag eqv-comparator 'a 'a)))");
}

#[test]
fn set_and_bag_comparators_register_as_default() {
    // From comparators: nested sets use set-comparator for equality.
    assert_true(
        "(let ((sos (set set-comparator
                     (set equal-comparator '(2 . 1) '(1 . 1) '(0 . 2) '(0 . 0))
                     (set equal-comparator '(2 . 1) '(1 . 1) '(0 . 0) '(0 . 2)))))
           (= 1 (set-size sos)))",
    );
    assert_true(
        "(let ((a (set number-comparator 1 2 3)))
           (and (=? set-comparator a (set-copy a))
                (not (=? set-comparator a (set number-comparator 1 2 4)))))",
    );
}
