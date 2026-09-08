//! Boolean operations checked against explicit enumeration.
//!
//! Relations are built as unions of fully specified transitions and then read back
//! with `enumerate_relation`; if `or_rel` were wrong the construction check would
//! fail before any of the operator comparisons ran, so the oracle is not circular.

use mxdcore::prelude::*;
use std::collections::HashSet;

const DOMAINS: [usize; 3] = [2, 3, 2];

fn mgr() -> MxdManager {
    let mut m = MxdManager::new();
    for (i, &n) in DOMAINS.iter().enumerate() {
        m.defvar(&format!("v{i}"), n);
    }
    m
}

/// Every state vector in Ω, in level order.
fn all_states() -> Vec<StateVec> {
    let mut out = vec![vec![]];
    for &n in DOMAINS.iter() {
        let mut next = Vec::new();
        for prefix in &out {
            for a in 0..n {
                let mut v = prefix.clone();
                v.push(a);
                next.push(v);
            }
        }
        out = next;
    }
    out
}

/// xorshift, so the suite stays dependency-free and reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn build_set(m: &mut MxdManager, states: &HashSet<StateVec>) -> NodeId {
    let mut acc = m.zero();
    for s in states {
        let one = m.state(s);
        acc = m.or_set(acc, one);
    }
    acc
}

fn build_rel(m: &mut MxdManager, pairs: &HashSet<Transition>) -> NodeId {
    let mut acc = m.zero();
    for (from, to) in pairs {
        let src: Vec<Src> = from.iter().map(|&v| Src::Val(v)).collect();
        let dst: Vec<Dst> = to.iter().map(|&v| Dst::Val(v)).collect();
        let one = m.mxd_singleton(&src, &dst);
        acc = m.or_rel(acc, one);
    }
    acc
}

fn read_set(m: &MxdManager, node: NodeId) -> HashSet<StateVec> {
    m.enumerate_set(node).into_iter().collect()
}

fn read_rel(m: &MxdManager, node: NodeId) -> HashSet<Transition> {
    m.enumerate_relation(node).into_iter().collect()
}

#[test]
fn test_set_ops_match_oracle() {
    let states = all_states();
    let universe: HashSet<StateVec> = states.iter().cloned().collect();
    let mut rng = Rng(0x5eed_1234_9abc_def1);

    for _ in 0..300 {
        let pick = |rng: &mut Rng| -> HashSet<StateVec> {
            let count = rng.below(states.len() + 1);
            let mut s = HashSet::new();
            for _ in 0..count {
                s.insert(states[rng.below(states.len())].clone());
            }
            s
        };
        let a = pick(&mut rng);
        let b = pick(&mut rng);

        let mut m = mgr();
        let fa = build_set(&mut m, &a);
        let fb = build_set(&mut m, &b);
        assert_eq!(read_set(&m, fa), a, "construction must round-trip");
        assert_eq!(read_set(&m, fb), b);

        let and = m.and_set(fa, fb);
        let or = m.or_set(fa, fb);
        let not = m.not_set(fa);
        let diff = m.setdiff_set(fa, fb);

        assert_eq!(read_set(&m, and), &a & &b);
        assert_eq!(read_set(&m, or), &a | &b);
        assert_eq!(read_set(&m, not), &universe - &a);
        assert_eq!(read_set(&m, diff), &a - &b);

        assert_eq!(m.cardinality_set(and), (&a & &b).len() as u128);
        assert_eq!(m.cardinality_set(not), (&universe - &a).len() as u128);
    }
}

#[test]
fn test_rel_ops_match_oracle() {
    let states = all_states();
    let pairs: Vec<Transition> = states
        .iter()
        .flat_map(|f| states.iter().map(move |t| (f.clone(), t.clone())))
        .collect();
    let universe: HashSet<Transition> = pairs.iter().cloned().collect();
    let mut rng = Rng(0xfeed_face_0bad_c0de);

    for _ in 0..200 {
        let pick = |rng: &mut Rng| -> HashSet<Transition> {
            let count = rng.below(9);
            let mut s = HashSet::new();
            for _ in 0..count {
                s.insert(pairs[rng.below(pairs.len())].clone());
            }
            s
        };
        let a = pick(&mut rng);
        let b = pick(&mut rng);

        let mut m = mgr();
        let fa = build_rel(&mut m, &a);
        let fb = build_rel(&mut m, &b);
        assert_eq!(read_rel(&m, fa), a, "construction must round-trip");
        assert_eq!(read_rel(&m, fb), b);

        let and = m.and_rel(fa, fb);
        let or = m.or_rel(fa, fb);
        let not = m.not_rel(fa);
        let diff = m.setdiff_rel(fa, fb);

        assert_eq!(read_rel(&m, and), &a & &b);
        assert_eq!(read_rel(&m, or), &a | &b);
        assert_eq!(read_rel(&m, not), &universe - &a);
        assert_eq!(read_rel(&m, diff), &a - &b);

        assert_eq!(m.cardinality_relation(and), (&a & &b).len() as u128);
        assert_eq!(m.cardinality_relation(not), (&universe - &a).len() as u128);
    }
}

