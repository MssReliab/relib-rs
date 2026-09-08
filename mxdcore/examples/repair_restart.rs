//! Boundary transitions under **repair and restart** on a non-monotone system.
//!
//! Same system as `boundary_nonmonotone.rs` — `distribution_system(n)` from
//! Sedlacek, Zaitseva, Levashenko and Kvassay (2021), RESS 215:107824 §4.2 —
//! but over four transition relations rather than one:
//!
//! | | |
//! |---|---|
//! | `dec` | `x_i → x_i − 1`, one component degrades a step |
//! | `inc` | `x_i → x_i + 1`, one component is repaired a step |
//! | `restart` | `x_i → top`, one component is replaced outright |
//! | `loop` | `dec ∪ restart`, the operational cycle: degrade gradually, restore to new |
//!
//! and both crossing directions per level:
//!
//! ```text
//! up_j   = R ∩ (L_j × U_j)    entering {φ ≥ j}
//! down_j = R ∩ (U_j × L_j)    leaving it
//! ```
//!
//! # What the numbers show
//!
//! In a monotone system these are trivially one-sided: degrading can only ever
//! cross downward, repairing only upward. Here they are not. Overproduction makes
//! φ fall from 2 to 1, so **degrading a component can move the system into the
//! better level set**, and **repairing one can move it out** — and restarting a
//! component outright is the strongest version of that, since it overshoots
//! hardest.
//!
//! Those wrong-direction crossings are the quantity the boundary operator exists
//! for. Minimal path and cut vectors cannot express them at all: they are not
//! defined for a non-monotone structure function, and `relib-mss`'s `minpath`
//! correctly refuses such a φ rather than returning something meaningless.
//!
//! Cross-checked against MEDDLY for `n = 2..14`; see `results/README.md`.
//!
//! ```text
//! cargo run --release -p relib-mxd --example repair_restart -- [N...]
//! ```

use mxdcore::prelude::*;
use std::collections::HashMap;
use std::time::Instant;

const T: usize = 3;
const YMIN: usize = 5;
const YMAX: usize = 20;
const STATES: usize = 3;
const SAT: usize = YMAX / T + 1;

// These helpers are duplicated from `boundary_nonmonotone.rs` rather than shared.
// That example mirrors the Julia script it is compared against and its committed
// results depend on it staying as it is; examples cannot share a module without
// putting case-study code in the library, which is worse than forty duplicated
// lines.

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

/// A relation built from per-component single steps: for each component and each
/// admissible `from`, the pair `(from, to)` with every other component invariant.
fn relation(m: &mut MxdManager, n: usize, steps: &dyn Fn(usize) -> Vec<(usize, usize)>) -> NodeId {
    let mut rel = m.zero();
    for i in 0..n {
        for (from, to) in steps(i) {
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

fn families(m: &mut MxdManager, n: usize) -> Vec<(&'static str, NodeId)> {
    let dec = relation(m, n, &|_| (1..STATES).map(|v| (v, v - 1)).collect());
    let inc = relation(m, n, &|_| (0..STATES - 1).map(|v| (v, v + 1)).collect());
    // Restart: any state below the top jumps straight to it.
    let restart = relation(m, n, &|_| (0..STATES - 1).map(|v| (v, STATES - 1)).collect());
    let looped = m.or_rel(dec, restart);
    vec![
        ("dec", dec),
        ("inc", inc),
        ("restart", restart),
        ("loop", looped),
    ]
}

fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("N must be a positive integer"))
        .collect();
    let ns: Vec<usize> = if args.is_empty() {
        (2..=22).collect()
    } else {
        args
    };

    // Warm up so the first row is not paying for cold allocator pages.
    {
        let mut m = MxdManager::new();
        for i in 0..3 {
            m.defvar(&format!("x{i}"), STATES);
        }
        let _ = families(&mut m, 3);
    }

    println!("n,relation,j,up_card,down_card,up_nodes,down_nodes,rel_card,rel_nodes,t_bnd_s");
    for n in ns {
        let mut m = MxdManager::new();
        for i in 0..n {
            m.defvar(&format!("x{i}"), STATES);
        }
        let fams = families(&mut m, n);
        let levels: Vec<(usize, NodeId, NodeId)> = (1..STATES)
            .map(|j| {
                let upper = level_set(&mut m, n, &move |s| phi(s) >= j);
                let lower = level_set(&mut m, n, &move |s| phi(s) < j);
                (j, lower, upper)
            })
            .collect();

        for (name, rel) in &fams {
            let rel_card = m.cardinality_relation(*rel);
            let rel_nodes = m.node_count(*rel);
            for &(j, lower, upper) in &levels {
                let t = Instant::now();
                let up = m.boundary(lower, upper, *rel);
                let down = m.boundary(upper, lower, *rel);
                let secs = t.elapsed().as_secs_f64();
                println!(
                    "{},{},{},{},{},{},{},{},{},{:.6}",
                    n,
                    name,
                    j,
                    m.cardinality_relation(up),
                    m.cardinality_relation(down),
                    m.node_count(up),
                    m.node_count(down),
                    rel_card,
                    rel_nodes,
                    secs
                );
                if name == &"dec" && m.cardinality_relation(up) > 0 {
                    eprintln!(
                        "n={n:2} j={j}: degrading crosses UP {} times -- only possible \
                         because φ is non-monotone",
                        m.cardinality_relation(up)
                    );
                }
            }
        }
    }
}
