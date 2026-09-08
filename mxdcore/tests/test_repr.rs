//! The vertical slice: does the representation mean what it is supposed to mean?
//!
//! Everything here is checked against hand-written expected values rather than
//! against another part of the crate, because these tests are what pins the
//! semantics that `cross` / `post_image` / `pre_image` will be built on.

use mxdcore::prelude::*;
use std::collections::HashSet;

/// Two variables: `x` with 2 states at level 0, `y` with 3 states at level 1.
/// Level 1 is the root, so a state vector prints as `[x, y]` but is read `y` first.
fn mgr2() -> MxdManager {
    let mut m = MxdManager::new();
    m.defvar("x", 2);
    m.defvar("y", 3);
    m
}

fn set_of(pairs: Vec<(Vec<usize>, Vec<usize>)>) -> HashSet<(Vec<usize>, Vec<usize>)> {
    pairs.into_iter().collect()
}

#[test]
fn test_identity_cardinality() {
    let mut m = mgr2();
    // Every component invariant. In MEDDLY terms: unprimed all DONT_CARE, primed
    // all DONT_CHANGE.
    let id = m.mxd_singleton(&[Src::Any, Src::Any], &[Dst::Same, Dst::Same]);

    assert_eq!(m.state_space_size(), 6);
    assert_eq!(m.cardinality_relation(id), 6);

    // And it really is the identity, not merely the right size.
    let expected = set_of(vec![
        (vec![0, 0], vec![0, 0]),
        (vec![0, 1], vec![0, 1]),
        (vec![0, 2], vec![0, 2]),
        (vec![1, 0], vec![1, 0]),
        (vec![1, 1], vec![1, 1]),
        (vec![1, 2], vec![1, 2]),
    ]);
    let got: HashSet<_> = m.enumerate_relation(id).into_iter().collect();
    assert_eq!(got, expected);
}

#[test]
fn test_singleton_enumerate() {
    let mut m = mgr2();

    // One fully specified transition: (x=0, y=2) -> (x=1, y=0).
    let t = m.mxd_singleton(
        &[Src::Val(0), Src::Val(2)],
        &[Dst::Val(1), Dst::Val(0)],
    );
    assert_eq!(
        m.enumerate_relation(t),
        vec![(vec![0, 2], vec![1, 0])],
        "a fully specified pair is exactly one transition"
    );
    assert_eq!(m.cardinality_relation(t), 1);

    // "y increments from 1 to 2, x does not move" -- the shape every repair or
    // failure transition takes.
    let inc = m.mxd_singleton(&[Src::Any, Src::Val(1)], &[Dst::Same, Dst::Val(2)]);
    let expected = set_of(vec![
        (vec![0, 1], vec![0, 2]),
        (vec![1, 1], vec![1, 2]),
    ]);
    let got: HashSet<_> = m.enumerate_relation(inc).into_iter().collect();
    assert_eq!(got, expected);
    assert_eq!(m.cardinality_relation(inc), 2);
}

#[test]
fn test_canonicity() {
    let mut m = mgr2();

    // Reaching the same relation through the pattern API and through raw node
    // construction must land on the same node.
    //
    // Note what this can and cannot show while the forest is only quasi-reduced:
    // no level is ever skipped, so a relation has essentially one shape and this
    // tests hash-consing rather than canonicalisation. The interesting version --
    // two genuinely different shapes collapsing to one -- only becomes possible
    // once identity reduction can elide diagonal blocks, and belongs with it.
    let by_pattern = m.mxd_singleton(&[Src::Any, Src::Val(0)], &[Dst::Same, Dst::Val(1)]);

    let one = m.one();
    let zero = m.zero();
    let x_diag = m.create_rel_node(0, &[one, zero, zero, one]);
    let y_block = {
        let mut blk = vec![zero; 9];
        blk[1] = x_diag; // (from y=0, to y=1)
        blk
    };
    let by_hand = m.create_rel_node(1, &y_block);

    assert_eq!(
        by_pattern, by_hand,
        "structurally equal relations must be the same node"
    );

    // Neighbouring relations must not collide.
    let other = m.mxd_singleton(&[Src::Any, Src::Val(0)], &[Dst::Same, Dst::Val(2)]);
    assert_ne!(by_pattern, other);

    // A block with no admitted transition is the empty relation, at any level.
    let empty_lo = m.create_rel_node(0, &vec![zero; 4]);
    let empty_hi = m.create_rel_node(1, &vec![zero; 9]);
    assert_eq!(empty_lo, zero);
    assert_eq!(empty_hi, zero);
}

