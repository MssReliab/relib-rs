//! The analysis layer itself: that its vocabulary means what it says, and that its
//! guard rails actually hold.

use mxdcore::analysis::*;

/// A series system: φ is the worst component. Small enough to state every answer
/// by hand.
#[test]
fn test_series_system_by_hand() {
    let sys = System::new(&[3, 3]);
    let levels = sys.levels_from_states(|x| *x.iter().min().unwrap());
    assert_eq!(levels.states(), 3);
    assert_eq!(sys.state_space_size(), 9);

    // {φ ≥ 2} is the single state (2,2); {φ ≥ 1} is the four states with both ≥ 1.
    assert_eq!(levels.upper(2).count(), 1);
    assert_eq!(levels.upper(1).count(), 4);
    assert_eq!(levels.lower(1).count(), 5);

    let degrade = sys.degrade();
    // From (2,2) either component can fall to 1: two ways out of {φ ≥ 2}.
    let out = degrade.boundary_down(&levels, 2);
    assert_eq!(out.count(), 2);
    // Degrading never enters a higher band in a monotone system.
    let into = degrade.boundary_up(&levels, 2);
    assert_eq!(into.count(), 0);
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

    let a = System::new(&states);
    let by_table = a.levels_from_states(phi);

    let b = System::new(&states);
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
            by_table.upper(j).count(),
            by_fold.upper(j).count(),
            "level {j}"
        );
        assert_eq!(
            by_table.upper(j).vectors(),
            by_fold.upper(j).vectors(),
            "level {j}"
        );
    }
}

/// The relation vocabulary means what it is named.
#[test]
fn test_relation_families() {
    let sys = System::new(&[3, 3]);

    // Each of two components can step down from 1 or 2: 2 steps each, and the
    // other component is free (3 states).
    let d = sys.degrade();
    assert_eq!(d.count(), 2 * 2 * 3);
    let r = sys.repair();
    assert_eq!(r.count(), 2 * 2 * 3);
    assert_eq!(d.converse(), r, "degrade and repair are converses");

    // Restart: from state 0 or 1 straight to 2. Fail: from 1 or 2 straight to 0.
    let restart = sys.restart();
    let fail = sys.fail();
    assert_eq!(restart.count(), 2 * 2 * 3);
    assert_eq!(fail.count(), 2 * 2 * 3);

    // They are NOT converses of one another once a component has more than two
    // states: converse(restart) is "the top state collapses to anything below",
    // {(2,0), (2,1)}, whereas fail is "anything above 0 drops to 0", {(1,0), (2,0)}.
    assert_ne!(restart.converse(), fail);
    let conv_restart = restart.converse();
    assert_eq!(
        conv_restart.count(),
        fail.count(),
        "…though they have the same size"
    );

    // With two states per component there is only one step, so they do coincide.
    let binary = System::new(&[2, 2]);
    let r2 = binary.restart();
    let f2 = binary.fail();
    assert_eq!(
        r2.converse(),
        f2,
        "with two states, restart and fail are converses after all"
    );

    // Nothing moves.
    let stay = sys.stay();
    assert_eq!(stay.count(), sys.state_space_size());
    // A single named step.
    let one = sys.single(0, 0, 2);
    assert_eq!(one.count(), 3, "the other component is free");
    assert_eq!(
        one.pairs(),
        (0..3)
            .map(|b| (vec![0, b], vec![2, b]))
            .collect::<Vec<_>>()
    );
}

/// Projections are existential, which is the distinction the manuscript turns on:
/// `sources` is "some step from here crosses", not "every step from here crosses".
#[test]
fn test_projections_are_existential() {
    let sys = System::new(&[3, 3]);
    let levels = sys.levels_from_states(|x| *x.iter().min().unwrap());
    let degrade = sys.degrade();
    let out = degrade.boundary_down(&levels, 2);

    // Both transitions leave (2,2), so the source projection is that single state
    // even though there are two ways out of it.
    let src = out.sources();
    assert_eq!(src.vectors(), vec![vec![2, 2]]);
    assert_eq!(out.count(), 2, "…while the boundary itself has two");

    let tgt = out.targets();
    let mut got = tgt.vectors();
    got.sort();
    assert_eq!(got, vec![vec![1, 2], vec![2, 1]]);
}

