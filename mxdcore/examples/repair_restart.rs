//! Boundary transitions under **repair and restart** on a non-monotone system.
//!
//! `distribution_system(n)` from Sedlacek, Zaitseva, Levashenko and Kvassay
//! (2021), RESS 215:107824 §4.2: `n` factories in three states each, production
//! `P(x) = 3·Σxᵢ`, and
//!
//! ```text
//! φ(x) = 0  if P < 5      1  if P > 20      2  otherwise
//! ```
//!
//! Non-monotone because of the middle case: producing *more* can drop φ from 2 to
//! 1 when the warehouse overflows. Four transition relations, both crossing
//! directions per level:
//!
//! | | |
//! |---|---|
//! | `dec` | one component degrades a step |
//! | `inc` | one component is repaired a step |
//! | `restart` | one component is replaced outright |
//! | `loop` | `dec ∪ restart`, the operational cycle |
//!
//! # What the numbers show
//!
//! Degrading a component can move the system **into** the better level set, and
//! repairing one can move it **out**. Both are identically zero in a monotone
//! system. Those wrong-direction crossings are the quantity the boundary operator
//! exists for; minimal path and cut vectors cannot express them, being undefined
//! for a non-monotone φ.
//!
//! Cross-checked against MEDDLY for `n = 2..14`; see `results/README.md`.
//!
//! ```text
//! cargo run --release -p relib-mxd --example repair_restart -- [N...]
//! ```

use mxdcore::analysis::*;
use std::time::Instant;

/// φ depends only on the component sum, so it is given as a fold. The accumulator
/// saturates at 7 because every larger sum means the same thing (`3·7 = 21 > 20`);
/// without that the diagram would grow with the sum's range for no benefit.
fn distribution_system(n: usize) -> (System, Levels) {
    let sys = System::new(&vec![3; n]);
    let levels = sys.levels_from_fold(
        0usize,
        |acc, _i, v| (acc + v).min(7),
        |&acc| {
            let p = 3 * acc;
            if p < 5 {
                0
            } else if p > 20 {
                1
            } else {
                2
            }
        },
    );
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

    // Warm up, so the first row is not paying for cold allocator pages.
    let _ = distribution_system(3);

    println!("n,relation,j,up_card,down_card,up_nodes,down_nodes,rel_card,rel_nodes,t_bnd_s");
    for n in ns {
        let (sys, levels) = distribution_system(n);

        let dec = sys.degrade();
        let inc = sys.repair();
        let restart = sys.restart();
        let looped = dec.union(&restart);
        let families = [
            ("dec", dec),
            ("inc", inc),
            ("restart", restart),
            ("loop", looped),
        ];

        for (name, rel) in families {
            let rel_card = rel.count();
            let rel_nodes = rel.node_count();
            for j in levels.interior() {
                let t = Instant::now();
                let up = rel.boundary_up(&levels, j);
                let down = rel.boundary_down(&levels, j);
                let secs = t.elapsed().as_secs_f64();
                println!(
                    "{},{},{},{},{},{},{},{},{},{:.6}",
                    n,
                    name,
                    j,
                    up.count(),
                    down.count(),
                    up.node_count(),
                    down.node_count(),
                    rel_card,
                    rel_nodes,
                    secs
                );
                if name == "dec" && up.count() > 0 {
                    eprintln!(
                        "n={n:2} j={j}: degrading crosses UP {} times -- only possible \
                         because φ is non-monotone",
                        up.count()
                    );
                }
            }
        }
    }
}
