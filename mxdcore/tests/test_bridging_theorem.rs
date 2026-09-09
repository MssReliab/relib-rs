//! The bridging theorem, checked numerically.
//!
//! `idea.md` §3 of the ORSJ manuscript states, as 要証明:
//!
//! ```text
//! MCV(j) = { x ∈ L_j : every increment from x lies in B = R_inc ∩ (L_j × U_j) }
//! ```
//!
//! It is what makes the boundary operator a *generalisation* rather than a
//! different thing: on a monotone system the boundary reproduces the classical
//! minimal cut vectors exactly, so the values it produces on a non-monotone one —
//! where minimal vectors are undefined — stand in for them rather than beside
//! them.
//!
//! # The point is ∀, not ∃
//!
//! The note in `idea.md` is the crux and this file exists mostly to pin it. The
//! easy projection `sources(B)` is the **existential** set — states from which
//! *some* increment crosses. `MCV(j)` is the **universal** one — states from which
//! *every* increment crosses, which is the same as being maximal in `L_j` once
//! `L_j` is downward closed. So `sources(B) ⊇ MCV(j)`, and the inclusion is
//! generally strict; `test_existential_is_strictly_larger` exhibits that.
//!
//! # How each side is computed
//!
//! Independently, and in different crates, from the same φ:
//!
//! - **left** — `relib-mss`: `mincut(φ).extract_level(j-1)`, whose documented
//!   contract is `maximal{x : φ(x) ≤ j-1}` = the maximal elements of `L_j`.
//! - **right** — this crate: `L_j \ sources(R_inc ∩ (L_j × L_j))`. A state fails
//!   the universal condition exactly when it has *some* increment that stays
//!   inside `L_j`, so removing the sources of the non-crossing increments leaves
//!   the states all of whose increments cross. States with no admissible increment
//!   at all survive, which is correct: the condition is vacuously true and such a
//!   state is maximal.
//!
//! This does not prove the theorem. It checks it on systems small enough to also
//! verify by brute force, which is what the third test does.

use mss::prelude::*;
use mxdcore::analysis::*;
use std::collections::HashSet;

/// A monotone system: component state counts plus φ.
struct Case {
    name: &'static str,
    states: Vec<usize>,
    phi: Box<dyn Fn(&[usize]) -> usize>,
}

/// φ(x) = max over y ≤ x of a random table — the standard way to force
/// monotonicity, and what `MDDMinsol.random_monotone_system` does.
fn monotonise(states: &[usize], seed: u64) -> Box<dyn Fn(&[usize]) -> usize> {
    let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let m = *states.iter().max().unwrap();
    let mut raw: Vec<usize> = Vec::new();
    let total: usize = states.iter().product();
    for _ in 0..total {
        raw.push((next() % m as u64) as usize);
    }
    let dims = states.to_vec();
    let index = move |x: &[usize], dims: &[usize]| {
        let mut i = 0;
        for (k, &v) in x.iter().enumerate() {
            i = i * dims[k] + v;
        }
        i
    };
    Box::new(move |x: &[usize]| {
        // max over the down-set of x
        let mut best = 0usize;
        let mut y = vec![0usize; x.len()];
        loop {
            best = best.max(raw[index(&y, &dims)]);
            let mut i = 0;
            loop {
                if i == x.len() {
                    return best;
                }
                y[i] += 1;
                if y[i] <= x[i] {
                    break;
                }
                y[i] = 0;
                i += 1;
            }
        }
    })
}

fn cases() -> Vec<Case> {
    let mut v: Vec<Case> = vec![
        Case {
            name: "series 3x3",
            states: vec![3, 3, 3],
            phi: Box::new(|x| *x.iter().min().unwrap()),
        },
        Case {
            name: "parallel 3x3",
            states: vec![3, 3, 3],
            phi: Box::new(|x| *x.iter().max().unwrap()),
        },
        Case {
            name: "mixed max(min(a,b),c)",
            states: vec![3, 3, 3],
            phi: Box::new(|x| x[0].min(x[1]).max(x[2])),
        },
        Case {
            name: "sum threshold",
            states: vec![3, 3, 2, 2],
            phi: Box::new(|x| {
                let s: usize = x.iter().sum();
                if s < 2 {
                    0
                } else if s < 5 {
                    1
                } else {
                    2
                }
            }),
        },
    ];
    for (i, states) in [vec![3, 3, 3], vec![2, 3, 2], vec![2, 2, 2, 2]]
        .into_iter()
        .enumerate()
    {
        for seed in 1..=3u64 {
            v.push(Case {
                name: Box::leak(format!("monotonised random {i}/{seed}").into_boxed_str()),
                phi: monotonise(&states, seed + 100 * i as u64),
                states: states.clone(),
            });
        }
    }
    v
}

