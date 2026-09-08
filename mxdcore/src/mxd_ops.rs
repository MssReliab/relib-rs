//! Boolean combination of sets and of relations.
//!
//! # Why sets and relations need different code
//!
//! For **sets** this is the ordinary MDD apply, transcribed from
//! `mddcore::mdd_ops`: full reduction makes a skipped level a don't-care, so a
//! sub-diagram means the same thing wherever it is encountered and the recursion
//! never has to know which level it is on. `Zero` is the empty set and `One` the
//! full one.
//!
//! For **relations** neither of those holds. A skipped level is the *identity*, so
//! `One` denotes the diagonal rather than `Ω × Ω`, and a sub-diagram's meaning
//! depends on how many levels remain below it. Every relation recursion therefore
//! carries an explicit level, and `and`/`or` have to expand a skipped operand into
//! its diagonal rather than passing it through.
//!
//! That difference is also why the operations are named per kind. `Zero` and `One`
//! are shared between the two, so a node alone does not say whether it should be
//! read as a set or as a relation — the caller has to.

use crate::mxd::{MxdManager, RelOp};
use common::prelude::*;

/// Op codes for the shared, op-keyed set table.
const OP_AND_SET: u32 = 0;
const OP_OR_SET: u32 = 1;
const OP_NOT_SET: u32 = 2;

impl MxdManager {
    // ---------------------------------------------------------------- sets

    /// Intersection of two sets.
    pub fn and_set(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let (mut f, mut g) = (f, g);
        if f > g {
            std::mem::swap(&mut f, &mut g);
        }
        if f == g {
            return f;
        }
        if f == self.zero() || g == self.zero() {
            return self.zero();
        }
        if f == self.one() {
            return g;
        }
        if g == self.one() {
            return f;
        }
        if let Some(v) = self.set_cache_get(OP_AND_SET, f, g) {
            return v;
        }
        let level = self.top_set_level(f, g);
        let fc = self.set_children(f, level);
        let gc = self.set_children(g, level);
        let children: Vec<NodeId> = fc
            .into_iter()
            .zip(gc)
            .map(|(a, b)| self.and_set(a, b))
            .collect();
        let node = self.create_set_node(level, &children);
        self.set_cache_put(OP_AND_SET, f, g, node);
        node
    }

    /// Union of two sets.
    pub fn or_set(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let (mut f, mut g) = (f, g);
        if f > g {
            std::mem::swap(&mut f, &mut g);
        }
        if f == g {
            return f;
        }
        if f == self.one() || g == self.one() {
            return self.one();
        }
        if f == self.zero() {
            return g;
        }
        if g == self.zero() {
            return f;
        }
        if let Some(v) = self.set_cache_get(OP_OR_SET, f, g) {
            return v;
        }
        let level = self.top_set_level(f, g);
        let fc = self.set_children(f, level);
        let gc = self.set_children(g, level);
        let children: Vec<NodeId> = fc
            .into_iter()
            .zip(gc)
            .map(|(a, b)| self.or_set(a, b))
            .collect();
        let node = self.create_set_node(level, &children);
        self.set_cache_put(OP_OR_SET, f, g, node);
        node
    }

    /// Complement of a set within `Ω`.
    pub fn not_set(&mut self, f: NodeId) -> NodeId {
        if f == self.zero() {
            return self.one();
        }
        if f == self.one() {
            return self.zero();
        }
        if let Some(v) = self.set_cache_get(OP_NOT_SET, f, 0) {
            return v;
        }
        // Not a terminal, so it has a level of its own.
        let level = self.level(&f).expect("non-terminal has a level");
        let fc = self.set_children(f, level);
        let children: Vec<NodeId> = fc.into_iter().map(|c| self.not_set(c)).collect();
        let node = self.create_set_node(level, &children);
        self.set_cache_put(OP_NOT_SET, f, 0, node);
        node
    }

    /// `f \ g` over sets.
    pub fn setdiff_set(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let ng = self.not_set(g);
        self.and_set(f, ng)
    }

