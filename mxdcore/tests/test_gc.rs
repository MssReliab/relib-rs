//! Garbage collection, and the hazard it creates for the level-keyed caches.
//!
//! `gc` does not compact, so reclaimed slots are handed back out to new nodes. The
//! relation tables are keyed `(f, g, level)` rather than by an op code, so
//! `retain_live3` cannot be used to filter them — the level word would be tested as
//! a node id — and they are cleared outright instead. If that clearing were ever
//! dropped, a recycled slot would let a stale entry answer for a different node,
//! and the failure would only appear under memory pressure: exactly the large runs
//! this crate exists for. These tests force the reuse deliberately.

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

#[test]
fn test_gc_keeps_roots_and_reclaims_rest() {
    let mut m = mgr();
    let keep = m.mxd_singleton(
        &[Src::Any, Src::Val(0), Src::Any],
        &[Dst::Same, Dst::Val(1), Dst::Same],
    );
    let _garbage = m.mxd_singleton(
        &[Src::Val(1), Src::Val(2), Src::Val(0)],
        &[Dst::Val(0), Dst::Val(0), Dst::Val(1)],
    );

    let before = m.live_node_count();
    let reclaimed = m.gc(&[keep]);
    assert!(reclaimed > 0);
    assert_eq!(m.live_node_count(), before - reclaimed);

    // Survivors keep their identity under hash-consing.
    let again = m.mxd_singleton(
        &[Src::Any, Src::Val(0), Src::Any],
        &[Dst::Same, Dst::Val(1), Dst::Same],
    );
    assert_eq!(again, keep);
}

#[test]
fn test_gc_with_no_roots_frees_every_nonterminal() {
    let mut m = mgr();
    let _ = m.mxd_singleton(
        &[Src::Val(0), Src::Val(0), Src::Val(0)],
        &[Dst::Val(1), Dst::Val(1), Dst::Val(1)],
    );
    m.gc(&[]);
    assert_eq!(m.live_node_count(), 2, "only Zero and One remain");
}

/// The real hazard: collect, let new nodes take the reclaimed slots, and check
/// that nothing computed afterwards is answered by a stale memo.
#[test]
fn test_recycled_slots_do_not_corrupt_results() {
    let states = all_states();
    let pairs: Vec<Transition> = states
        .iter()
        .flat_map(|f| states.iter().map(move |t| (f.clone(), t.clone())))
        .collect();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);

    let mut m = mgr();

    // A relation we hold on to, and the answers it must keep giving.
    let mut kept_pairs = HashSet::new();
    for _ in 0..6 {
        kept_pairs.insert(pairs[rng.below(pairs.len())].clone());
    }
    let kept = build_rel(&mut m, &kept_pairs);
    let kept_transpose = m.transpose(kept);
    let kept_complement = m.not_rel(kept);
    let expected: HashSet<Transition> = m.enumerate_relation(kept).into_iter().collect();
    let expected_t: HashSet<Transition> = m.enumerate_relation(kept_transpose).into_iter().collect();
    let expected_c = m.cardinality_relation(kept_complement);

    for round in 0..30 {
        // Churn: build throwaway relations, exercise every cached operation on
        // them, then drop them all.
        for _ in 0..8 {
            let mut junk = HashSet::new();
            for _ in 0..rng.below(6) + 1 {
                junk.insert(pairs[rng.below(pairs.len())].clone());
            }
            let j = build_rel(&mut m, &junk);
            let _ = m.and_rel(j, kept);
            let _ = m.or_rel(j, kept);
            let _ = m.transpose(j);
            let _ = m.not_rel(j);
            let one = m.one();
            let _ = m.post_image(one, j);
            let _ = m.pre_image(one, j);
        }
        m.gc(&[kept, kept_transpose, kept_complement]);

        assert_eq!(
            m.enumerate_relation(kept).into_iter().collect::<HashSet<_>>(),
            expected,
            "kept relation changed meaning after gc round {round}"
        );
        assert_eq!(
            m.enumerate_relation(kept_transpose)
                .into_iter()
                .collect::<HashSet<_>>(),
            expected_t,
            "transpose changed after gc round {round}"
        );
        assert_eq!(
            m.cardinality_relation(kept_complement),
            expected_c,
            "complement changed after gc round {round}"
        );

        // Recomputing through the cleared tables must agree with what we held.
        assert_eq!(m.transpose(kept), kept_transpose);
        assert_eq!(m.not_rel(kept), kept_complement);
    }
}

/// The lazily built full relation is memoized by level, and those entries are node
/// ids like any other: if `gc` reclaims them while the memo still names them, the
/// next complement of an empty relation silently returns whatever took the slot.
#[test]
fn test_full_relation_memo_survives_recycling() {
    let mut m = mgr();
    let omega = m.state_space_size();
    let zero = m.zero();

    let full = m.not_rel(zero);
    assert_eq!(m.cardinality_relation(full), omega * omega);

    // Drop it, so every node backing the memo is reclaimed.
    m.gc(&[]);
    assert_eq!(m.live_node_count(), 2);

    // Refill the freed slots with unrelated nodes.
    for a in 0..2 {
        for b in 0..3 {
            let _ = m.mxd_singleton(
                &[Src::Val(a), Src::Val(b), Src::Val(0)],
                &[Dst::Val(a), Dst::Val(b), Dst::Val(1)],
            );
        }
    }

    // Asking again must rebuild rather than trust a stale id.
    let again = m.not_rel(zero);
    assert_eq!(
        m.cardinality_relation(again),
        omega * omega,
        "the full relation must not be answered by a recycled slot"
    );
    assert!(
        m.enumerate_relation(again).len() as u128 == omega * omega,
        "and it must really contain every pair"
    );
}

/// The same churn, but checking a *set*-valued result, so the op-keyed set table
/// (which is filtered by `retain_live` rather than cleared) is covered too.
#[test]
fn test_set_cache_survives_recycling() {
    let states = all_states();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut m = mgr();

    let a = m.state(&[0, 1, 1]);
    let b = m.state(&[1, 2, 0]);
    let kept = m.or_set(a, b);
    let expected: HashSet<StateVec> = m.enumerate_set(kept).into_iter().collect();

    for round in 0..30 {
        for _ in 0..8 {
            let s = states[rng.below(states.len())].clone();
            let j = m.state(&s);
            let _ = m.or_set(j, kept);
            let _ = m.and_set(j, kept);
            let _ = m.not_set(j);
            let _ = m.cross(j, kept);
        }
        m.gc(&[kept]);
        assert_eq!(
            m.enumerate_set(kept).into_iter().collect::<HashSet<_>>(),
            expected,
            "kept set changed meaning after gc round {round}"
        );
    }
}