#[test]
fn test_step_forward_and_backward_are_dual() {
    let sys = System::new(&[3, 3]);
    let repair = sys.repair();
    let start = sys.all_states();
    let reachable = repair.step_forward(&start);
    let conv = repair.converse();
    assert_eq!(
        reachable,
        conv.step_backward(&start),
        "stepping forward under R is stepping back under its converse"
    );
}

/// Handles carry the system they came from. Mixing two systems is a programmer
/// error and is caught rather than silently computing on the wrong forest.
#[test]
#[should_panic(expected = "different System")]
fn test_handles_are_not_portable_between_systems() {
    let a = System::new(&[3, 3]);
    let b = System::new(&[3, 3]);
    let from_a = a.degrade();
    let from_b = b.degrade();
    // Same shape, same node ids — and still not interchangeable.
    let _ = from_b.union(&from_a);
}

#[test]
#[should_panic(expected = "out of range")]
fn test_single_rejects_a_bad_component() {
    let sys = System::new(&[3, 3]);
    let _ = sys.single(5, 0, 1);
}

#[test]
#[should_panic(expected = "has no state")]
fn test_single_rejects_a_bad_state() {
    let sys = System::new(&[3, 3]);
    let _ = sys.single(0, 0, 7);
}

/// The property the `Rc`/`Weak` handles exist for: a handle is a collection root,
/// so collecting cannot invalidate one you are still holding.
///
/// The previous design could not offer this — its handles were bare node ids, so
/// the layer had no `gc` at all and the arena only ever grew.
#[test]
fn test_handles_survive_collection() {
    let sys = System::new(&[3, 3, 3]);
    let levels = sys.levels_from_states(|x| *x.iter().min().unwrap());
    let degrade = sys.degrade();
    let kept = degrade.boundary_down(&levels, 2);
    let expected = kept.pairs();
    let expected_count = kept.count();
    assert!(expected_count > 0);

    // Build garbage and drop it.
    for a in 0..3 {
        for b in 0..3 {
            let junk = sys.single(0, a, b);
            let _ = junk.converse();
        }
    }
    let before = sys.live_node_count();
    let reclaimed = sys.gc();
    assert!(reclaimed > 0, "the dropped work should be collectable");
    assert!(sys.live_node_count() < before);

    // Everything still held means exactly what it did.
    assert_eq!(kept.pairs(), expected);
    assert_eq!(kept.count(), expected_count);
    assert_eq!(degrade.boundary_down(&levels, 2), kept);
    // …and the level sets, held by `levels`, are still roots too.
    assert_eq!(levels.upper(2).count(), 1);
}

/// Dropping a handle releases its root, so the node becomes collectable — the
/// counting has to go both ways or the arena would never shrink.
#[test]
fn test_dropping_a_handle_releases_its_root() {
    let sys = System::new(&[3, 3, 3]);
    sys.gc();
    let baseline = sys.live_node_count();

    {
        let r = sys.degrade();
        let _c = r.converse();
        assert!(sys.live_node_count() > baseline);
        // Still held here, so a collection keeps them.
        sys.gc();
        assert!(sys.live_node_count() > baseline);
    }

    // Out of scope: both handles dropped, both now collectable.
    sys.gc();
    assert_eq!(
        sys.live_node_count(),
        baseline,
        "dropped handles must stop being roots"
    );
}

/// Cloning a handle takes another reference to the same root, so one of them
/// going out of scope does not strand the other.
#[test]
fn test_cloning_shares_the_root() {
    let sys = System::new(&[3, 3]);
    let a = sys.degrade();
    let expected = a.count();
    {
        let b = a.clone();
        assert_eq!(b, a);
        drop(b);
    }
    sys.gc();
    assert_eq!(a.count(), expected, "the original must still be rooted");
}