    /// The higher of the two operands' levels. Both are non-terminal here.
    fn top_set_level(&self, f: NodeId, g: NodeId) -> Level {
        let lf = self.level(&f).expect("non-terminal has a level");
        let lg = self.level(&g).expect("non-terminal has a level");
        lf.max(lg)
    }

    /// Children of a set node at `level`, expanding a skipped level as a
    /// don't-care (the node repeated `n` times).
    fn set_children(&self, node: NodeId, level: Level) -> Vec<NodeId> {
        match self.children_at(node, level, false) {
            Some(c) => c,
            None => vec![node; self.var(level).domain],
        }
    }

    // ----------------------------------------------------------- relations

    /// Intersection of two relations.
    pub fn and_rel(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let top = self.num_vars() as i64 - 1;
        self.and_rel_at(f, g, top)
    }

    /// Union of two relations.
    pub fn or_rel(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let top = self.num_vars() as i64 - 1;
        self.or_rel_at(f, g, top)
    }

    /// Complement of a relation within `Ω × Ω`.
    pub fn not_rel(&mut self, f: NodeId) -> NodeId {
        let top = self.num_vars() as i64 - 1;
        self.not_rel_at(f, top)
    }

    /// `f \ g` over relations.
    pub fn setdiff_rel(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let ng = self.not_rel(g);
        self.and_rel(f, ng)
    }

    fn and_rel_at(&mut self, f: NodeId, g: NodeId, level: i64) -> NodeId {
        let (mut f, mut g) = (f, g);
        if f > g {
            std::mem::swap(&mut f, &mut g);
        }
        if f == self.zero() || g == self.zero() {
            return self.zero();
        }
        if level < 0 {
            return self.one();
        }
        if f == g {
            return f;
        }
        if let Some(v) = self.rel_cache_get(RelOp::And, f, g, level) {
            return v;
        }
        let l = level as usize;
        let fb = self.rel_block(f, l);
        let gb = self.rel_block(g, l);
        let block: Vec<NodeId> = fb
            .into_iter()
            .zip(gb)
            .map(|(a, b)| self.and_rel_at(a, b, level - 1))
            .collect();
        let node = self.create_rel_node(l, &block);
        self.rel_cache_put(RelOp::And, f, g, level, node);
        node
    }

    fn or_rel_at(&mut self, f: NodeId, g: NodeId, level: i64) -> NodeId {
        let (mut f, mut g) = (f, g);
        if f > g {
            std::mem::swap(&mut f, &mut g);
        }
        if f == self.zero() {
            return g;
        }
        if g == self.zero() {
            return f;
        }
        if level < 0 {
            return self.one();
        }
        if f == g {
            return f;
        }
        if let Some(v) = self.rel_cache_get(RelOp::Or, f, g, level) {
            return v;
        }
        let l = level as usize;
        let fb = self.rel_block(f, l);
        let gb = self.rel_block(g, l);
        let block: Vec<NodeId> = fb
            .into_iter()
            .zip(gb)
            .map(|(a, b)| self.or_rel_at(a, b, level - 1))
            .collect();
        let node = self.create_rel_node(l, &block);
        self.rel_cache_put(RelOp::Or, f, g, level, node);
        node
    }

    /// The `n × n` block of a relation node at `level`, expanding a skipped level
    /// into the identity it stands for: `zero` everywhere except the diagonal,
    /// which carries the node itself.
    ///
    /// Making the skip explicit here is what lets `and`/`or`/`not` be a single
    /// elementwise pass. Combining an expanded diagonal elementwise gives the right
    /// answer for all three: intersecting leaves `zero ∧ x = zero` off the diagonal,
    /// uniting leaves `zero ∨ x = x`, and complementing turns the off-diagonal
    /// `zero`s into the full relation, which is exactly the complement of the
    /// identity.
    fn rel_block(&self, node: NodeId, level: Level) -> Vec<NodeId> {
        if let Some(b) = self.children_at(node, level, true) {
            return b;
        }
        let n = self.var(level).domain;
        let zero = self.zero();
        let mut block = vec![zero; n * n];
        for a in 0..n {
            block[a * n + a] = node;
        }
        block
    }

    // ------------------------------------------------ sets -> relation