/// The direction of a transition must survive the round trip. This is the test
/// that fails if the `a * n + b` block index is ever read transposed -- the bug
/// that `pre_image`/`post_image` will be most exposed to.
#[test]
fn test_direction_is_not_symmetric() {
    let mut m = mgr2();

    let forward = m.mxd_singleton(&[Src::Val(0), Src::Val(0)], &[Dst::Val(1), Dst::Val(2)]);
    let backward = m.mxd_singleton(&[Src::Val(1), Src::Val(2)], &[Dst::Val(0), Dst::Val(0)]);

    assert_ne!(forward, backward);
    assert_eq!(m.enumerate_relation(forward), vec![(vec![0, 0], vec![1, 2])]);
    assert_eq!(m.enumerate_relation(backward), vec![(vec![1, 2], vec![0, 0])]);
}

#[test]
fn test_sentinel_distinction() {
    let mut m = mgr2();

    // The bug class the Src/Dst types exist to prevent: on the primed side,
    // DONT_CHANGE (-2) and DONT_CARE (-1) are different relations, and using the
    // latter for an invariant component is silently wrong.
    let invariant = m.mxd_singleton(&[Src::Any, Src::Any], &[Dst::Same, Dst::Same]);
    let unconstrained = m.mxd_singleton(&[Src::Any, Src::Any], &[Dst::Any, Dst::Any]);

    assert_ne!(invariant, unconstrained, "identity is not the full relation");
    assert_eq!(m.cardinality_relation(invariant), 6, "|Ω|");
    assert_eq!(m.cardinality_relation(unconstrained), 36, "|Ω|²");

    // The same distinction on a single component.
    let one_same = m.mxd_singleton(&[Src::Val(0), Src::Any], &[Dst::Val(0), Dst::Same]);
    let one_any = m.mxd_singleton(&[Src::Val(0), Src::Any], &[Dst::Val(0), Dst::Any]);
    assert_ne!(one_same, one_any);
    assert_eq!(m.cardinality_relation(one_same), 3);
    assert_eq!(m.cardinality_relation(one_any), 9);
}

#[test]
fn test_meddly_sentinels_agree() {
    let mut m = mgr2();
    let typed = m.mxd_singleton(&[Src::Any, Src::Val(1)], &[Dst::Same, Dst::Val(2)]);
    let raw = m.from_meddly_sentinels(&[-1, 1], &[-2, 2]);
    assert_eq!(typed, raw, "the compatibility constructor must agree");
}

#[test]
fn test_set_roundtrip() {
    let mut m = mgr2();

    let s = m.state(&[1, 2]);
    assert_eq!(m.enumerate_set(s), vec![vec![1, 2]]);
    assert_eq!(m.cardinality_set(s), 1);

    // A don't-care component really is a don't-care: full reduction elides the
    // level, and enumeration has to put it back.
    let any_x = m.set_minterm(&[Src::Any, Src::Val(0)]);
    let got: HashSet<_> = m.enumerate_set(any_x).into_iter().collect();
    assert_eq!(
        got,
        [vec![0, 0], vec![1, 0]].into_iter().collect::<HashSet<_>>()
    );
    assert_eq!(m.cardinality_set(any_x), 2);

    // The whole space, and the empty set.
    let all = m.set_minterm(&[Src::Any, Src::Any]);
    assert_eq!(all, m.one(), "an unconstrained set reduces to the One terminal");
    assert_eq!(m.cardinality_set(all), 6);
    assert_eq!(m.cardinality_set(m.zero()), 0);
    assert!(m.enumerate_set(m.zero()).is_empty());
}

#[test]
fn test_gc_keeps_reachable() {
    let mut m = mgr2();
    let keep = m.mxd_singleton(&[Src::Any, Src::Val(1)], &[Dst::Same, Dst::Val(2)]);
    let before = m.cardinality_relation(keep);

    // Garbage to be collected.
    let _junk = m.mxd_singleton(&[Src::Val(1), Src::Val(0)], &[Dst::Val(0), Dst::Val(0)]);
    let live_before = m.live_node_count();
    let reclaimed = m.gc(&[keep]);

    assert!(reclaimed > 0, "the unreferenced relation should be reclaimed");
    assert_eq!(m.live_node_count(), live_before - reclaimed);
    assert_eq!(
        m.cardinality_relation(keep),
        before,
        "surviving node ids stay valid and mean the same thing"
    );
}
