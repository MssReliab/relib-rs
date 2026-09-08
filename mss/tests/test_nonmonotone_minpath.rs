//! Why the boundary operator exists: `minpath` correctly refuses the system it is
//! used on.
//!
//! `distribution_system(n)` (Sedlacek, Zaitseva, Levashenko and Kvassay 2021,
//! RESS 215:107824 §4.2) is non-monotone — production `P = 3·Σxᵢ` with
//! `φ = 0 (P<5) / 1 (P>20) / 2 (otherwise)`, so overproduction drops φ from 2 to 1.
//! Minimal path and cut vectors are not defined for such a φ, and this crate says
//! so rather than returning something meaningless.
//!
//! The complement of this test lives in `relib-mxd`: the boundary operator
//! `B = R ∩ (L × U)` is defined for exactly the same φ and produces real numbers
//! from it, cross-checked against MEDDLY. Together the two say what the method is
//! for — the same system, one route undefined and the other not.

use mss::prelude::*;

const T: usize = 3;
const YMIN: usize = 5;
const YMAX: usize = 20;
const STATES: usize = 3;

fn phi_of_sum(sum: usize) -> i32 {
    let p = T * sum;
    if p < YMIN {
        0
    } else if p > YMAX {
        1
    } else {
        2
    }
}

/// Build φ as an MTMDD, bottom-up, one component per level.
fn build(
    mgr: &MssMgr<i32>,
    headers: &[HeaderId],
    n: usize,
    level: i64,
    sum: usize,
) -> MddNode<i32> {
    if level < 0 {
        return mgr.value(phi_of_sum(sum));
    }
    let l = level as usize;
    let children: Vec<MddNode<i32>> = (0..STATES)
        .map(|v| build(mgr, headers, n, level - 1, sum + v))
        .collect();
    mgr.create_node(headers[l], &children)
}

fn distribution_system(n: usize) -> (MssMgr<i32>, MddNode<i32>) {
    let mut mgr: MssMgr<i32> = MssMgr::new();
    let headers: Vec<HeaderId> = (0..n)
        .map(|i| {
            mgr.defvar(&format!("x{i}"), STATES)
                .get_header()
                .expect("a declared variable has a header")
        })
        .collect();
    let phi = build(&mgr, &headers, n, n as i64 - 1, 0);
    (mgr, phi)
}

#[test]
fn test_minpath_refuses_the_non_monotone_distribution_system() {
    // n must be large enough for the overflow branch to be reachable: φ = 1 needs
    // 3·Σxᵢ > 20, i.e. Σxᵢ ≥ 7, and each component contributes at most 2.
    for n in 4..=6usize {
        let (mgr, phi) = distribution_system(n);
        assert!(
            mgr.minpath(&phi).is_none(),
            "n={n}: minpath must refuse a non-monotone φ"
        );
        assert!(
            mgr.mincut(&phi).is_none(),
            "n={n}: mincut must refuse it too"
        );
    }
}

/// The refusal has to be about *this* system, not about the construction. Below
/// the overflow threshold the very same builder gives a monotone φ, and then both
/// are defined.
#[test]
fn test_the_same_construction_is_accepted_when_monotone() {
    // With n = 3 the sum reaches at most 6, so 3·Σxᵢ ≤ 18 < 20 and the overflow
    // branch is unreachable: φ is non-decreasing.
    let (mgr, phi) = distribution_system(3);
    assert!(
        mgr.minpath(&phi).is_some(),
        "without the overflow branch φ is monotone and minpath should succeed"
    );
    assert!(mgr.mincut(&phi).is_some());
}

/// And the non-monotonicity is exactly the overflow, not an artefact: raising a
/// component from a state where φ = 2 can land on φ = 1.
#[test]
fn test_the_witness_is_the_overflow() {
    let n = 8;
    let mut found = None;
    // Σ = 6 gives φ = 2 (P = 18); Σ = 7 gives φ = 1 (P = 21).
    for sum in 0..=(2 * n) {
        if phi_of_sum(sum) == 2 && sum + 1 <= 2 * n && phi_of_sum(sum + 1) < 2 {
            found = Some(sum);
            break;
        }
    }
    let sum = found.expect("the overflow step must exist");
    assert_eq!(sum, 6, "the drop happens between Σ = 6 and Σ = 7");
    assert_eq!(phi_of_sum(6), 2);
    assert_eq!(phi_of_sum(7), 1, "one more unit of production lowers φ");

    // …and the diagram agrees that this φ is not coherent.
    let (mgr, phi) = distribution_system(n);
    assert!(mgr.minpath(&phi).is_none());
}