fn all_states(states: &[usize]) -> Vec<Vec<usize>> {
    let mut out = vec![vec![]];
    for &n in states {
        let mut next = Vec::new();
        for p in &out {
            for a in 0..n {
                let mut v = p.clone();
                v.push(a);
                next.push(v);
            }
        }
        out = next;
    }
    out
}

/// Build φ in `relib-mss` as an MTMDD, one component per level.
fn phi_in_mss(
    states: &[usize],
    phi: &dyn Fn(&[usize]) -> usize,
) -> (MssMgr<i32>, MddNode<i32>, Vec<String>) {
    fn rec(
        mgr: &MssMgr<i32>,
        headers: &[HeaderId],
        states: &[usize],
        phi: &dyn Fn(&[usize]) -> usize,
        level: i64,
        assign: &mut Vec<usize>,
    ) -> MddNode<i32> {
        if level < 0 {
            return mgr.value(phi(assign) as i32);
        }
        let l = level as usize;
        let children: Vec<MddNode<i32>> = (0..states[l])
            .map(|v| {
                assign[l] = v;
                rec(mgr, headers, states, phi, level - 1, assign)
            })
            .collect();
        mgr.create_node(headers[l], &children)
    }

    let mut mgr: MssMgr<i32> = MssMgr::new();
    let names: Vec<String> = (0..states.len()).map(|i| format!("x{i}")).collect();
    let headers: Vec<HeaderId> = names
        .iter()
        .zip(states)
        .map(|(n, &d)| mgr.defvar(n, d).get_header().unwrap())
        .collect();
    let mut assign = vec![0usize; states.len()];
    let node = rec(
        &mgr,
        &headers,
        states,
        phi,
        states.len() as i64 - 1,
        &mut assign,
    );
    (mgr, node, names)
}

/// `MCV(j)` from the minimal-cut side: the maximal elements of `L_j`.
fn mcv_from_mss(
    mgr: &MssMgr<i32>,
    phi: &MddNode<i32>,
    names: &[String],
    j: usize,
) -> HashSet<Vec<usize>> {
    let family = mgr.mincut(phi).expect("these systems are monotone");
    family
        .extract_level(j as i32 - 1)
        .iter()
        .map(|d| names.iter().map(|n| d[n]).collect())
        .collect()
}

/// The universal set: `L_j` minus the sources of increments that stay inside it.
fn universal_from_mxd(sys: &System, levels: &Levels, j: usize) -> HashSet<Vec<usize>> {
    let lower = levels.lower(j);
    let inc = sys.repair(); // R_inc: one component steps up one state
    let stays_inside = inc.boundary(&lower, &lower);
    let has_a_non_crossing_step = stays_inside.sources();
    lower
        .difference(&has_a_non_crossing_step)
        .vectors()
        .into_iter()
        .collect()
}

/// The existential set: states from which *some* increment crosses.
fn existential_from_mxd(sys: &System, levels: &Levels, j: usize) -> HashSet<Vec<usize>> {
    let inc = sys.repair();
    inc.boundary_up(levels, j)
        .sources()
        .vectors()
        .into_iter()
        .collect()
}

#[test]
fn test_bridging_theorem_holds() {
    let mut checked = 0usize;
    for case in cases() {
        let (mgr, phi_node, names) = phi_in_mss(&case.states, &*case.phi);
        assert!(
            mgr.mincut(&phi_node).is_some(),
            "{}: the case must be monotone for MCV to exist",
            case.name
        );

        let sys = System::new(&case.states);
        let levels = sys.levels_from_states(|x| (case.phi)(x));

        for j in levels.interior() {
            let left = mcv_from_mss(&mgr, &phi_node, &names, j);
            let right = universal_from_mxd(&sys, &levels, j);
            assert_eq!(
                left, right,
                "{} level {j}: MCV(j) and the universal set disagree",
                case.name
            );
            checked += 1;
        }
    }
    assert!(checked >= 10, "expected a decent number of levels, got {checked}");
}

