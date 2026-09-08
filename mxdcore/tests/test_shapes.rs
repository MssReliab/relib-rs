//! The same oracle comparisons, across varying numbers of variables and domain
//! sizes.
//!
//! Every other randomised suite in this crate is fixed at three variables with
//! domains `[2, 3, 2]`. That leaves the shape itself untested: a single variable, a
//! single-state component (where the identity and the full relation coincide, so
//! identity reduction fires on a block that is also the full one), and domains
//! larger than the `n = 2` most hand cases use.

use mxdcore::prelude::*;
use std::collections::HashSet;

const SHAPES: &[&[usize]] = &[
    &[2],
    &[3],
    &[1, 3],
    &[2, 2],
    &[4, 2],
    &[2, 3, 2],
    &[3, 1, 2],
    &[2, 2, 2, 2],
];

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

fn mgr(domains: &[usize]) -> MxdManager {
    let mut m = MxdManager::new();
    for (i, &n) in domains.iter().enumerate() {
        m.defvar(&format!("v{i}"), n);
    }
    m
}

fn all_states(domains: &[usize]) -> Vec<StateVec> {
    let mut out = vec![vec![]];
    for &n in domains {
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

#[test]
fn test_every_operation_across_shapes() {
    for (si, domains) in SHAPES.iter().enumerate() {
        let states = all_states(domains);
        let omega = states.len();
        let pairs: Vec<Transition> = states
            .iter()
            .flat_map(|f| states.iter().map(move |t| (f.clone(), t.clone())))
            .collect();
        let universe_r: HashSet<Transition> = pairs.iter().cloned().collect();
        let universe_s: HashSet<StateVec> = states.iter().cloned().collect();
        let mut rng = Rng(0x243f_6a88_85a3_08d3 ^ (si as u64 + 1));

        for round in 0..60 {
            let mut r = HashSet::new();
            for _ in 0..rng.below(6) {
                r.insert(pairs[rng.below(pairs.len())].clone());
            }
            let mut a = HashSet::new();
            for _ in 0..rng.below(4) {
                a.insert(states[rng.below(omega)].clone());
            }
            let mut b = HashSet::new();
            for _ in 0..rng.below(4) {
                b.insert(states[rng.below(omega)].clone());
            }

            let mut m = mgr(domains);
            let fr = build_rel(&mut m, &r);
            let fa = build_set(&mut m, &a);
            let fb = build_set(&mut m, &b);
            let ctx = format!("shape {domains:?} round {round}");

            assert_eq!(
                m.enumerate_relation(fr).into_iter().collect::<HashSet<_>>(),
                r,
                "{ctx}: relation round-trip"
            );
            assert_eq!(
                m.enumerate_set(fa).into_iter().collect::<HashSet<_>>(),
                a,
                "{ctx}: set round-trip"
            );

            // Boolean algebra.
            let and_s = m.and_set(fa, fb);
            let or_s = m.or_set(fa, fb);
            let not_s = m.not_set(fa);
            assert_eq!(read_set(&m, and_s), &a & &b, "{ctx}: and_set");
            assert_eq!(read_set(&m, or_s), &a | &b, "{ctx}: or_set");
            assert_eq!(read_set(&m, not_s), &universe_s - &a, "{ctx}: not_set");

            let not_r = m.not_rel(fr);
            assert_eq!(read_rel(&m, not_r), &universe_r - &r, "{ctx}: not_rel");

            // Product, converse, images, boundary.
            let x = m.cross(fa, fb);
            let expected_x: HashSet<Transition> = a
                .iter()
                .flat_map(|f| b.iter().map(move |t| (f.clone(), t.clone())))
                .collect();
            assert_eq!(read_rel(&m, x), expected_x, "{ctx}: cross");
            assert_eq!(
                m.cardinality_relation(x),
                (a.len() * b.len()) as u128,
                "{ctx}: |cross|"
            );

            let t = m.transpose(fr);
            let expected_t: HashSet<Transition> =
                r.iter().map(|(f, s)| (s.clone(), f.clone())).collect();
            assert_eq!(read_rel(&m, t), expected_t, "{ctx}: transpose");

            let post = m.post_image(fa, fr);
            let pre = m.pre_image(fa, fr);
            let expected_post: HashSet<StateVec> = r
                .iter()
                .filter(|(f, _)| a.contains(f))
                .map(|(_, s)| s.clone())
                .collect();
            let expected_pre: HashSet<StateVec> = r
                .iter()
                .filter(|(_, s)| a.contains(s))
                .map(|(f, _)| f.clone())
                .collect();
            assert_eq!(read_set(&m, post), expected_post, "{ctx}: post_image");
            assert_eq!(read_set(&m, pre), expected_pre, "{ctx}: pre_image");
            assert_eq!(m.post_image(fa, t), pre, "{ctx}: post ∘ transpose = pre");

            let bnd = m.boundary(fa, fb, fr);
            let expected_b: HashSet<Transition> = r
                .iter()
                .filter(|(f, s)| a.contains(f) && b.contains(s))
                .cloned()
                .collect();
            assert_eq!(read_rel(&m, bnd), expected_b, "{ctx}: boundary");
        }
    }
}

/// A single-state component makes the identity and the full relation the same
/// thing, which is the one place identity reduction is allowed to collapse a full
/// block. Worth pinning per shape rather than only for the all-ones domain.
#[test]
fn test_unit_components_collapse_correctly() {
    for domains in SHAPES.iter().filter(|d| d.contains(&1)) {
        let mut m = mgr(domains);
        let k = domains.len();
        let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);
        let all = m.one();
        let full = m.cross(all, all);
        let omega = m.state_space_size();

        assert_eq!(m.cardinality_relation(id), omega, "{domains:?}: |identity|");
        assert_eq!(
            m.cardinality_relation(full),
            omega * omega,
            "{domains:?}: |full|"
        );
        // They coincide only when every component has a single state.
        if domains.iter().all(|&n| n == 1) {
            assert_eq!(id, full, "{domains:?}");
        } else {
            assert_ne!(id, full, "{domains:?}");
        }
    }
}

fn read_set(m: &MxdManager, node: NodeId) -> HashSet<StateVec> {
    m.enumerate_set(node).into_iter().collect()
}

fn read_rel(m: &MxdManager, node: NodeId) -> HashSet<Transition> {
    m.enumerate_relation(node).into_iter().collect()
}

/// Cardinalities are `u128` because the multi-state case studies reach `|Ω| = 3^22`,
/// and a relation lives in `Ω × Ω`. `3^44` overflows `u64` by a factor of ~50, so a
/// 64-bit count would silently wrap on exactly the systems this crate is for.
#[test]
fn test_cardinality_beyond_64_bits() {
    let mut m = mgr(&vec![3usize; 22]);
    let omega = m.state_space_size();
    assert_eq!(omega, 3u128.pow(22));
    assert!(omega * omega > u64::MAX as u128, "the case must actually overflow u64");

    let all = m.one();
    let full = m.cross(all, all);
    assert_eq!(
        m.cardinality_relation(full),
        3u128.pow(44),
        "|Ω × Ω| must be exact, not wrapped"
    );

    // The identity over the same space, for contrast.
    let id = m.mxd_singleton(&vec![Src::Any; 22], &vec![Dst::Same; 22]);
    assert_eq!(m.cardinality_relation(id), omega);
}
