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
//! directly comparable and are the correctness check; **relation node counts are
//! not**, because this crate fuses a variable's source and target into one node
//! where MEDDLY interleaves two levels.
//!
//! Nothing here enumerates the state space: `|Ω| = 3^n` passes 10^10 by n = 22 and
//! is printed only for reference.
//!
//! ```text
//! cargo run --release -p relib-mxd --example boundary_nonmonotone -- [N...]
//! ```

use mxdcore::analysis::*;
use std::time::Instant;

const T: usize = 3;
const YMIN: usize = 5;
const YMAX: usize = 20;
/// The largest component sum worth distinguishing: everything at or above it
/// gives φ = 1, so the accumulator saturates here and the diagram stays linear
/// in `n` rather than growing with the sum's range.
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

fn distribution_system(n: usize) -> (System, Levels) {
    let mut sys = System::new(&vec![3; n]);
    let levels = sys.levels_from_fold(0usize, |acc, _i, v| (acc + v).min(SAT), |&acc| phi(acc));
    (sys, levels)
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
    let _ = distribution_system(3);

    println!("n,state_space,phi_nodes,rel_nodes,rel_card,live_nodes,j,b_card,b_nodes,bd_card,bd_nodes,t_phi_s,t_rel_s,t_bnd_s");
    for n in ns {
        let t0 = Instant::now();
        let (mut sys, levels) = distribution_system(n);
        let t_phi = t0.elapsed().as_secs_f64();

        // φ itself is not built as one diagram; its level sets are. Report the
        // largest, which is the closest honest analogue.
        let phi_nodes = levels
            .interior()
            .flat_map(|j| [levels.upper(j), levels.lower(j)])
            .map(|s| sys.node_count_states(s))
            .max()
            .unwrap();

        let t1 = Instant::now();
        let dec = sys.degrade();
        let inc = sys.repair();
        let rel = sys.union(dec, inc);
        let t_rel = t1.elapsed().as_secs_f64();

        let t2 = Instant::now();
        let rows: Vec<(usize, u128, usize, u128, usize)> = levels
            .interior()
            .map(|j| {
                let up = sys.boundary_up(&levels, j, rel);
                let down = sys.boundary_down(&levels, j, rel);
                (
                    j,
                    sys.count(up),
                    sys.node_count(up),
                    sys.count(down),
                    sys.node_count(down),
                )
            })
            .collect();
        let t_bnd = t2.elapsed().as_secs_f64();

        let ss = 3u128
            .checked_pow(n as u32)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "overflow".into());
        let rel_card = sys.count(rel);
        let rel_nodes = sys.node_count(rel);
        let live = sys.engine().live_node_count();

        for (j, bc, bn, bdc, bdn) in &rows {
            println!(
                "{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.6},{:.6}",
                n, ss, phi_nodes, rel_nodes, rel_card, live, j, bc, bn, bdc, bdn, t_phi, t_rel,
                t_bnd
            );
        }
        eprintln!(
            "n={:2}  |Ω|={:<22} phi={:4}  rel={:5}  live={:6}  t_rel={:.6}s  t_bnd={:.6}s",
            n, ss, phi_nodes, rel_nodes, live, t_rel, t_bnd
        );
        for (j, bc, bn, bdc, _) in &rows {
            eprintln!("        j={j}  |B_j|={bc:<14} B_j nodes={bn:5}   |B_j^down|={bdc}");
        }
    }
}
