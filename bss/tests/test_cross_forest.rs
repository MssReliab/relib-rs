//! Handles are only meaningful in the forest that issued them.
//!
//! A node handle carries a node id and a `Weak` back-reference to its manager, but
//! the binary operations used to read only the id from the other operand — and two
//! managers number their nodes identically, so mixing them did not fail. It
//! computed on the wrong diagram and returned a plausible wrong answer.
//!
//! `relibmss` has always checked this on the Python side (`other.mdd is not
//! self.mdd` → `ValueError`); callers using the Rust crates directly were
//! unprotected. These tests pin the guards that close that.

use bss::prelude::*;

fn two_managers() -> (BddMgr, BddMgr) {
    let mut a = BddMgr::new();
    let mut b = BddMgr::new();
    // Same variables, declared in the same order: the two forests are structurally
    // identical, which is exactly what makes the confusion silent.
    for m in [&mut a, &mut b] {
        m.defvar("x");
        m.defvar("y");
    }
    (a, b)
}

#[test]
#[should_panic(expected = "different managers")]
fn test_bdd_and_across_forests() {
    let (mut a, mut b) = two_managers();
    let xa = a.defvar("x");
    let yb = b.defvar("y");
    let _ = xa.and(&yb);
}

#[test]
#[should_panic(expected = "different managers")]
fn test_bdd_or_across_forests() {
    let (mut a, mut b) = two_managers();
    let xa = a.defvar("x");
    let yb = b.defvar("y");
    let _ = xa.or(&yb);
}

#[test]
#[should_panic(expected = "different managers")]
fn test_bdd_ite_across_forests() {
    let (mut a, mut b) = two_managers();
    let xa = a.defvar("x");
    let ya = a.defvar("y");
    let yb = b.defvar("y");
    let _ = xa.ite(&ya, &yb);
}

#[test]
#[should_panic(expected = "different manager")]
fn test_bdd_mgr_and_rejects_a_foreign_node() {
    let (a, mut b) = two_managers();
    let yb = b.defvar("y");
    let _ = a.and(&[yb]);
}

/// Within one forest everything still works — the guard must not cost correctness.
#[test]
fn test_same_forest_is_unaffected() {
    let mut a = BddMgr::new();
    let x = a.defvar("x");
    let y = a.defvar("y");
    let both = x.and(&y);
    let either = x.or(&y);
    assert!(!both.eq(&either));

    let pv: std::collections::HashMap<String, f64> =
        [("x".to_string(), 0.5f64), ("y".to_string(), 0.5f64)]
            .into_iter()
            .collect();
    let p_both: f64 = both.prob(&pv, &[true]);
    let p_either: f64 = either.prob(&pv, &[true]);
    assert!((p_both - 0.25).abs() < 1e-12, "{p_both}");
    assert!((p_either - 0.75).abs() < 1e-12, "{p_either}");
}
