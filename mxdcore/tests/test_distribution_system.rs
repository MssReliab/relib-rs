//! The non-monotone case study, pinned against MEDDLY's published numbers.
//!
//! `examples/boundary_nonmonotone.rs` computes the boundary operator on
//! `distribution_system(n)` under `degrade ∪ repair`. This test locks the small
//! end of that computation to the values MEDDLY produces via
//! `MDDMinsol/scripts/boundary_nonmonotone.jl`, so the example cannot rot and the
//! agreement is not a claim that has to be re-established by hand.
//!
//! Only cardinalities are pinned. Relation node counts differ from MEDDLY's by
//! construction and are not asserted; see the crate docs.

mod common;

use common::{distribution_system, phi};
use mxdcore::analysis::*;

/// `(n, |B_1|, |B_2|)` as computed by MEDDLY.
const REFERENCE: &[(usize, u128, u128)] = &[
    (2, 4, 4),
    (3, 9, 9),
    (4, 16, 32),
    (5, 25, 155),
    (6, 36, 612),
    (7, 49, 1918),
    (8, 64, 5048),
    (9, 81, 11673),
    (10, 100, 24460),
];

/// The relation the reference script uses: one component moves one step, either
/// way.
fn both_ways(sys: &mut System) -> Transitions {
    let dec = sys.degrade();
    let inc = sys.repair();
    sys.union(dec, inc)
}

#[test]
fn test_boundary_matches_meddly_reference() {
    for &(n, b1, b2) in REFERENCE {
        let (mut sys, levels) = distribution_system(n);
        let rel = both_ways(&mut sys);

        let mut got = Vec::new();
        for j in levels.interior() {
            let up = sys.boundary_up(&levels, j, rel);
            let down = sys.boundary_down(&levels, j, rel);
            // The relation is symmetric (every step has its reverse), so the two
            // directions must have equal counts -- as MEDDLY also reports.
            assert_eq!(
                sys.count(up),
                sys.count(down),
                "n={n} j={j}: up and down boundaries should balance"
            );
            got.push(sys.count(up));
        }
        assert_eq!(got, vec![b1, b2], "n={n}: boundary cardinalities vs MEDDLY");
    }
}

/// `|B_1| = n²` exactly, which is also what the paper draft states. Worth pinning
/// as a closed form rather than only as a table: `B_1` is the set of steps from
/// `φ = 0` (sum ≤ 1) up to `φ ≥ 1` (sum ≥ 2), and from each of the `n` states with
/// sum 1 there are exactly `n` increments that raise the sum.
#[test]
fn test_b1_is_n_squared() {
    for n in 2..=12usize {
        let (mut sys, levels) = distribution_system(n);
        let rel = both_ways(&mut sys);
        let b = sys.boundary_up(&levels, 1, rel);
        assert_eq!(sys.count(b), (n * n) as u128, "n={n}");
    }
}

/// The system really is non-monotone, which is the reason the boundary operator is
/// being used at all: if it were monotone, minimal vectors would apply and this
/// whole route would be unnecessary. Monotone here would mean the upper set is
/// closed under increments; `φ ≥ 2` is not, because overproduction drops φ to 1.
#[test]
fn test_system_is_actually_non_monotone() {
    let n = 8;
    let (mut sys, levels) = distribution_system(n);
    let inc = sys.repair();
    let upper2 = levels.upper(2);

    // Under a monotone system every repair step out of the upper set would land
    // back inside it. Here some land outside, and that is exactly the overflow.
    let image = sys.step_forward(upper2, inc);
    let escaped = sys.difference_states(image, upper2);
    assert_ne!(
        escaped,
        sys.no_states(),
        "repairing must be able to leave the upper set"
    );

    // Sanity: the escapes are all the overflow case, φ = 1, i.e. sum ≥ 7. Built in
    // *this* system — handles are not portable between systems, and the API says so.
    let overflow = sys
        .levels_from_fold(0usize, |a, _, v| (a + v).min(7), |&a| usize::from(a >= 7))
        .upper(1);
    let outside = sys.difference_states(escaped, overflow);
    assert_eq!(
        outside,
        sys.no_states(),
        "escapes must all be the overflow branch"
    );

    // And the boundary does cross back down, which is what the downward boundary
    // measures.
    let dec = sys.degrade();
    let rel = sys.union(dec, inc);
    let down = sys.boundary_down(&levels, 2, rel);
    assert!(sys.count(down) > 0);
}

/// φ's own arithmetic, independent of any diagram: the drop is between Σ = 6 and
/// Σ = 7, where production passes `ymax`.
#[test]
fn test_the_overflow_step() {
    assert_eq!(phi(6), 2, "3·6 = 18 is inside the band");
    assert_eq!(phi(7), 1, "3·7 = 21 overflows, and φ falls");
    assert_eq!(phi(1), 0, "3·1 = 3 is below ymin");
}
