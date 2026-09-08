//! Graphviz output.
//!
//! Assertions are on the emitted source, in the style of the sibling `mddcore`
//! renderers' tests. The point is not the exact layout but that the two node kinds
//! stay distinguishable and that relation edges name the transition.

use mxdcore::prelude::*;

fn mgr() -> MxdManager {
    let mut m = MxdManager::new();
    m.defvar("x", 2);
    m.defvar("y", 3);
    m
}

#[test]
fn test_set_node_renders_every_edge() {
    let mut m = mgr();
    let s = m.set_minterm(&[Src::Any, Src::Val(2)]);
    let dot = m.dot_string(&s);

    assert!(dot.starts_with("digraph {"));
    assert!(dot.trim_end().ends_with('}'));
    // `x` is a don't-care, so full reduction elided it; only `y` is drawn.
    assert!(dot.contains("shape=circle, label=\"y\""));
    assert!(!dot.contains("label=\"x\""));
    // A set keeps its empty edges, so all three values appear.
    for v in 0..3 {
        assert!(dot.contains(&format!("[label=\"{v}\"]")), "missing edge {v}");
    }
    assert!(dot.contains("label=\"F\""));
    assert!(dot.contains("label=\"T\""));
}

#[test]
fn test_relation_edges_name_the_transition() {
    let mut m = mgr();
    let step = m.mxd_singleton(&[Src::Any, Src::Val(0)], &[Dst::Same, Dst::Val(1)]);
    let dot = m.dot_string(&step);

    // Relation nodes are double circles and carry the primed label.
    assert!(dot.contains("shape=doublecircle, label=\"y'\""));
    // The one live cell is labelled by the transition it stands for.
    assert!(dot.contains("[label=\"0→1\"]"));
    // Empty cells are not drawn, so no F terminal appears at all.
    assert_eq!(dot.matches("→").count(), 1, "only the live cell is drawn");
    assert!(!dot.contains("label=\"F\""));
    // `x` is invariant, so identity reduction elided it -- it is simply absent.
    assert!(!dot.contains("x'"));
}

#[test]
fn test_set_and_relation_are_distinguishable() {
    let mut m = mgr();
    let s = m.set_minterm(&[Src::Val(1), Src::Val(1)]);
    let r = m.mxd_singleton(&[Src::Val(1), Src::Val(1)], &[Dst::Val(0), Dst::Val(0)]);

    let ds = m.dot_string(&s);
    let dr = m.dot_string(&r);

    assert!(ds.contains("shape=circle") && !ds.contains("doublecircle"));
    assert!(dr.contains("doublecircle") && !dr.contains("shape=circle,"));
    // Both variables are drawn on each side, under their own labels.
    assert!(ds.contains("label=\"x\"") && ds.contains("label=\"y\""));
    assert!(dr.contains("label=\"x'\"") && dr.contains("label=\"y'\""));
}

#[test]
fn test_terminals_render_alone() {
    let m = mgr();
    assert!(m.dot_string(&m.zero()).contains("label=\"F\""));
    // As a relation this T is the identity, which the module docs flag as the
    // thing a picture cannot show on its own.
    assert!(m.dot_string(&m.one()).contains("label=\"T\""));
}

#[test]
fn test_shared_subgraph_is_emitted_once() {
    let mut m = mgr();
    // Two transitions sharing the level-0 sub-diagram.
    let a = m.mxd_singleton(&[Src::Val(0), Src::Val(0)], &[Dst::Val(1), Dst::Val(1)]);
    let b = m.mxd_singleton(&[Src::Val(0), Src::Val(2)], &[Dst::Val(1), Dst::Val(1)]);
    let r = m.or_rel(a, b);
    let dot = m.dot_string(&r);

    // The shared x' node is declared once even though two edges reach it.
    let decls = dot.matches("shape=doublecircle, label=\"x'\"").count();
    assert_eq!(decls, 1, "hash-consed node must be declared once");
}
