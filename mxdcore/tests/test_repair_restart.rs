//! Repair and restart boundaries on the non-monotone distribution system.
//!
//! Pins `examples/repair_restart.rs` against the numbers MEDDLY produces, and
//! against the structural facts that make those numbers interesting — the
//! wrong-direction crossings a monotone system cannot have.
//!
//! Cardinalities only; relation node counts differ from MEDDLY's by construction.

mod common;

use common::distribution_system;
use mxdcore::analysis::*;

/// `(n, relation, j, up, down)` as produced by MEDDLY through
/// `MDDMinsol.boundary_level` / `boundary_level_down`.
///
/// Transcribed mechanically from the reference CSV, not by hand. The first
/// attempt at this table was typed out, and two of its entries were numbers that
/// appear nowhere in the data — the test caught it, but only because it happened
/// to disagree rather than happening to match.
const REFERENCE: &[(usize, &str, usize, u128, u128)] = &[
    (4, "dec", 1, 0, 16),
    (4, "dec", 2, 16, 16),
    (4, "inc", 1, 16, 0),
    (4, "inc", 2, 16, 16),
    (4, "restart", 1, 20, 0),
    (4, "restart", 2, 20, 28),
    (4, "loop", 1, 20, 16),
    (4, "loop", 2, 36, 44),
    (8, "dec", 1, 0, 64),
    (8, "dec", 2, 4984, 64),
    (8, "inc", 1, 64, 0),
    (8, "inc", 2, 64, 4984),
    (8, "restart", 1, 72, 0),
    (8, "restart", 2, 72, 7112),
    (8, "loop", 1, 72, 64),
    (8, "loop", 2, 5056, 7176),
    (12, "dec", 1, 0, 144),
    (12, "dec", 2, 86328, 144),
    (12, "inc", 1, 144, 0),
    (12, "inc", 2, 144, 86328),
    (12, "restart", 1, 156, 0),
    (12, "restart", 2, 156, 113652),
    (12, "loop", 1, 156, 144),
    (12, "loop", 2, 86484, 113796),
    (14, "dec", 1, 0, 196),
    (14, "dec", 2, 248248, 196),
    (14, "inc", 1, 196, 0),
    (14, "inc", 2, 196, 248248),
    (14, "restart", 1, 210, 0),
    (14, "restart", 2, 210, 318318),
    (14, "loop", 1, 210, 196),
    (14, "loop", 2, 248458, 318514),
];

fn named(sys: &mut System, name: &str) -> Transitions {
    match name {
        "dec" => sys.degrade(),
        "inc" => sys.repair(),
        "restart" => sys.restart(),
        "loop" => {
            let d = sys.degrade();
            let r = sys.restart();
            sys.union(d, r)
        }
        other => panic!("unknown relation `{other}`"),
    }
}

#[test]
fn test_boundaries_match_meddly() {
    for &(n, name, j, want_up, want_down) in REFERENCE {
        let (mut sys, levels) = distribution_system(n);
        let rel = named(&mut sys, name);
        let up = sys.boundary_up(&levels, j, rel);
        let down = sys.boundary_down(&levels, j, rel);
        assert_eq!(sys.count(up), want_up, "n={n} {name} j={j}: upward boundary");
        assert_eq!(
            sys.count(down),
            want_down,
            "n={n} {name} j={j}: downward boundary"
        );
    }
}

/// The point of the whole exercise: degrading a component can move the system
/// **into** the better level set, and repairing one can move it **out**. Neither
/// is possible when φ is monotone, and neither is expressible as a minimal path or
/// cut vector.
#[test]
fn test_wrong_direction_crossings_exist() {
    for n in [4usize, 8, 12] {
        let (mut sys, levels) = distribution_system(n);
        let dec = sys.degrade();
        let inc = sys.repair();

        let dec_up = sys.boundary_up(&levels, 2, dec);
        let inc_down = sys.boundary_down(&levels, 2, inc);
        assert!(
            sys.count(dec_up) > 0,
            "n={n}: degrading must be able to enter the upper set"
        );
        assert!(
            sys.count(inc_down) > 0,
            "n={n}: repairing must be able to leave it"
        );

        // At level 1 the system is still ordinary: below `ymin` nothing overflows,
        // so degrading only ever crosses downward there.
        let dec_up1 = sys.boundary_up(&levels, 1, dec);
        assert_eq!(
            sys.count(dec_up1),
            0,
            "n={n}: at level 1 degrading cannot cross upward"
        );
    }
}

/// `inc` is the converse of `dec`, so their boundaries must be converses too:
/// `converse(dec ∩ (L × U)) == inc ∩ (U × L)`. This ties the two wrong-direction
/// numbers together as one fact rather than two coincidences, and it fails if
/// either the converse or the boundary reads a transition backwards.
#[test]
fn test_boundaries_are_converse_under_transpose() {
    for n in [3usize, 5, 9] {
        let (mut sys, levels) = distribution_system(n);
        let dec = sys.degrade();
        let inc = sys.repair();
        assert_eq!(
            sys.converse(dec),
            inc,
            "n={n}: degrade and repair must be converses"
        );

        for j in levels.interior() {
            let dec_up = sys.boundary_up(&levels, j, dec);
            let inc_down = sys.boundary_down(&levels, j, inc);
            assert_eq!(
                sys.converse(dec_up),
                inc_down,
                "n={n} j={j}: boundaries must be converses"
            );
        }
    }
}

/// Restarting overshoots harder than repairing a step at a time: it jumps a
/// component straight to the top, so it drops out of `{φ ≥ 2}` more often than
/// `inc` does. That is the operational cost of restart-on-failure here, and it is
/// only visible as a transition count.
#[test]
fn test_restart_overshoots_more_than_gradual_repair() {
    for n in [8usize, 12, 16] {
        let (mut sys, levels) = distribution_system(n);
        let inc = sys.repair();
        let restart = sys.restart();

        let inc_down = sys.boundary_down(&levels, 2, inc);
        let restart_down = sys.boundary_down(&levels, 2, restart);
        let (a, b) = (sys.count(inc_down), sys.count(restart_down));
        assert!(
            b > a,
            "n={n}: restart ({b}) should leave the upper set more often than a single \
             repair step ({a})"
        );
    }
}

/// The operational loop — degrade a step, restore to new — is the union of the
/// two, and its boundaries are the unions of theirs.
#[test]
fn test_loop_is_the_union_of_its_parts() {
    let n = 10;
    let (mut sys, levels) = distribution_system(n);
    let dec = sys.degrade();
    let restart = sys.restart();
    let looped = sys.union(dec, restart);

    for up in [true, false] {
        let both = if up {
            sys.boundary_up(&levels, 2, looped)
        } else {
            sys.boundary_down(&levels, 2, looped)
        };
        let parts = {
            let a = if up {
                sys.boundary_up(&levels, 2, dec)
            } else {
                sys.boundary_down(&levels, 2, dec)
            };
            let b = if up {
                sys.boundary_up(&levels, 2, restart)
            } else {
                sys.boundary_down(&levels, 2, restart)
            };
            sys.union(a, b)
        };
        assert_eq!(
            both, parts,
            "the boundary of a union is the union of boundaries"
        );
    }
}
