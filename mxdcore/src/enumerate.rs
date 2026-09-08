//! Ground truth for what a diagram *means*.
//!
//! Every later stage (`cross`, `post_image`, `pre_image`) is checked against these
//! functions, so the skip rules below are the definition of the representation.
//!
//! # Skip rules
//!
//! Walking from the root (level `k-1`) down to level 0, the node reached at a given
//! level may belong to a *lower* level — the level was skipped. What that means
//! depends on which kind of diagram is being read, and the two are opposites:
//!
//! - **Set** (fully reduced): a skipped level is a **don't-care**. The variable
//!   ranges over all `n` of its states.
//! - **Relation** (identity reduced): a skipped level is the **identity**,
//!   `to_i == from_i`. It is *not* a don't-care — that would be a full `n × n`
//!   block, which is a different relation.
//!
//! The relation rule is implemented here already even though the current
//! `create_rel_node` is only quasi-reduced and never produces a skip (so the branch
//! is unreachable today). Writing it now means enabling identity reduction later is
//! a change to one reduction rule and not to the semantics.
//!
//! Reaching the terminal `One` before level 0 is exactly such a skip, and is handled
//! by the same rules.

use crate::mxd::MxdManager;
use common::prelude::*;

/// A state vector, indexed by level.
pub type StateVec = Vec<usize>;
/// A transition: `(from, to)`.
pub type Transition = (StateVec, StateVec);

impl MxdManager {
    /// All state vectors in the set rooted at `node`, in lexicographic order of
    /// `(x_{k-1}, …, x_0)`.
    pub fn enumerate_set(&self, node: NodeId) -> Vec<StateVec> {
        let k = self.num_vars();
        let mut out = Vec::new();
        let mut assign = vec![0usize; k];
        self.walk_set(node, k as i64 - 1, &mut assign, &mut out);
        out
    }

    fn walk_set(&self, node: NodeId, level: i64, assign: &mut StateVec, out: &mut Vec<StateVec>) {
        if node == self.zero() {
            return;
        }
        if level < 0 {
            debug_assert_eq!(node, self.one(), "a full path must end at the One terminal");
            out.push(assign.clone());
            return;
        }
        let l = level as usize;
        let n = self.var(l).domain;
        match self.children_at(node, l, false) {
            Some(edges) => {
                for (a, child) in edges.into_iter().enumerate() {
                    assign[l] = a;
                    self.walk_set(child, level - 1, assign, out);
                }
            }
            // Level skipped: don't-care, so every value leads to the same sub-diagram.
            None => {
                for a in 0..n {
                    assign[l] = a;
                    self.walk_set(node, level - 1, assign, out);
                }
            }
        }
    }

    /// All transitions in the relation rooted at `node`.
    pub fn enumerate_relation(&self, node: NodeId) -> Vec<Transition> {
        let k = self.num_vars();
        let mut out = Vec::new();
        let mut from = vec![0usize; k];
        let mut to = vec![0usize; k];
        self.walk_rel(node, k as i64 - 1, &mut from, &mut to, &mut out);
        out
    }

    fn walk_rel(
        &self,
        node: NodeId,
        level: i64,
        from: &mut StateVec,
        to: &mut StateVec,
        out: &mut Vec<Transition>,
    ) {
        if node == self.zero() {
            return;
        }
        if level < 0 {
            debug_assert_eq!(node, self.one(), "a full path must end at the One terminal");
            out.push((from.clone(), to.clone()));
            return;
        }
        let l = level as usize;
        let n = self.var(l).domain;
        match self.children_at(node, l, true) {
            Some(block) => {
                for a in 0..n {
                    for b in 0..n {
                        let child = block[a * n + b];
                        if child == self.zero() {
                            continue;
                        }
                        from[l] = a;
                        to[l] = b;
                        self.walk_rel(child, level - 1, from, to, out);
                    }
                }
            }
            // Level skipped: the identity, so `to == from` and only the diagonal exists.
            None => {
                for a in 0..n {
                    from[l] = a;
                    to[l] = a;
                    self.walk_rel(node, level - 1, from, to, out);
                }
            }
        }
    }

    /// Number of state vectors in the set rooted at `node`.
    pub fn cardinality_set(&self, node: NodeId) -> u64 {
        let mut memo = BddHashMap::default();
        self.count(node, self.num_vars() as i64 - 1, false, &mut memo)
    }

    /// Number of transitions in the relation rooted at `node`.
    pub fn cardinality_relation(&self, node: NodeId) -> u64 {
        let mut memo = BddHashMap::default();
        self.count(node, self.num_vars() as i64 - 1, true, &mut memo)
    }

    fn count(
        &self,
        node: NodeId,
        level: i64,
        want_rel: bool,
        memo: &mut BddHashMap<(NodeId, i64), u64>,
    ) -> u64 {
        if node == self.zero() {
            return 0;
        }
        if level < 0 {
            return 1;
        }
        if let Some(&v) = memo.get(&(node, level)) {
            return v;
        }
        let l = level as usize;
        let n = self.var(l).domain;
        let total = match self.children_at(node, l, want_rel) {
            Some(edges) => edges
                .into_iter()
                .map(|c| self.count(c, level - 1, want_rel, memo))
                .sum(),
            // A skipped level multiplies by `n` either way, but for different
            // reasons: `n` don't-care values for a set, `n` diagonal entries for a
            // relation. Both leave the sub-diagram unchanged.
            None => n as u64 * self.count(node, level - 1, want_rel, memo),
        };
        memo.insert((node, level), total);
        total
    }
}
