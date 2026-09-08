//! Handles are only meaningful in the forest that issued them.
//!
//! See `bss/tests/test_cross_forest.rs` for the reasoning; this covers the
//! multi-state side, which has the larger operator surface.

use mss::prelude::*;

fn two_managers() -> (MddMgr<i32>, MddMgr<i32>) {
    let mut a: MddMgr<i32> = MddMgr::new();
    let mut b: MddMgr<i32> = MddMgr::new();
    for m in [&mut a, &mut b] {
        m.defvar("x", 3);
        m.defvar("y", 3);
    }
    (a, b)
}

/// Every binary operator on `MddNode`, checked the same way. If one is ever added
/// without a guard this list is where it should show up.
macro_rules! cross_forest_panics {
    ($($name:ident => $op:ident),* $(,)?) => {
        $(
            #[test]
            #[should_panic(expected = "different managers")]
            fn $name() {
                let (mut a, mut b) = two_managers();
                let xa = a.defvar("x", 3);
                let yb = b.defvar("y", 3);
                let _ = xa.$op(&yb);
            }
        )*
    };
}

cross_forest_panics! {
    test_add => add,
    test_sub => sub,
    test_mul => mul,
    test_div => div,
    test_min => min,
    test_max => max,
    test_eq  => eq,
    test_ne  => ne,
    test_lt  => lt,
    test_le  => le,
    test_gt  => gt,
    test_ge  => ge,
    test_and => and,
    test_or  => or,
    test_xor => xor,
}

#[test]
#[should_panic(expected = "different managers")]
fn test_ite() {
    let (mut a, mut b) = two_managers();
    let xa = a.defvar("x", 3);
    let ya = a.defvar("y", 3);
    let yb = b.defvar("y", 3);
    let _ = xa.and(&ya).ite(&ya, &yb);
}

#[test]
#[should_panic(expected = "different manager")]
fn test_mgr_min_rejects_a_foreign_node() {
    let (a, mut b) = two_managers();
    let yb = b.defvar("y", 3);
    let _ = a.min(&[yb]);
}

/// The ZMDD families that `minpath` returns carry the same hazard: two `MssMgr`s
/// each own a ZMDD forest, and their node ids coincide.
#[test]
#[should_panic(expected = "different managers")]
fn test_zmdd_intersect_across_forests() {
    let mut a: MssMgr<i32> = MssMgr::new();
    let mut b: MssMgr<i32> = MssMgr::new();
    let (xa, ya) = (a.defvar("x", 3), a.defvar("y", 3));
    let (xb, yb) = (b.defvar("x", 3), b.defvar("y", 3));
    let fa = a.minpath(&xa.min(&ya)).expect("coherent");
    let fb = b.minpath(&xb.min(&yb)).expect("coherent");
    let _ = fa.intersect(&fb);
}

/// Within one forest nothing changes.
#[test]
fn test_same_forest_is_unaffected() {
    let mut m: MssMgr<i32> = MssMgr::new();
    let x = m.defvar("x", 3);
    let y = m.defvar("y", 3);
    let phi = x.min(&y);
    let family = m.minpath(&phi).expect("min is coherent");
    // Series system: the minimal vector for level j is (j, j).
    assert_eq!(family.extract_level(1).len(), 1);
    assert_eq!(family.extract_level(2).len(), 1);

    let same = m.minpath(&x.min(&y)).expect("coherent");
    let both = family.intersect(&same);
    assert_eq!(both.extract_level(2).len(), 1);
}
