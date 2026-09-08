//! The case study the boundary tests share.
//!
//! `distribution_system(n)` — Sedlacek, Zaitseva, Levashenko and Kvassay (2021),
//! RESS 215:107824 §4.2. `n` factories in three states each, production
//! `P(x) = 3·Σxᵢ`, and `φ = 0 (P<5) / 1 (P>20) / 2 (otherwise)`.
//!
//! Non-monotone: producing more can drop φ from 2 to 1 when the warehouse
//! overflows. Integration tests are separate binaries, so this lives in a shared
//! module rather than being copied into each; the examples keep their own copy
//! because they are meant to be readable on their own.

#![allow(dead_code)]

use mxdcore::analysis::*;

pub const T: usize = 3;
pub const YMIN: usize = 5;
pub const YMAX: usize = 20;
pub const STATES: usize = 3;
/// Every sum at or above this gives φ = 1, so the accumulator saturates here.
pub const SAT: usize = YMAX / T + 1;

pub fn phi(sum: usize) -> usize {
    let p = T * sum;
    if p < YMIN {
        0
    } else if p > YMAX {
        1
    } else {
        2
    }
}

pub fn distribution_system(n: usize) -> (System, Levels) {
    let sys = System::new(&vec![STATES; n]);
    let levels = sys.levels_from_fold(0usize, |acc, _i, v| (acc + v).min(SAT), |&acc| phi(acc));
    (sys, levels)
}
