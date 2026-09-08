//! The analysis layer itself: that its vocabulary means what it says, and that its
//! guard rails actually hold.

use mxdcore::analysis::*;

/// A series system: φ is the worst component. Small enough to state every answer
/// by hand.
#[test]
fn test_series_system_by_hand() {
    let mut sys = System::new(&[3, 3]);
    let levels = sys.levels_from_states(|x| *x.iter().min().unwrap());
    assert_eq!(levels.states(), 3);
    assert_eq!(sys.state_space_size(), 9);

    // {φ ≥ 2} is the single state (2,2); {φ ≥ 1} is the four states with both ≥ 1.
    assert_eq!(sys.count_states(levels.upper(2)), 1);
    assert_eq!(sys.count_states(levels.upper(1)), 4);
    assert_eq!(sys.count_states(levels.lower(1)), 5);

    let degrade = sys.degrade();
    // From (2,2) either component can fall to 1: two ways out of {φ ≥ 2}.
    let out = sys.boundary_down(&levels, 2, degrade);
    assert_eq!(sys.count(out), 2);
    // Degrading never enters a higher band in a monotone system.
    let into = sys.boundary_up(&levels, 2, degrade);
    assert_eq!(sys.count(into), 0);
}

/// The two ways of giving φ must agree where both are affordable.
#[test]
fn test_fold_and_table_agree() {
    let states = [3usize, 3, 3, 2];
    let phi = |x: &[usize]| {
        let s: usize = x.iter().sum();
        if s < 2 {
            0
        } else if s > 5 {
            1
        } else {
            2
        }
    };

    let mut a = System::new(&states);
    let by_table = a.levels_from_states(phi);

    let mut b = System::new(&states);
    let by_fold = b.levels_from_fold(
        0usize,
        |acc, _i, v| (acc + v).min(6),
        |&s| {
            if s < 2 {
                0
            } else if s > 5 {
                1
            } else {
                2
            }
        },
    );

    assert_eq!(by_table.states(), by_fold.states());
    for j in by_table.interior() {
        assert_eq!(
            a.count_states(by_table.upper(j)),
            b.count_states(by_fold.upper(j)),
            "level {j}"
        );
        assert_eq!(
            a.state_vectors(by_table.upper(j)),
            b.state_vectors(by_fold.upper(j)),
            "level {j}"
        );
    }
}

/// The relation vocabulary means what it is named.
#[test]
fn test_relation_families() {
    let mut sys = System::new(&[3, 3]);

    // Each of two components can step down from 1 or 2: 2 steps each, and the
    // other component is free (3 states).
    let d = sys.degrade();
    assert_eq!(sys.count(d), 2 * 2 * 3);
    let r = sys.repair();
    assert_eq!(sys.count(r), 2 * 2 * 3);
    assert_eq!(sys.converse(d), r, "degrade and repair are converses");

    // Restart: from state 0 or 1 straight to 2. Fail: from 1 or 2 straight to 0.
    let restart = sys.restart();
    let fail = sys.fail();
    assert_eq!(sys.count(restart), 2 * 2 * 3);
    assert_eq!(sys.count(fail), 2 * 2 * 3);

    // They are NOT converses of one another once a component has more than two
    // states: converse(restart) is "the top state collapses to anything below",
    // {(2,0), (2,1)}, whereas fail is "anything above 0 drops to 0", {(1,0), (2,0)}.
    assert_ne!(sys.converse(restart), fail);
    let conv_restart = sys.converse(restart);
    assert_eq!(
        sys.count(conv_restart),
        sys.count(fail),
        "…though they have the same size"
    );

    // With two states per component there is only one step, so they do coincide.
    let mut binary = System::new(&[2, 2]);
    let r2 = binary.restart();
    let f2 = binary.fail();
    assert_eq!(
        binary.converse(r2),
        f2,
        "with two states, restart and fail are converses after all"
    );

    // Nothing moves.
    let stay = sys.stay();
    assert_eq!(sys.count(stay), sys.state_space_size());
    // A single named step.
    let one = sys.single(0, 0, 2);
    assert_eq!(sys.count(one), 3, "the other component is free");
    assert_eq!(
        sys.transitions(one),
        (0..3)
            .map(|b| (vec![0, b], vec![2, b]))
            .collect::<Vec<_>>()
    );
}

/// Projections are existential, which is the distinction the manuscript turns on:
/// `sources` is "some step from here crosses", not "every step from here crosses".
#[test]
fn test_projections_are_existential() {
    let mut sys = System::new(&[3, 3]);
    let levels = sys.levels_from_states(|x| *x.iter().min().unwrap());
    let degrade = sys.degrade();
    let out = sys.boundary_down(&levels, 2, degrade);

    // Both transitions leave (2,2), so the source projection is that single state
    // even though there are two ways out of it.
    let src = sys.sources(out);
    assert_eq!(sys.state_vectors(src), vec![vec![2, 2]]);
    assert_eq!(sys.count(out), 2, "…while the boundary itself has two");

    let tgt = sys.targets(out);
    let mut got = sys.state_vectors(tgt);
    got.sort();
    assert_eq!(got, vec![vec![1, 2], vec![2, 1]]);
}

#[test]
fn test_step_forward_and_backward_are_dual() {
    let mut sys = System::new(&[3, 3]);
    let repair = sys.repair();
    let start = sys.all_states();
    let reachable = sys.step_forward(start, repair);
    let conv = sys.converse(repair);
    assert_eq!(
        reachable,
        sys.step_backward(start, conv),
        "stepping forward under R is stepping back under its converse"
    );
}

/// Handles carry the system they came from. Mixing two systems is a programmer
/// error and is caught rather than silently computing on the wrong forest.
#[test]
#[should_panic(expected = "different System")]
fn test_handles_are_not_portable_between_systems() {
    let mut a = System::new(&[3, 3]);
    let b = System::new(&[3, 3]);
    let from_a = a.degrade();
    // Same shape, same node ids — and still not interchangeable.
    let _ = b.count(from_a);
}

#[test]
#[should_panic(expected = "out of range")]
fn test_single_rejects_a_bad_component() {
    let mut sys = System::new(&[3, 3]);
    let _ = sys.single(5, 0, 1);
}

#[test]
#[should_panic(expected = "has no state")]
fn test_single_rejects_a_bad_state() {
    let mut sys = System::new(&[3, 3]);
    let _ = sys.single(0, 0, 7);
}
