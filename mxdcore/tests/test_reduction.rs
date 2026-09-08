//! Identity reduction: that it fires, that it compresses the shape the multi-state
//! workload is made of, and that it is canonical.
//!
//! The correctness of *what* the reduced diagrams denote is covered by the oracle
//! suites in `test_ops` / `test_cross` / `test_image`, which all pass unchanged
//! with reduction on. This file checks the things only reduction can show.

use mxdcore::prelude::*;

fn mgr(k: usize, domain: usize) -> MxdManager {
    let mut m = MxdManager::new();
    for i in 0..k {
        m.defvar(&format!("v{i}"), domain);
    }
    m
}

/// Live non-terminal nodes reachable from `roots`.
fn nodes(m: &mut MxdManager, roots: &[NodeId]) -> usize {
    m.gc(roots);
    m.live_node_count() - 2 // Zero and One
}

#[test]
fn test_identity_collapses_to_the_one_terminal() {
    let mut m = mgr(4, 3);
    let k = 4;
    let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);

    assert_eq!(
        id,
        m.one(),
        "every level is a diagonal block, so the whole spine elides"
    );
    assert_eq!(m.cardinality_relation(id), m.state_space_size());
    assert_eq!(nodes(&mut m, &[id]), 0, "the identity costs no nodes at all");
}

/// The shape the repair/restart workload is built from: one component moves, every
/// other component is invariant. Without identity reduction each invariant
/// component costs a node, so a single transition costs `k` of them; with it, the
/// cost is independent of `k`.
#[test]
fn test_invariant_components_are_free() {
    for k in [2usize, 5, 10, 20] {
        let mut m = mgr(k, 3);
        let mut src = vec![Src::Any; k];
        let mut dst = vec![Dst::Same; k];
        // Only the middle component moves.
        src[k / 2] = Src::Val(0);
        dst[k / 2] = Dst::Val(1);
        let t = m.mxd_singleton(&src, &dst);

        assert_eq!(
            nodes(&mut m, &[t]),
            1,
            "a single moving component should cost one node whatever k is (k = {k})"
        );
    }
}

/// Canonicity, in the form that only becomes meaningful once a level can be
/// elided: two constructions with different intermediate shapes must land on the
/// same node, not merely on equal contents.
#[test]
fn test_canonical_across_different_constructions() {
    let mut m = mgr(3, 2);

    // Route 1: state it as a pattern.
    let by_pattern = m.mxd_singleton(
        &[Src::Any, Src::Val(0), Src::Any],
        &[Dst::Same, Dst::Val(1), Dst::Same],
    );

    // Route 2: build the level-1 block by hand over an explicit identity spine
    // that reduction has to recognise and elide.
    let one = m.one();
    let zero = m.zero();
    let spine = m.create_rel_node(0, &[one, zero, zero, one]); // diagonal -> One
    assert_eq!(spine, one, "the diagonal spine must elide");
    let mid = m.create_rel_node(1, &[zero, spine, zero, zero]); // v1: 0 -> 1
    let by_hand = m.create_rel_node(2, &[mid, zero, zero, mid]); // diagonal -> mid
    assert_eq!(by_hand, mid, "the top diagonal must elide too");

    assert_eq!(by_pattern, by_hand);

    // Route 3: the union of the fully specified transitions it stands for. This one
    // never builds a diagonal block directly; reduction has to produce it.
    let mut acc = m.zero();
    for a in 0..2 {
        for c in 0..2 {
            let t = m.mxd_singleton(
                &[Src::Val(a), Src::Val(0), Src::Val(c)],
                &[Dst::Val(a), Dst::Val(1), Dst::Val(c)],
            );
            acc = m.or_rel(acc, t);
        }
    }
    assert_eq!(
        acc, by_pattern,
        "a union of explicit transitions must reduce to the same node as the pattern"
    );
}

/// Reduction must not quietly turn the full relation into the identity: they are
/// the two things the rule is most able to confuse.
#[test]
fn test_full_relation_is_not_reduced() {
    let mut m = mgr(3, 3);
    let k = 3;
    let all = m.one();
    let full = m.cross(all, all);
    let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);

    assert_ne!(full, id);
    assert_eq!(m.cardinality_relation(id), 27);
    assert_eq!(m.cardinality_relation(full), 27 * 27);
    assert_eq!(nodes(&mut m, &[full]), 3, "the full relation keeps every level");
}

/// A single-state component is a degenerate case where the identity and the full
/// relation genuinely coincide, so reduction firing there is correct rather than a
/// confusion of the two.
#[test]
fn test_unit_domain_identity_equals_full() {
    let mut m = mgr(3, 1);
    let k = 3;
    let id = m.mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);
    let all = m.one();
    let full = m.cross(all, all);
    assert_eq!(id, full, "with one state per component there is only one pair");
    assert_eq!(m.cardinality_relation(id), 1);
}

/// Reduction changes node shapes, so the images have to keep working across an
/// elided level -- this is the path that was unreachable before.
#[test]
fn test_images_across_elided_levels() {
    let mut m = mgr(4, 3);
    let k = 4;
    let mut src = vec![Src::Any; k];
    let mut dst = vec![Dst::Same; k];
    src[1] = Src::Val(0);
    dst[1] = Dst::Val(2);
    let step = m.mxd_singleton(&src, &dst);

    let at0 = m.set_minterm(&[Src::Any, Src::Val(0), Src::Any, Src::Any]);
    let at2 = m.set_minterm(&[Src::Any, Src::Val(2), Src::Any, Src::Any]);

    assert_eq!(m.post_image(at0, step), at2);
    assert_eq!(m.pre_image(at2, step), at0);
    assert_eq!(m.post_image(at2, step), m.zero());

    // The other three components are untouched, so the image preserves them.
    let one_state = m.state(&[1, 0, 2, 1]);
    let moved = m.post_image(one_state, step);
    assert_eq!(m.enumerate_set(moved), vec![vec![1, 2, 2, 1]]);
}
