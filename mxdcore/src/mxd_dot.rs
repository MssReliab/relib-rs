//! Graphviz rendering.
//!
//! Set and relation nodes are drawn differently because the diagram mixes them:
//! a set node is a circle labelled with its variable, a relation node a
//! double circle labelled `x'`. Terminals are squares, `F` and `T`.
//!
//! Two rendering choices worth knowing when reading a picture:
//!
//! - **Relation edges are labelled `a→b`**, the transition the block cell stands
//!   for, rather than the flat child index. Edges into `Zero` are **not drawn**:
//!   a relation node has `n²` of them and is usually sparse, so drawing the empty
//!   ones buries the structure. Set nodes keep every edge, matching the sibling
//!   `mddcore` renderers.
//! - **An elided level is invisible.** That is inherent to drawing a reduced
//!   diagram, but it bites harder here than for a set, because a skipped relation
//!   level does not mean "don't care" — it means the component *does not move*. A
//!   picture with no node for a variable is asserting identity on it, and a `T`
//!   terminal reached from a relation node is the identity on everything below,
//!   not the full relation.

use crate::mxd::*;
use common::prelude::*;

impl Dot for MxdManager {
    type Node = NodeId;

    fn dot_impl<T>(&self, io: &mut T, id: &NodeId, visited: &mut BddHashSet<NodeId>)
    where
        T: std::io::Write,
    {
        if visited.contains(id) {
            return;
        }
        visited.insert(*id);
        match self.get_node(id).unwrap() {
            Node::Zero => {
                let s = format!("\"obj{id}\" [shape=square, label=\"F\"];\n");
                io.write_all(s.as_bytes()).unwrap();
            }
            Node::One => {
                let s = format!("\"obj{id}\" [shape=square, label=\"T\"];\n");
                io.write_all(s.as_bytes()).unwrap();
            }
            Node::NonTerminal(fnode) => {
                let kind = self.kind(fnode.headerid());
                let shape = if kind.is_rel() {
                    "doublecircle"
                } else {
                    "circle"
                };
                let s = format!(
                    "\"obj{}\" [shape={}, label=\"{}\"];\n",
                    fnode.id(),
                    shape,
                    self.label(id).unwrap()
                );
                io.write_all(s.as_bytes()).unwrap();

                let n = self.var(kind.level()).domain;
                for (i, child) in fnode.iter().enumerate() {
                    let label = if kind.is_rel() {
                        // Skip the empty cells; see the module docs.
                        if child == self.zero() {
                            continue;
                        }
                        format!("{}→{}", i / n, i % n)
                    } else {
                        format!("{i}")
                    };
                    self.dot_impl(io, &child, visited);
                    let s = format!(
                        "\"obj{}\" -> \"obj{}\" [label=\"{}\"];\n",
                        fnode.id(),
                        child,
                        label
                    );
                    io.write_all(s.as_bytes()).unwrap();
                }
            }
        }
    }
}
