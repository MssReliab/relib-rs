//! Timing for the boundary pipeline, at sizes past what the case studies use.
//!
//! Three phases, because they have different costs and different fixes:
//!
//! | | |
//! |---|---|
//! | `levels` | building `{U_j}` and `{L_j}` from φ given as a fold |
//! | `rel` | building `degrade ∪ restart` |
//! | `boundary` | `R ∩ (L_j × U_j)` both ways at every level, and counting it |
//!
//! `live` is the whole arena, intermediates included, so it also shows how much
//! garbage a construction leaves behind.
//!
//! Not a comparison against anything — `results/README.md` has the MEDDLY one. This
//! exists to find which phase dominates before changing it, which is how the three
//! optimisations recorded in the CHANGELOG were located.
//!
//! ```text
//! cargo run --release -p relib-mxd --example bench_boundary
//! ```

use mxdcore::analysis::*;
use std::time::{Duration, Instant};

/// The distribution system's shape: φ depends only on the component sum, and the
/// accumulator saturates because every sum at or above `sat` means the same thing.
fn build(n: usize, states: usize, sat: usize) -> (System, Levels) {
    let sys = System::new(&vec![states; n]);
    let levels = sys.levels_from_fold(
        0usize,
        move |a, _, v| (a + v).min(sat),
        move |&a| {
            if a < sat / 3 {
                0
            } else if a >= sat {
                1
            } else {
                2
            }
        },
    );
    (sys, levels)
}

const CASES: &[(usize, usize, usize)] = &[
    (22, 3, 7),   // the committed case study
    (60, 3, 7),
    (120, 3, 7),
    (60, 5, 20),  // more states per component: the block is n², so this is the hard axis
    (100, 5, 20),
];

fn best_of<T>(rounds: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut last = f();
    for _ in 0..rounds {
        let t = Instant::now();
        last = f();
        best = best.min(t.elapsed());
    }
    (best, last)
}

fn main() {
    for &(n, states, sat) in CASES {
        let (t_lv, _) = best_of(3, || build(n, states, sat));
        let (sys, levels) = build(n, states, sat);

        let (t_rel, _) = best_of(3, || sys.degrade().union(&sys.restart()));
        let rel = sys.degrade().union(&sys.restart());

        let (t_bnd, total) = best_of(3, || {
            let mut sum = 0u128;
            for j in levels.interior() {
                sum += rel.boundary_up(&levels, j).count();
                sum += rel.boundary_down(&levels, j).count();
            }
            sum
        });

        println!(
            "n={n:3} states={states} | levels {t_lv:>9.3?} | rel {t_rel:>9.3?} | \
             boundary {t_bnd:>9.3?} | rel {:>4} nodes | live {:>6} | Σ|B| {total}",
            rel.node_count(),
            sys.live_node_count(),
        );
    }
}
