//! Timing for the boundary pipeline, decomposed so an engine difference can be
//! told apart from an algorithmic one.
//!
//! Both phases are measured **two ways**, because the comparison against MEDDLY
//! otherwise merges two unrelated effects into one number:
//!
//! | phase | method | |
//! |---|---|---|
//! | `rel` | `union` | a singleton per (component, step), unioned one at a time — what `MDDMinsol` does |
//! | `rel` | `chain` | built directly, one node per component — what this crate does |
//! | `bnd` | `product` | `R ∩ cross(L, U)`, materialising the product — what `MDDMinsol` does |
//! | `bnd` | `fused` | a three-way recursion, never building the product |
//!
//! So `union`/`product` are the same algorithms MEDDLY is driven with, and
//! comparing *those* against MEDDLY is an engine comparison. `chain`/`fused` are
//! algorithmic wins on top, and both are available to a MEDDLY caller too — the
//! chain trivially, the fused boundary only if MEDDLY grew an operation for it.
//!
//! φ construction is not timed: MEDDLY builds it as an MTMDD and thresholds,
//! this crate builds the level sets directly, and those are different work.
//!
//! ```text
//! cargo run --release -p relib-mxd --example bench_boundary -- [out.csv]
//! ```
//!
//! With no argument it prints a readable summary; with a path it writes the CSV
//! the comparison in `results/README.md` is built from.
//!
//! # Every measurement is cold
//!
//! These operations are memoized — `cross` and `and_rel` keep compute tables on
//! the manager — so timing the *same* system repeatedly measures the cache, not
//! the work. A first draft of this harness did exactly that and reported the
//! fused boundary as three times **slower** than the product one, because the
//! product's tables were warm from the previous round while the fused
//! recursion's memo is per-call.
//!
//! So each timed run rebuilds the system, the level sets and the relation from
//! scratch, and only the phase under test is inside the clock. That is also what
//! the case studies do: they compute each boundary once.

use mxdcore::analysis::*;
use std::time::{Duration, Instant};

const T: usize = 3;
const YMIN: usize = 5;
const YMAX: usize = 20;
/// Every sum at or above this gives φ = 1, so the accumulator saturates here.
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

fn build(n: usize, states: usize) -> (System, Levels) {
    let sys = System::new(&vec![states; n]);
    let levels = sys.levels_from_fold(0usize, |a, _, v| (a + v).min(SAT), |&a| phi(a));
    (sys, levels)
}

/// Steps admitted for a component with `states` states.
fn steps(kind: &str, states: usize) -> Vec<(usize, usize)> {
    match kind {
        "dec" => (1..states).map(|v| (v, v - 1)).collect(),
        "inc" => (0..states - 1).map(|v| (v, v + 1)).collect(),
        "restart" => (0..states - 1).map(|v| (v, states - 1)).collect(),
        other => panic!("unknown step family `{other}`"),
    }
}

/// The relation built the way `MDDMinsol` builds it: one singleton per
/// (component, step), unioned in.
fn relation_by_union(sys: &System, kinds: &[&str]) -> Transitions {
    let mut acc = sys.no_transitions();
    for i in 0..sys.len() {
        for kind in kinds {
            for (from, to) in steps(kind, sys.states()[i]) {
                acc = acc.union(&sys.single(i, from, to));
            }
        }
    }
    acc
}

/// The same relation built as a chain, which is what `System::degrade` and
/// friends do.
fn relation_by_chain(sys: &System, kinds: &[&str]) -> Transitions {
    let mut acc = sys.no_transitions();
    for kind in kinds {
        let part = match *kind {
            "dec" => sys.degrade(),
            "inc" => sys.repair(),
            "restart" => sys.restart(),
            other => panic!("unknown step family `{other}`"),
        };
        acc = acc.union(&part);
    }
    acc
}

/// `R ∩ (L × U)` with the product materialised, as MEDDLY is driven.
fn boundary_by_product(lower: &StateSet, upper: &StateSet, rel: &Transitions) -> Transitions {
    lower.cross(upper).intersect(rel)
}

