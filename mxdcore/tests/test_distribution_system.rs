//! The non-monotone case study, pinned against MEDDLY's published numbers.
//!
//! `examples/boundary_nonmonotone.rs` computes the boundary operator on
//! `distribution_system(n)` (Sedlacek et al. 2021, RESS 215:107824 §4.2). This
//! test locks the small end of that computation to the values MEDDLY produces via
//! `MDDMinsol/scripts/boundary_nonmonotone.jl`, so the example cannot rot and the
//! agreement is not a claim that has to be re-established by hand.
//!
//! Only cardinalities are pinned. Relation node counts differ from MEDDLY's by
//! construction and are not asserted; see the crate docs.

use mxdcore::prelude::*;

const T: usize = 3;
const YMIN: usize = 5;
const YMAX: usize = 20;
const STATES: usize = 3;
const SAT: usize = YMAX / T + 1;

fn phi(sum: usize) -> usize {
    let p = T * sum;
    if p < YMIN {
        0
    } else if p > YMAX {
        1
    } else {
        2
    }
}

fn level_set(m: &mut MxdManager, n: usize, keep: &dyn Fn(usize) -> bool) -> NodeId {
    fn rec(
        m: &mut MxdManager,
        level: i64,
        acc: usize,
        keep: &dyn Fn(usize) -> bool,
        memo: &mut std::collections::HashMap<(i64, usize), NodeId>,
    ) -> NodeId {
        if level < 0 {
            return if keep(acc) { m.one() } else { m.zero() };
        }
        if let Some(&hit) = memo.get(&(level, acc)) {
            return hit;
        }
        let children: Vec<NodeId> = (0..STATES)
            .map(|v| rec(m, level - 1, (acc + v).min(SAT), keep, memo))
            .collect();
        let node = m.create_set_node(level as usize, &children);
        memo.insert((level, acc), node);
        node
    }
    rec(m, n as i64 - 1, 0, keep, &mut std::collections::HashMap::new())
}

fn transition_relation(m: &mut MxdManager, n: usize) -> NodeId {
    let mut rel = m.zero();
    for i in 0..n {
        for v in 0..(STATES - 1) {
            for (from, to) in [(v, v + 1), (v + 1, v)] {
                let mut src = vec![Src::Any; n];
                let mut dst = vec![Dst::Same; n];
                src[i] = Src::Val(from);
                dst[i] = Dst::Val(to);
                let e = m.mxd_singleton(&src, &dst);
                rel = m.or_rel(rel, e);
            }
        }
    }
    rel
}

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

#[test]
fn test_boundary_matches_meddly_reference() {
    for &(n, b1, b2) in REFERENCE {
        let mut m = MxdManager::new();
        for i in 0..n {
            m.defvar(&format!("x{i}"), STATES);
        }
        let rel = transition_relation(&mut m, n);

        let mut got = Vec::new();
        for j in 1..STATES {
            let upper = level_set(&mut m, n, &move |s| phi(s) >= j);
            let lower = level_set(&mut m, n, &move |s| phi(s) < j);
            let up = m.boundary(lower, upper, rel);
            let down = m.boundary(upper, lower, rel);
            // The relation is symmetric (every step has its reverse), so the two
            // directions must have equal counts -- as MEDDLY also reports.
            assert_eq!(
                m.cardinality_relation(up),
                m.cardinality_relation(down),
                "n={n} j={j}: up and down boundaries should balance"
            );
            got.push(m.cardinality_relation(up));
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
        let mut m = MxdManager::new();
        for i in 0..n {
            m.defvar(&format!("x{i}"), STATES);
        }
        let rel = transition_relation(&mut m, n);
        let upper = level_set(&mut m, n, &|s| phi(s) >= 1);
        let lower = level_set(&mut m, n, &|s| phi(s) < 1);
        let b = m.boundary(lower, upper, rel);
        assert_eq!(m.cardinality_relation(b), (n * n) as u128, "n={n}");
    }
}

/// The system really is non-monotone, which is the reason the boundary operator is
/// being used at all: if it were monotone, minimal vectors would apply and this
/// whole route would be unnecessary. Monotone here would mean the upper set is
/// closed under increments; `φ ≥ 2` is not, because overproduction drops φ to 1.
#[test]
fn test_system_is_actually_non_monotone() {
    let n = 8;
    let mut m = MxdManager::new();
    for i in 0..n {
        m.defvar(&format!("x{i}"), STATES);
    }
    let rel = transition_relation(&mut m, n);
    let upper2 = level_set(&mut m, n, &|s| phi(s) >= 2);

    // Increment-only part of the relation.
    let mut inc = m.zero();
    for i in 0..n {
        for v in 0..(STATES - 1) {
            let mut src = vec![Src::Any; n];
            let mut dst = vec![Dst::Same; n];
            src[i] = Src::Val(v);
            dst[i] = Dst::Val(v + 1);
            let e = m.mxd_singleton(&src, &dst);
            inc = m.or_rel(inc, e);
        }
    }

    // Under a monotone system every increment out of the upper set would land back
    // inside it. Here some land outside, and that is exactly the overflow.
    let image = m.post_image(upper2, inc);
    let escaped = m.setdiff_set(image, upper2);
    assert_ne!(
        escaped,
        m.zero(),
        "incrementing must be able to leave the upper set"
    );

    // Sanity: the escapes are all the overflow case, φ = 1, i.e. sum ≥ 7.
    let overflow = level_set(&mut m, n, &|s| s >= 7);
    let outside = m.setdiff_set(escaped, overflow);
    assert_eq!(outside, m.zero(), "escapes must all be the overflow branch");

    // And the full relation does cross back down, which is what the downward
    // boundary measures.
    let lower2 = level_set(&mut m, n, &|s| phi(s) < 2);
    let down = m.boundary(upper2, lower2, rel);
    assert!(m.cardinality_relation(down) > 0);
}
