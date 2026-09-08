//! `post_image` / `pre_image`.
//!
//! The hazard here is the index orientation: `post` quantifies away the source and
//! indexes the result by the target, `pre` does the opposite, and swapping them
//! yields a plausible wrong answer that symmetric test data cannot see. So the
//! relations used below are deliberately asymmetric, and the MEDDLY contract
//! `post(S, Rᵀ) == pre(S, R)` is checked on every random case rather than once.

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

fn oracle_post(s: &HashSet<StateVec>, r: &HashSet<Transition>) -> HashSet<StateVec> {
    r.iter()
        .filter(|(f, _)| s.contains(f))
        .map(|(_, t)| t.clone())
        .collect()
}

fn oracle_pre(s: &HashSet<StateVec>, r: &HashSet<Transition>) -> HashSet<StateVec> {
    r.iter()
        .filter(|(_, t)| s.contains(t))
        .map(|(f, _)| f.clone())
        .collect()
}

#[test]
fn test_images_match_oracle() {
    let states = all_states();
    let pairs: Vec<Transition> = states
        .iter()
        .flat_map(|f| states.iter().map(move |t| (f.clone(), t.clone())))
        .collect();
    let mut rng = Rng(0xabcd_ef01_2345_6789);

    for _ in 0..300 {
        let mut r = HashSet::new();
        for _ in 0..rng.below(9) {
            r.insert(pairs[rng.below(pairs.len())].clone());
        }
        let mut s = HashSet::new();
        for _ in 0..rng.below(5) {
            s.insert(states[rng.below(states.len())].clone());
        }

        let mut m = mgr();
        let fr = build_rel(&mut m, &r);
        let fs = build_set(&mut m, &s);

        let post = m.post_image(fs, fr);
        let pre = m.pre_image(fs, fr);

        assert_eq!(read_set(&m, post), oracle_post(&s, &r));
        assert_eq!(read_set(&m, pre), oracle_pre(&s, &r));

        // post(S, Rᵀ) == pre(S, R), on every case rather than once.
        let rt = m.transpose(fr);
        let post_t = m.post_image(fs, rt);
        assert_eq!(post_t, pre, "post ∘ transpose = pre");
        let pre_t = m.pre_image(fs, rt);
        assert_eq!(pre_t, post);
    }
}

/// The MEDDLY contracts on a cartesian-product relation.
#[test]
fn test_image_of_cross() {
    let mut m = mgr();
    let a: HashSet<StateVec> = [vec![0, 0, 0], vec![1, 2, 0]].into_iter().collect();
    let b: HashSet<StateVec> = [vec![1, 1, 1], vec![0, 2, 1]].into_iter().collect();

    let fa = build_set(&mut m, &a);
    let fb = build_set(&mut m, &b);
    let ab = m.cross(fa, fb);

    // Everything in A maps onto all of B, and back.
    let fwd = m.post_image(fa, ab);
    assert_eq!(read_set(&m, fwd), b, "post_image(A, A × B) = B");
    let back = m.pre_image(fb, ab);
    assert_eq!(read_set(&m, back), a, "pre_image(B, A × B) = A");

    // Nothing in B is a source of A × B (A and B are disjoint here).
    let none = m.post_image(fb, ab);
    assert_eq!(none, m.zero(), "post_image(B, A × B) is empty");
}

/// The identity moves nothing, so it images every set onto itself.
#[test]
fn test_image_under_identity() {
    let mut m = mgr();
    let k = DOMAINS.len();
    let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);
    let s: HashSet<StateVec> = [vec![0, 1, 1], vec![1, 2, 0]].into_iter().collect();
    let fs = build_set(&mut m, &s);

    let post = m.post_image(fs, id);
    let pre = m.pre_image(fs, id);
    assert_eq!(post, fs);
    assert_eq!(pre, fs);

    // And through the One terminal, which reaches the skipped-level path.
    let one = m.one();
    let post_one = m.post_image(fs, one);
    assert_eq!(post_one, fs);
}

/// An asymmetric, hand-checked case: `v1` increments and the others stay put.
/// Reading it forwards and backwards must give different answers.
#[test]
fn test_image_direction_hand_checked() {
    let mut m = mgr();
    let step = m.mxd_singleton(
        &[Src::Any, Src::Val(0), Src::Any],
        &[Dst::Same, Dst::Val(1), Dst::Same],
    );

    let at0 = m.set_minterm(&[Src::Any, Src::Val(0), Src::Any]);
    let at1 = m.set_minterm(&[Src::Any, Src::Val(1), Src::Any]);
    let at2 = m.set_minterm(&[Src::Any, Src::Val(2), Src::Any]);

    // Forwards from v1=0 lands on v1=1; backwards from v1=1 lands on v1=0.
    assert_eq!(m.post_image(at0, step), at1);
    assert_eq!(m.pre_image(at1, step), at0);

    // And the directions are genuinely different: nothing steps into v1=0, and
    // nothing steps out of v1=1 under this relation.
    let into_0 = m.pre_image(at0, step);
    assert_eq!(into_0, m.zero());
    let out_of_1 = m.post_image(at1, step);
    assert_eq!(out_of_1, m.zero());
    let out_of_2 = m.post_image(at2, step);
    assert_eq!(out_of_2, m.zero());
}

/// The image of the boundary is what the multi-state analysis actually reads off:
/// `pre_image(U, B)` is the set of states that can cross from `L` into `U`.
#[test]
fn test_boundary_projection() {
    let mut m = mgr();
    let step0 = m.mxd_singleton(
        &[Src::Any, Src::Val(0), Src::Any],
        &[Dst::Same, Dst::Val(1), Dst::Same],
    );
    let step1 = m.mxd_singleton(
        &[Src::Any, Src::Val(1), Src::Any],
        &[Dst::Same, Dst::Val(2), Dst::Same],
    );
    let r = m.or_rel(step0, step1);

    let lower = m.set_minterm(&[Src::Any, Src::Val(0), Src::Any]);
    let upper = m.set_minterm(&[Src::Any, Src::Val(1), Src::Any]);
    let b = m.boundary(lower, upper, r);

    // Sources of the boundary are exactly L, targets exactly U.
    let sources = m.pre_image(upper, b);
    assert_eq!(read_set(&m, sources), read_set(&m, lower));
    let targets = m.post_image(lower, b);
    assert_eq!(read_set(&m, targets), read_set(&m, upper));
}