    /// Cartesian product: the relation `{ (m, m') : m ∈ f, m' ∈ g }`.
    ///
    /// Takes two **set** edges and returns a **relation** edge. Its cardinality is
    /// `|f| · |g|`.
    ///
    /// Note this is not commutative — `cross(a, b)` is the transpose of
    /// `cross(b, a)` — so unlike `and_rel`/`or_rel` there is no operand swap to
    /// canonicalise the cache key.
    pub fn cross(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let top = self.num_vars() as i64 - 1;
        self.cross_at(f, g, top)
    }

    fn cross_at(&mut self, f: NodeId, g: NodeId, level: i64) -> NodeId {
        if f == self.zero() || g == self.zero() {
            return self.zero();
        }
        if level < 0 {
            return self.one();
        }
        if let Some(v) = self.rel_cache_get(RelOp::Cross, f, g, level) {
            return v;
        }
        let l = level as usize;
        let n = self.var(l).domain;
        // Both operands are sets, so a skipped level is a don't-care on each side
        // independently. The product of two don't-cares is the full n x n block --
        // emphatically not the identity, which is why the level cannot be inferred
        // from the operands and has to be carried.
        let fa = self.set_children(f, l);
        let gb = self.set_children(g, l);
        let mut block = Vec::with_capacity(n * n);
        for a in 0..n {
            for b in 0..n {
                block.push(self.cross_at(fa[a], gb[b], level - 1));
            }
        }
        let node = self.create_rel_node(l, &block);
        self.rel_cache_put(RelOp::Cross, f, g, level, node);
        node
    }

    /// The converse relation `{ (m', m) : (m, m') ∈ f }`.
    ///
    /// Mirrors each `n × n` block about its diagonal, which is why a skipped level
    /// (the identity) transposes to itself.
    pub fn transpose(&mut self, f: NodeId) -> NodeId {
        let top = self.num_vars() as i64 - 1;
        self.transpose_at(f, top)
    }

    fn transpose_at(&mut self, f: NodeId, level: i64) -> NodeId {
        if f == self.zero() {
            return self.zero();
        }
        if level < 0 {
            return self.one();
        }
        if let Some(v) = self.rel_cache_get(RelOp::Transpose, f, 0, level) {
            return v;
        }
        let l = level as usize;
        let n = self.var(l).domain;
        let fb = self.rel_block(f, l);
        let mut block = Vec::with_capacity(n * n);
        for a in 0..n {
            for b in 0..n {
                // The cell for (from = a, to = b) of the result is the transpose of
                // the cell for (from = b, to = a) of the operand.
                block.push(self.transpose_at(fb[b * n + a], level - 1));
            }
        }
        let node = self.create_rel_node(l, &block);
        self.rel_cache_put(RelOp::Transpose, f, 0, level, node);
        node
    }

    /// The boundary operator `B = R ∩ (L × U)`: the transitions of `R` that leave
    /// `L` and land in `U`.
    ///
    /// Nothing here assumes the underlying structure function is monotone, which is
    /// the point — it is what lets repair and restart be analysed the same way as
    /// failure.
    pub fn boundary(&mut self, lower: NodeId, upper: NodeId, rel: NodeId) -> NodeId {
        let product = self.cross(lower, upper);
        self.and_rel(product, rel)
    }

    fn not_rel_at(&mut self, f: NodeId, level: i64) -> NodeId {
        if level < 0 {
            return if f == self.zero() {
                self.one()
            } else {
                self.zero()
            };
        }
        // The empty relation complements to the full one -- which is emphatically
        // not the `One` terminal, that being the identity.
        if f == self.zero() {
            return self.full_relation(level);
        }
        if let Some(v) = self.rel_cache_get(RelOp::Not, f, 0, level) {
            return v;
        }
        let l = level as usize;
        let fb = self.rel_block(f, l);
        let block: Vec<NodeId> = fb
            .into_iter()
            .map(|c| self.not_rel_at(c, level - 1))
            .collect();
        let node = self.create_rel_node(l, &block);
        self.rel_cache_put(RelOp::Not, f, 0, level, node);
        node
    }
}