const FAMILIES: &[(&str, &[&str])] = &[
    ("dec_inc", &["dec", "inc"]),
    ("restart", &["restart"]),
    ("dec_restart", &["dec", "restart"]),
];

/// (components, states per component)
const GRID: &[(usize, usize)] = &[
    (8, 3),
    (12, 3),
    (16, 3),
    (22, 3),
    (40, 3),
    (60, 3),
    (100, 3),
    (8, 4),
    (16, 4),
    (30, 4),
    (60, 4),
    (8, 5),
    (16, 5),
    (30, 5),
    (60, 5),
];

/// Runs `f` `rounds` times and keeps the fastest, where `f` is responsible for
/// building its own state — the point is that nothing is memoized between runs.
fn best_of<T>(rounds: usize, mut f: impl FnMut() -> (Duration, T)) -> (Duration, T) {
    let (mut best, mut last) = f();
    for _ in 1..rounds {
        let (t, v) = f();
        if t < best {
            best = t;
        }
        last = v;
    }
    (best, last)
}

fn main() {
    let out = std::env::args().nth(1);
    let mut rows = vec!["n,states,relation,phase,method,seconds,cards".to_string()];

    // Warm up the allocator; the measured runs each build their own system.
    let _ = build(3, 3);

    for &(n, states) in GRID {
        for &(fam, kinds) in FAMILIES {
            // --- relation construction, each run on a fresh forest
            let (t_union, by_union) = best_of(3, || {
                let (sys, _) = build(n, states);
                let t = Instant::now();
                let r = relation_by_union(&sys, kinds);
                (t.elapsed(), r.count())
            });
            let (t_chain, by_chain) = best_of(3, || {
                let (sys, _) = build(n, states);
                let t = Instant::now();
                let r = relation_by_chain(&sys, kinds);
                (t.elapsed(), r.count())
            });
            assert_eq!(
                by_union, by_chain,
                "n={n} states={states} {fam}: the two constructions must agree"
            );

            // --- boundaries, each run on a fresh forest so no compute table is warm
            let bnd = |fused: bool| {
                best_of(3, || {
                    let (sys, levels) = build(n, states);
                    let rel = relation_by_chain(&sys, kinds);
                    let t = Instant::now();
                    let mut cs = Vec::new();
                    for j in levels.interior() {
                        let (lo, hi) = (levels.lower(j), levels.upper(j));
                        if fused {
                            cs.push(rel.boundary(&lo, &hi).count());
                            cs.push(rel.boundary(&hi, &lo).count());
                        } else {
                            cs.push(boundary_by_product(&lo, &hi, &rel).count());
                            cs.push(boundary_by_product(&hi, &lo, &rel).count());
                        }
                    }
                    (t.elapsed(), cs)
                })
            };
            let (t_prod, c_prod) = bnd(false);
            let (t_fused, c_fused) = bnd(true);
            assert_eq!(
                c_prod, c_fused,
                "n={n} states={states} {fam}: the two boundaries must agree"
            );

            let cards = c_fused
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(";");
            for (phase, method, secs) in [
                ("rel", "union", t_union),
                ("rel", "chain", t_chain),
                ("bnd", "product", t_prod),
                ("bnd", "fused", t_fused),
            ] {
                rows.push(format!(
                    "{n},{states},{fam},{phase},{method},{:.9},{cards}",
                    secs.as_secs_f64()
                ));
            }

            if out.is_none() {
                println!(
                    "n={n:3} st={states} {fam:<12} | rel union {t_union:>9.3?} chain {t_chain:>9.3?} \
                     | bnd product {t_prod:>9.3?} fused {t_fused:>9.3?}"
                );
            }
        }
    }

    if let Some(path) = out {
        std::fs::write(&path, rows.join("\n") + "\n").expect("cannot write the CSV");
        eprintln!("wrote {path}");
    }
}
