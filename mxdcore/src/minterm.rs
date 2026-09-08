//! Building single minterms: one state vector, or one transition pattern.
//!
//! # The sentinel footgun this replaces
//!
//! MEDDLY specifies a transition pair with two `int` arrays and two magic values,
//! `DONT_CARE = -1` and `DONT_CHANGE = -2`. An **invariant** component must be
//! written `(-1, -2)`: don't-care on the source, don't-change on the target. Writing
//! `-1` on the target side instead is accepted and silently means something else —
//! "the component may end up in any state" rather than "the component does not
//! move" — which quietly produces the wrong relation.
//!
//! The types below make that unwritable: an invariant component is
//! `(Src::Any, Dst::Same)`, and `Dst::Any` has to be asked for by name.

use crate::mxd::MxdManager;
use common::prelude::*;

/// The source (pre-transition) value of one component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Src {
    /// Exactly this state.
    Val(usize),
    /// Any state. MEDDLY's `DONT_CARE` (`-1`).
    Any,
}

/// The target (post-transition) value of one component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dst {
    /// Exactly this state.
    Val(usize),
    /// Whatever the source was — the component does not move. MEDDLY's
    /// `DONT_CHANGE` (`-2`). This is what an invariant component needs.
    Same,
    /// Any state, *independent* of the source. MEDDLY's `DONT_CARE` (`-1`) on the
    /// primed side. Rarely what you want; see the module docs.
    Any,
}

impl MxdManager {
    /// A set containing the single state vector `values` (indexed by level).
    pub fn state(&mut self, values: &[usize]) -> NodeId {
        let pattern: Vec<Src> = values.iter().map(|&v| Src::Val(v)).collect();
        self.set_minterm(&pattern)
    }

    /// The set of state vectors matching `pattern` (indexed by level), where
    /// `Src::Any` leaves a component free.
    pub fn set_minterm(&mut self, pattern: &[Src]) -> NodeId {
        assert_eq!(
            pattern.len(),
            self.num_vars(),
            "pattern must give one entry per variable"
        );
        let mut cur = self.one();
        for level in 0..self.num_vars() {
            let n = self.var(level).domain;
            let children: Vec<NodeId> = match pattern[level] {
                Src::Val(a) => {
                    assert!(a < n, "state {a} out of range at level {level}");
                    (0..n)
                        .map(|i| if i == a { cur } else { self.zero() })
                        .collect()
                }
                Src::Any => vec![cur; n],
            };
            cur = self.create_set_node(level, &children);
        }
        cur
    }

    /// The relation containing exactly the transitions matching `(src, dst)`
    /// (both indexed by level).
    ///
    /// This is the analogue of MEDDLY's `create_from_minterm_pair`. Note that it can
    /// denote many transitions at once: `Src::Any` and `Dst::Any` are patterns, not
    /// single values. With every component `(Src::Any, Dst::Same)` the result is the
    /// identity relation over the whole state space.
    pub fn mxd_singleton(&mut self, src: &[Src], dst: &[Dst]) -> NodeId {
        assert_eq!(src.len(), self.num_vars(), "src must give one entry per variable");
        assert_eq!(dst.len(), self.num_vars(), "dst must give one entry per variable");
        let zero = self.zero();
        let mut cur = self.one();
        for level in 0..self.num_vars() {
            let n = self.var(level).domain;
            let mut block = vec![zero; n * n];
            // Which (a, b) cells of this variable's n x n block the pattern admits.
            match (src[level], dst[level]) {
                (Src::Val(a), Dst::Val(b)) => {
                    assert!(a < n && b < n, "state out of range at level {level}");
                    block[a * n + b] = cur;
                }
                (Src::Val(a), Dst::Same) => {
                    assert!(a < n, "state {a} out of range at level {level}");
                    block[a * n + a] = cur;
                }
                (Src::Val(a), Dst::Any) => {
                    assert!(a < n, "state {a} out of range at level {level}");
                    for b in 0..n {
                        block[a * n + b] = cur;
                    }
                }
                (Src::Any, Dst::Val(b)) => {
                    assert!(b < n, "state {b} out of range at level {level}");
                    for a in 0..n {
                        block[a * n + b] = cur;
                    }
                }
                // The diagonal: the component keeps whatever value it had.
                (Src::Any, Dst::Same) => {
                    for a in 0..n {
                        block[a * n + a] = cur;
                    }
                }
                // The full block: source and target both unconstrained, and
                // independent of each other. Deliberately different from the above.
                (Src::Any, Dst::Any) => {
                    for cell in block.iter_mut() {
                        *cell = cur;
                    }
                }
            }
            cur = self.create_rel_node(level, &block);
        }
        cur
    }

    /// `mxd_singleton` from MEDDLY's raw sentinel arrays, so test vectors written
    /// against the Julia/MEDDLY implementation can be replayed verbatim.
    ///
    /// `-1` is `DONT_CARE` on either side; `-2` is `DONT_CHANGE` and is only
    /// meaningful on the target side. Panics on `-2` in `unprimed`, which MEDDLY
    /// leaves undefined.
    pub fn from_meddly_sentinels(&mut self, unprimed: &[i32], primed: &[i32]) -> NodeId {
        let src: Vec<Src> = unprimed
            .iter()
            .map(|&v| match v {
                -1 => Src::Any,
                -2 => panic!("DONT_CHANGE (-2) is not meaningful on the unprimed side"),
                v if v >= 0 => Src::Val(v as usize),
                v => panic!("unknown sentinel {v} on the unprimed side"),
            })
            .collect();
        let dst: Vec<Dst> = primed
            .iter()
            .map(|&v| match v {
                -1 => Dst::Any,
                -2 => Dst::Same,
                v if v >= 0 => Dst::Val(v as usize),
                v => panic!("unknown sentinel {v} on the primed side"),
            })
            .collect();
        self.mxd_singleton(&src, &dst)
    }
}
