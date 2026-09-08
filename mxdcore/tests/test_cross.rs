//! `cross`, `transpose` and the boundary operator, against explicit enumeration.
//!
//! `cross` is the first operation whose two operands play *different* roles, so it
//! is the first that can be wrong by transposition while still looking plausible.
//! `transpose` is the cheapest instrument for catching that, which is why it lands
//! here rather than being deferred to the image operations that will need it most.

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

fn read_rel(m: &MxdManager, node: NodeId) -> HashSet<Transition> {
    m.enumerate_relation(node).into_iter().collect()
}

fn product(a: &HashSet<StateVec>, b: &HashSet<StateVec>) -> HashSet<Transition> {
    a.iter()
        .flat_map(|f| b.iter().map(move |t| (f.clone(), t.clone())))
        .collect()
}

fn swapped(r: &HashSet<Transition>) -> HashSet<Transition> {
    r.iter().map(|(f, t)| (t.clone(), f.clone())).collect()
}

#[test]
fn test_cross_matches_oracle() {
    let states = all_states();
    let mut rng = Rng(0x1234_5678_9abc_def0);

    for _ in 0..200 {
        let pick = |rng: &mut Rng| -> HashSet<StateVec> {
            let count = rng.below(5);
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
        let x = m.cross(fa, fb);

        assert_eq!(read_rel(&m, x), product(&a, &b));
        assert_eq!(
            m.cardinality_relation(x),
            (a.len() * b.len()) as u64,
            "|cross(A,B)| = |A|·|B|"
        );
    }
}

#[test]
fn test_cross_with_empty_is_empty() {
    let mut m = mgr();
    let zero = m.zero();
    let a = m.state(&[1, 2, 0]);
    let l = m.cross(a, zero);
    let r = m.cross(zero, a);
    assert_eq!(l, zero);
    assert_eq!(r, zero);
}

#[test]
fn test_cross_of_full_sets_is_the_full_relation() {
    let mut m = mgr();
    let all = m.one();
    let x = m.cross(all, all);
    let omega = m.state_space_size();
    assert_eq!(m.cardinality_relation(x), omega * omega);

    // And it is not the One terminal, which as a relation is the identity.
    assert_ne!(x, m.one());
    let id = m.mxd_singleton(&vec![Src::Any; DOMAINS.len()], &vec![Dst::Same; DOMAINS.len()]);
    assert_ne!(x, id);
}

/// The orientation check. `cross` is not commutative, and swapping its operands is
/// exactly transposition — so this fails if either operation reads `a * n + b` the
/// wrong way round, including the case where *both* do and the bug cancels in
/// `cross` alone.
#[test]
fn test_cross_transpose_duality() {
    let mut m = mgr();
    let a = {
        let mut s = HashSet::new();
        s.insert(vec![0, 0, 0]);
        s.insert(vec![1, 2, 0]);
        build_set(&mut m, &s)
    };
    let b = {
        let mut s = HashSet::new();
        s.insert(vec![1, 1, 1]);
        build_set(&mut m, &s)
    };

    let ab = m.cross(a, b);
    let ba = m.cross(b, a);
    assert_ne!(ab, ba, "cross is not commutative");

    let t = m.transpose(ab);
    assert_eq!(t, ba, "transpose(A × B) = B × A");
    assert_eq!(read_rel(&m, t), swapped(&read_rel(&m, ab)));
}

#[test]
fn test_transpose_matches_oracle() {
    let states = all_states();
    let pairs: Vec<Transition> = states
        .iter()
        .flat_map(|f| states.iter().map(move |t| (f.clone(), t.clone())))
        .collect();
    let mut rng = Rng(0x0bad_beef_1234_5678);

    for _ in 0..200 {
        let count = rng.below(9);
        let mut r = HashSet::new();
        for _ in 0..count {
            r.insert(pairs[rng.below(pairs.len())].clone());
        }

        let mut m = mgr();
        let fr = build_rel(&mut m, &r);
        let t = m.transpose(fr);

        assert_eq!(read_rel(&m, t), swapped(&r));
        assert_eq!(m.cardinality_relation(t), r.len() as u64);

        let back = m.transpose(t);
        assert_eq!(back, fr, "transpose is an involution");
    }
}

#[test]
fn test_transpose_fixes_symmetric_relations() {
    let mut m = mgr();
    let k = DOMAINS.len();

    // The identity and the full relation are both symmetric.
    let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);
    assert_eq!(m.transpose(id), id);

    let all = m.one();
    let full = m.cross(all, all);
    assert_eq!(m.transpose(full), full);

    // The One terminal read as a relation is the identity, so it too is fixed --
    // and this reaches transpose's skipped-level path.
    let one = m.one();
    let t_one = m.transpose(one);
    assert_eq!(read_rel(&m, t_one), read_rel(&m, id));
}

/// `B = R ∩ (L × U)` on a case small enough to check by hand.
///
/// Two variables' worth of structure: `R` increments `v1` by one and leaves the
/// others alone; `L` is everything with `v1 = 0` and `U` everything with `v1 = 1`.
/// The boundary is then exactly the `0 -> 1` transitions, one per combination of
/// the untouched variables.
#[test]
fn test_boundary_operator_hand_checked() {
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

    // v0 has 2 states and v2 has 2 states, both invariant across the step.
    let expected: HashSet<Transition> = (0..2)
        .flat_map(|a| (0..2).map(move |c| (vec![a, 0, c], vec![a, 1, c])))
        .collect();
    assert_eq!(read_rel(&m, b), expected);
    assert_eq!(m.cardinality_relation(b), 4);

    // The reverse boundary is empty: R only ever increments.
    let back = m.boundary(upper, lower, r);
    assert_eq!(back, m.zero(), "an increment-only relation has no 1 -> 0 step");
}

#[test]
fn test_boundary_matches_oracle() {
    let states = all_states();
    let pairs: Vec<Transition> = states
        .iter()
        .flat_map(|f| states.iter().map(move |t| (f.clone(), t.clone())))
        .collect();
    let mut rng = Rng(0xdead_c0de_face_0001);

    for _ in 0..200 {
        let mut r = HashSet::new();
        for _ in 0..rng.below(9) {
            r.insert(pairs[rng.below(pairs.len())].clone());
        }
        let mut l = HashSet::new();
        for _ in 0..rng.below(5) {
            l.insert(states[rng.below(states.len())].clone());
        }
        let mut u = HashSet::new();
        for _ in 0..rng.below(5) {
            u.insert(states[rng.below(states.len())].clone());
        }

        let mut m = mgr();
        let fr = build_rel(&mut m, &r);
        let fl = build_set(&mut m, &l);
        let fu = build_set(&mut m, &u);
        let b = m.boundary(fl, fu, fr);

        let expected: HashSet<Transition> = r
            .iter()
            .filter(|(f, t)| l.contains(f) && u.contains(t))
            .cloned()
            .collect();
        assert_eq!(read_rel(&m, b), expected);
    }
}