/// The identity is where the set/relation asymmetry actually bites: complementing
/// it must give every off-diagonal pair, not the empty relation, and that only
/// works if `One` is read as the diagonal rather than as `Ω × Ω`.
#[test]
fn test_identity_complement() {
    let mut m = mgr();
    let k = DOMAINS.len();
    let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);

    let omega = m.state_space_size();
    assert_eq!(m.cardinality_relation(id), omega);

    let off = m.not_rel(id);
    assert_eq!(
        m.cardinality_relation(off),
        omega * omega - omega,
        "the complement of the identity is every pair that moves"
    );
    assert!(
        m.enumerate_relation(off).iter().all(|(f, t)| f != t),
        "no fixed point may survive"
    );

    // Involution, and the identity partitions Ω × Ω with its complement.
    let back = m.not_rel(off);
    assert_eq!(back, id, "complement is an involution");
    let all = m.or_rel(id, off);
    assert_eq!(m.cardinality_relation(all), omega * omega);
    let none = m.and_rel(id, off);
    assert_eq!(none, m.zero());
}

/// Intersecting an arbitrary relation with the identity extracts its fixed points.
#[test]
fn test_intersect_with_identity() {
    let mut m = mgr();
    let k = DOMAINS.len();
    let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);

    let mut pairs = HashSet::new();
    pairs.insert((vec![0, 0, 0], vec![0, 0, 0])); // a fixed point
    pairs.insert((vec![1, 2, 1], vec![1, 2, 1])); // another
    pairs.insert((vec![0, 1, 0], vec![1, 1, 0])); // moves
    let r = build_rel(&mut m, &pairs);

    let fixed = m.and_rel(r, id);
    let expected: HashSet<Transition> = pairs.iter().filter(|(f, t)| f == t).cloned().collect();
    assert_eq!(read_rel(&m, fixed), expected);
}

/// `One` read as a relation *is* the identity, and must behave identically to the
/// explicitly built identity spine.
///
/// This is the only way to reach the skipped-level arms of the relation operations
/// right now: `create_rel_node` is quasi-reduced, so it never elides a level and
/// nothing else produces a relation node that skips one. Those arms exist for
/// identity reduction, and this test is what keeps them honest until then — under
/// identity reduction the spine below will collapse to `One` and the two operands
/// stop merely agreeing and become the same node.
#[test]
fn test_one_terminal_is_the_identity_relation() {
    let mut m = mgr();
    let k = DOMAINS.len();
    let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);
    let one = m.one();

    assert_eq!(
        m.cardinality_relation(one),
        m.state_space_size(),
        "the One terminal denotes the diagonal, not the full relation"
    );
    assert_eq!(read_rel(&m, one), read_rel(&m, id));

    let mut pairs = HashSet::new();
    pairs.insert((vec![0, 0, 0], vec![0, 0, 0]));
    pairs.insert((vec![1, 2, 1], vec![1, 2, 1]));
    pairs.insert((vec![0, 1, 0], vec![1, 1, 0]));
    pairs.insert((vec![1, 0, 1], vec![0, 2, 0]));
    let r = build_rel(&mut m, &pairs);

    // (Some, None) and (None, Some) arms.
    let via_one = m.and_rel(r, one);
    let via_spine = m.and_rel(r, id);
    assert_eq!(read_rel(&m, via_one), read_rel(&m, via_spine));
    let via_one = m.or_rel(r, one);
    let via_spine = m.or_rel(r, id);
    assert_eq!(read_rel(&m, via_one), read_rel(&m, via_spine));

    // (None, None) arm, and the skipped-level branch of `not`.
    assert_eq!(read_rel(&m, one), {
        let both = m.and_rel(one, one);
        read_rel(&m, both)
    });
    let not_one = m.not_rel(one);
    let not_spine = m.not_rel(id);
    assert_eq!(read_rel(&m, not_one), read_rel(&m, not_spine));
    assert_eq!(
        m.cardinality_relation(not_one),
        m.state_space_size() * m.state_space_size() - m.state_space_size()
    );
}

#[test]
fn test_gc_survives_cached_ops() {
    let mut m = mgr();
    let k = DOMAINS.len();
    let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);
    let off = m.not_rel(id);
    let before = m.enumerate_relation(off);

    // Build garbage, collect it, then recompute through the (now cleared) tables.
    for a in 0..2 {
        let _ = m.mxd_singleton(
            &[Src::Val(a), Src::Val(0), Src::Val(0)],
            &[Dst::Val(0), Dst::Val(1), Dst::Val(1)],
        );
    }
    m.gc(&[id, off]);

    assert_eq!(m.enumerate_relation(off), before);
    let again = m.not_rel(id);
    assert_eq!(again, off, "recomputation after gc must be consistent");
}
