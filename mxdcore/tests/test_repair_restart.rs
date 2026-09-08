//! Repair and restart boundaries on the non-monotone distribution system.
//!
//! Pins `examples/repair_restart.rs` against the numbers MEDDLY produces, and
//! against the structural facts that make those numbers interesting — the
//! wrong-direction crossings a monotone system cannot have.
//!
//! Cardinalities only; relation node counts differ from MEDDLY's by construction.

use mxdcore::prelude::*;
use std::collections::HashMap;

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

fn mgr(n: usize) -> MxdManager {
    let mut m = MxdManager::new();
    for i in 0..n {
        m.defvar(&format!("x{i}"), STATES);
    }
    m
}

fn level_set(m: &mut MxdManager, n: usize, keep: &dyn Fn(usize) -> bool) -> NodeId {
    fn rec(
        m: &mut MxdManager,
        level: i64,
        acc: usize,
        keep: &dyn Fn(usize) -> bool,
        memo: &mut HashMap<(i64, usize), NodeId>,
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
    rec(m, n as i64 - 1, 0, keep, &mut HashMap::new())
}

fn relation(m: &mut MxdManager, n: usize, steps: &[(usize, usize)]) -> NodeId {
    let mut rel = m.zero();
    for i in 0..n {
        for &(from, to) in steps {
            let mut src = vec![Src::Any; n];
            let mut dst = vec![Dst::Same; n];
            src[i] = Src::Val(from);
            dst[i] = Dst::Val(to);
            let e = m.mxd_singleton(&src, &dst);
            rel = m.or_rel(rel, e);
        }
    }
    rel
}

fn dec(m: &mut MxdManager, n: usize) -> NodeId {
    let steps: Vec<(usize, usize)> = (1..STATES).map(|v| (v, v - 1)).collect();
    relation(m, n, &steps)
}
fn inc(m: &mut MxdManager, n: usize) -> NodeId {
    let steps: Vec<(usize, usize)> = (0..STATES - 1).map(|v| (v, v + 1)).collect();
    relation(m, n, &steps)
}
fn restart(m: &mut MxdManager, n: usize) -> NodeId {
    let steps: Vec<(usize, usize)> = (0..STATES - 1).map(|v| (v, STATES - 1)).collect();
    relation(m, n, &steps)
}

fn sets(m: &mut MxdManager, n: usize, j: usize) -> (NodeId, NodeId) {
    let lower = level_set(m, n, &move |s| phi(s) < j);
    let upper = level_set(m, n, &move |s| phi(s) >= j);
    (lower, upper)
}

/// `(n, relation, j, up, down)` as produced by MEDDLY through
/// `MDDMinsol.boundary_level` / `boundary_level_down`.
///
/// Transcribed mechanically from the reference CSV, not by hand. The first
/// attempt at this table was typed out, and two of its entries were numbers that
/// appear nowhere in the data — the test caught it, but only because it happened
/// to disagree rather than happening to match.
const REFERENCE: &[(usize, &str, usize, u128, u128)] = &[
    (4, "dec", 1, 0, 16),
    (4, "dec", 2, 16, 16),
    (4, "inc", 1, 16, 0),
    (4, "inc", 2, 16, 16),
    (4, "restart", 1, 20, 0),
    (4, "restart", 2, 20, 28),
    (4, "loop", 1, 20, 16),
    (4, "loop", 2, 36, 44),
    (8, "dec", 1, 0, 64),
    (8, "dec", 2, 4984, 64),
    (8, "inc", 1, 64, 0),
    (8, "inc", 2, 64, 4984),
    (8, "restart", 1, 72, 0),
    (8, "restart", 2, 72, 7112),
    (8, "loop", 1, 72, 64),
    (8, "loop", 2, 5056, 7176),
    (12, "dec", 1, 0, 144),
    (12, "dec", 2, 86328, 144),
    (12, "inc", 1, 144, 0),
    (12, "inc", 2, 144, 86328),
    (12, "restart", 1, 156, 0),
    (12, "restart", 2, 156, 113652),
    (12, "loop", 1, 156, 144),
    (12, "loop", 2, 86484, 113796),
    (14, "dec", 1, 0, 196),
    (14, "dec", 2, 248248, 196),
    (14, "inc", 1, 196, 0),
    (14, "inc", 2, 196, 248248),
    (14, "restart", 1, 210, 0),
    (14, "restart", 2, 210, 318318),
    (14, "loop", 1, 210, 196),
    (14, "loop", 2, 248458, 318514),
];

#[test]
fn test_boundaries_match_meddly() {
    for &(n, name, j, want_up, want_down) in REFERENCE {
        let mut m = mgr(n);
        let rel = match name {
            "dec" => dec(&mut m, n),
            "inc" => inc(&mut m, n),
            "restart" => restart(&mut m, n),
            "loop" => {
                let d = dec(&mut m, n);
                let r = restart(&mut m, n);
                m.or_rel(d, r)
            }
            other => panic!("unknown relation `{other}`"),
        };
        let (lower, upper) = sets(&mut m, n, j);
        let up = m.boundary(lower, upper, rel);
        let down = m.boundary(upper, lower, rel);
        assert_eq!(
            m.cardinality_relation(up),
            want_up,
            "n={n} {name} j={j}: upward boundary"
        );
        assert_eq!(
            m.cardinality_relation(down),
            want_down,
            "n={n} {name} j={j}: downward boundary"
        );
    }
}

/// The point of the whole exercise: degrading a component can move the system
/// **into** the better level set, and repairing one can move it **out**. Neither
/// is possible when φ is monotone, and neither is expressible as a minimal path or
/// cut vector.
#[test]
fn test_wrong_direction_crossings_exist() {
    for n in [4usize, 8, 12] {
        let mut m = mgr(n);
        let d = dec(&mut m, n);
        let i = inc(&mut m, n);
        let (lower, upper) = sets(&mut m, n, 2);

        let dec_up = m.boundary(lower, upper, d);
        let inc_down = m.boundary(upper, lower, i);
        assert!(
            m.cardinality_relation(dec_up) > 0,
            "n={n}: degrading must be able to enter the upper set"
        );
        assert!(
            m.cardinality_relation(inc_down) > 0,
            "n={n}: repairing must be able to leave it"
        );

        // At level 1 the system is still ordinary: below `ymin` nothing overflows,
        // so degrading only ever crosses downward there.
        let (l1, u1) = sets(&mut m, n, 1);
        let dec_up1 = m.boundary(l1, u1, d);
        assert_eq!(
            m.cardinality_relation(dec_up1),
            0,
            "n={n}: at level 1 degrading cannot cross upward"
        );
    }
}

/// `inc` is the converse of `dec`, so their boundaries must be converses too:
/// `transpose(dec ∩ (L × U)) == inc ∩ (U × L)`. This ties the two wrong-direction
/// numbers together as one fact rather than two coincidences, and it fails if
/// either `transpose` or `boundary` reads a transition backwards.
#[test]
fn test_boundaries_are_converse_under_transpose() {
    for n in [3usize, 5, 9] {
        let mut m = mgr(n);
        let d = dec(&mut m, n);
        let i = inc(&mut m, n);
        assert_eq!(m.transpose(d), i, "n={n}: dec and inc must be converses");

        for j in 1..STATES {
            let (lower, upper) = sets(&mut m, n, j);
            let dec_up = m.boundary(lower, upper, d);
            let inc_down = m.boundary(upper, lower, i);
            assert_eq!(
                m.transpose(dec_up),
                inc_down,
                "n={n} j={j}: boundaries must be converses"
            );
        }
    }
}

/// Restarting overshoots harder than repairing a step at a time: it jumps a
/// component straight to the top, so it drops out of `{φ ≥ 2}` more often than
/// `inc` does. That is the operational cost of restart-on-failure here, and it is
/// only visible as a transition count.
#[test]
fn test_restart_overshoots_more_than_gradual_repair() {
    for n in [8usize, 12, 16] {
        let mut m = mgr(n);
        let i = inc(&mut m, n);
        let r = restart(&mut m, n);
        let (lower, upper) = sets(&mut m, n, 2);

        let inc_b = m.boundary(upper, lower, i);
        let restart_b = m.boundary(upper, lower, r);
        let inc_down = m.cardinality_relation(inc_b);
        let restart_down = m.cardinality_relation(restart_b);
        assert!(
            restart_down > inc_down,
            "n={n}: restart ({restart_down}) should leave the upper set more often \
             than a single repair step ({inc_down})"
        );
    }
}

/// The operational loop — degrade a step, restore to new — is the union of the
/// two, and its boundaries are the unions of theirs.
#[test]
fn test_loop_is_the_union_of_its_parts() {
    let n = 10;
    let mut m = mgr(n);
    let d = dec(&mut m, n);
    let r = restart(&mut m, n);
    let looped = m.or_rel(d, r);
    let (lower, upper) = sets(&mut m, n, 2);

    for (l, u) in [(lower, upper), (upper, lower)] {
        let both = m.boundary(l, u, looped);
        let parts = {
            let a = m.boundary(l, u, d);
            let b = m.boundary(l, u, r);
            m.or_rel(a, b)
        };
        assert_eq!(both, parts, "the boundary of a union is the union of boundaries");
    }
}