/// The distinction the manuscript's note is about: the easy projection is the
/// existential set, it strictly contains `MCV(j)`, and using it in place of `MCV`
/// would be wrong.
#[test]
fn test_existential_is_strictly_larger() {
    let mut strict = 0usize;
    let mut total = 0usize;
    for case in cases() {
        let sys = System::new(&case.states);
        let levels = sys.levels_from_states(|x| (case.phi)(x));
        for j in levels.interior() {
            let some = existential_from_mxd(&sys, &levels, j);
            let all = universal_from_mxd(&sys, &levels, j);
            assert!(
                all.is_subset(&some),
                "{} level {j}: every state whose increments all cross has some that crosses",
                case.name
            );
            total += 1;
            if all.len() < some.len() {
                strict += 1;
            }
        }
    }
    assert!(
        strict > 0,
        "the inclusion was never strict in {total} levels, so this test proves nothing"
    );
    eprintln!("  ∃ ⊋ ∀ in {strict} of {total} levels");
}

/// Both sides against brute force over the explicit state space, so the agreement
/// above cannot be two diagram engines sharing a mistake.
#[test]
fn test_both_sides_match_brute_force() {
    for case in cases() {
        let states = all_states(&case.states);
        let phi = |x: &Vec<usize>| (case.phi)(x);

        let sys = System::new(&case.states);
        let levels = sys.levels_from_states(|x| (case.phi)(x));
        let (mgr, phi_node, names) = phi_in_mss(&case.states, &*case.phi);

        for j in levels.interior() {
            // maximal elements of L_j = {x : φ(x) < j}
            let lower: Vec<&Vec<usize>> = states.iter().filter(|x| phi(x) < j).collect();
            let expected: HashSet<Vec<usize>> = lower
                .iter()
                .filter(|x| {
                    !lower
                        .iter()
                        .any(|y| y != *x && y.iter().zip(x.iter()).all(|(a, b)| a >= b))
                })
                .map(|x| (*x).clone())
                .collect();

            assert_eq!(
                mcv_from_mss(&mgr, &phi_node, &names, j),
                expected,
                "{} level {j}: mss MCV vs brute force",
                case.name
            );
            assert_eq!(
                universal_from_mxd(&sys, &levels, j),
                expected,
                "{} level {j}: the universal set vs brute force",
                case.name
            );
        }
    }
}

/// A worked instance small enough to check by hand, so the ∃/∀ gap can be quoted
/// rather than asserted.
///
/// Two components of three states, φ = min (a series system), at level 1:
///
/// ```text
/// L_1 = {x : min(x) = 0} = {(0,0), (0,1), (0,2), (1,0), (2,0)}
///
/// ∃  sources(B)  = {(0,1), (0,2), (1,0), (2,0)}
/// ∀  MCV(1)      = {        (0,2),         (2,0)}
/// ```
///
/// `(0,1)` is in the existential set because raising the first component reaches
/// `(1,1)`, where `min = 1`, so *some* increment crosses. It is not in the
/// universal one because raising the *second* reaches `(0,2)`, where `min` is
/// still 0 — and it is not maximal in `L_1` for exactly that reason. `(0,0)` is in
/// neither: no single increment gets `min` above 0.
#[test]
fn test_worked_example_of_the_gap() {
    let states = [3usize, 3];
    let phi = |x: &[usize]| *x.iter().min().unwrap();

    let sys = System::new(&states);
    let levels = sys.levels_from_states(phi);

    let sorted = |mut v: Vec<Vec<usize>>| {
        v.sort();
        v
    };
    let lower = sorted(levels.lower(1).vectors());
    assert_eq!(
        lower,
        vec![vec![0, 0], vec![0, 1], vec![0, 2], vec![1, 0], vec![2, 0]]
    );

    let exists = sorted(existential_from_mxd(&sys, &levels, 1).into_iter().collect());
    let forall = sorted(universal_from_mxd(&sys, &levels, 1).into_iter().collect());
    assert_eq!(
        exists,
        vec![vec![0, 1], vec![0, 2], vec![1, 0], vec![2, 0]],
        "some increment crosses"
    );
    assert_eq!(
        forall,
        vec![vec![0, 2], vec![2, 0]],
        "every increment crosses -- the maximal elements of L_1"
    );

    // And the universal side really is what `relib-mss` calls the minimal cut
    // vectors, computed on the other engine.
    let (mgr, node, names) = phi_in_mss(&states, &phi);
    assert_eq!(
        sorted(mcv_from_mss(&mgr, &node, &names, 1).into_iter().collect()),
        forall
    );
}
