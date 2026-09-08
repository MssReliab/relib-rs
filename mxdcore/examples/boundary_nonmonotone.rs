//! The boundary operator on a **non-monotone** multi-state system.
//!
//! System: `distribution_system(n)` from Sedlacek, Zaitseva, Levashenko and
//! Kvassay (2021), RESS 215:107824 §4.2. `n` factories, each in one of three
//! states; production `P(x) = t · Σ xᵢ`, and
//!
//! ```text
//! φ(x) = 0  if P < ymin      (too little produced)
//!        1  if P > ymax      (warehouse overflows)
//!        2  otherwise
//! ```
//!
//! It is non-monotone precisely because of the middle case: producing *more* can
//! drop φ from 2 to 1. Minimal path/cut vectors are not defined for such a system
//! — the Rauzy recursion behind them assumes monotonicity — so the boundary
//! operator `B_j = R ∩ (L_j × U_j)` is what is left, and it never assumed
//! monotonicity in the first place.
//!
//! This mirrors `MDDMinsol/scripts/boundary_nonmonotone.jl`, which computes the
//! same quantities through MEDDLY, so the two can be compared. Cardinalities are
//! directly comparable and are the correctness check; **node counts are not**,
//! because this crate fuses a variable's source and target into one node where
//! MEDDLY interleaves two levels.
//!
//! Nothing here enumerates the state space: `|Ω| = 3^n` passes 10^10 by n = 22 and
//! is printed only for reference.
//!
//! ```text
//! cargo run --release -p relib-mxd --example boundary_nonmonotone -- [N...]
//! ```

use mxdcore::prelude::*;
use std::collections::HashMap;
use std::time::Instant;

const T: usize = 3;
const YMIN: usize = 5;
const YMAX: usize = 20;
const STATES: usize = 3;
/// The largest sum worth distinguishing: every `s` at or above it gives φ = 1.
const SAT: usize = YMAX / T + 1;

/// φ as a function of the component sum, which is all it depends on.
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

/// The set `{ x : keep(φ(Σ xᵢ)) }`, built directly as a boolean MDD.
///
/// φ depends on the sum alone, so there is no need for a value-carrying diagram:
/// the recursion carries the partial sum, saturated at `SAT` because every larger
/// sum behaves identically. That saturation is what keeps the result linear in `n`
/// rather than quadratic.
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
    let mut memo = HashMap::new();
    rec(m, n as i64 - 1, 0, keep, &mut memo)
}

/// `R = R_inc ∪ R_dec`: any single component steps one level up or down, every
/// other component invariant. Both directions, since the system is non-monotone
/// and repair matters.
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

struct Row {
    n: usize,
    phi_nodes: usize,
    rel_nodes: usize,
    rel_card: u128,
    live: usize,
    t_phi: f64,
    t_rel: f64,
    t_bnd: f64,
    levels: Vec<(usize, u128, usize, u128, usize)>,
}

fn measure(n: usize) -> Row {
    let mut m = MxdManager::new();
    for i in 0..n {
        m.defvar(&format!("x{i}"), STATES);
    }

    // The level sets. j runs over 1..m-1 with m = 3, so j is 1 and 2.
    let t0 = Instant::now();
    let mut uppers = Vec::new();
    let mut lowers = Vec::new();
    for j in 1..STATES {
        uppers.push(level_set(&mut m, n, &move |s| phi(s) >= j));
        lowers.push(level_set(&mut m, n, &move |s| phi(s) < j));
    }
    let t_phi = t0.elapsed().as_secs_f64();
    // φ itself is not built as one diagram here; its level sets are. Report the
    // size of the largest, which is the closest honest analogue.
    let phi_nodes = uppers
        .iter()
        .chain(lowers.iter())
        .map(|&e| m.node_count(e))
        .max()
        .unwrap();

    let t1 = Instant::now();
    let rel = transition_relation(&mut m, n);
    let t_rel = t1.elapsed().as_secs_f64();

    let t2 = Instant::now();
    let mut levels = Vec::new();
    for (idx, j) in (1..STATES).enumerate() {
        let (l, u) = (lowers[idx], uppers[idx]);
        let up = m.boundary(l, u, rel);
        let down = m.boundary(u, l, rel);
        levels.push((
            j,
            m.cardinality_relation(up),
            m.node_count(up),
            m.cardinality_relation(down),
            m.node_count(down),
        ));
    }
    let t_bnd = t2.elapsed().as_secs_f64();

    Row {
        n,
        phi_nodes,
        rel_nodes: m.node_count(rel),
        rel_card: m.cardinality_relation(rel),
        live: m.live_node_count(),
        t_phi,
        t_rel,
        t_bnd,
        levels,
    }
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

    // Warm up, so the first measured row is not paying for cold allocator pages.
    let _ = measure(3);

    println!("n,state_space,phi_nodes,rel_nodes,rel_card,live_nodes,j,b_card,b_nodes,bd_card,bd_nodes,t_phi_s,t_rel_s,t_bnd_s");
    for n in ns {
        let r = measure(n);
        let ss = 3u128.checked_pow(n as u32);
        let ss = ss.map(|v| v.to_string()).unwrap_or_else(|| "overflow".into());
        for (j, bc, bn, bdc, bdn) in &r.levels {
            println!(
                "{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.6},{:.6}",
                r.n, ss, r.phi_nodes, r.rel_nodes, r.rel_card, r.live, j, bc, bn, bdc, bdn,
                r.t_phi, r.t_rel, r.t_bnd
            );
        }
        eprintln!(
            "n={:2}  |Ω|={:<22} phi={:4}  rel={:5}  live={:6}  t_rel={:.6}s  t_bnd={:.6}s",
            r.n, ss, r.phi_nodes, r.rel_nodes, r.live, r.t_rel, r.t_bnd
        );
        for (j, bc, bn, bdc, _) in &r.levels {
            eprintln!("        j={j}  |B_j|={bc:<14} B_j nodes={bn:5}   |B_j^down|={bdc}");
        }
    }
}
