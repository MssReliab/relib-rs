//! Matrix Decision Diagrams (MxD) for the relib toolkit: **relations** over
//! multi-valued state vectors, alongside the **sets** they act on, in one forest.
//!
//! A set denotes a subset of `Ω = Π_i {0..n_i-1}`; a relation denotes a subset of
//! `Ω × Ω`. The intended use is the boundary operator `B = R ∩ (L × U)` for
//! multi-state system analysis, which unlike minimal-vector methods does not need
//! the structure function to be monotone — so it also covers repair/restart.
//!
//! # Representation
//!
//! MEDDLY gives each variable two interleaved levels (unprimed, then primed). This
//! crate fuses them: one node per variable with `n × n` edges, child `a * n + b`
//! meaning `(from = a, to = b)`. The trade is losing the sharing of target
//! sub-graphs between different source values — acceptable because `n` is a
//! component's state count — in exchange for the diagonal being *local* to a node,
//! which is what makes identity reduction a plain predicate instead of a rewrite
//! that only the parent has enough information to perform.
//!
//! Consequently node counts here are **not** comparable with MEDDLY's, by design.
//!
//! # Status
//!
//! Sets and relations can be combined, crossed and transposed ([`mxd_ops`]), and
//! the boundary operator `B = R ∩ (L × U)` is available as
//! [`MxdManager::boundary`](mxd::MxdManager::boundary). The image operations
//! `post_image` and `pre_image` are still to come.
//!
//! Relation nodes are currently only **quasi-reduced** (no level is ever skipped).
//! Identity reduction — eliding diagonal blocks — is the next step;
//! [`enumerate`] already reads the skips it will introduce, and [`mxd_ops`] already
//! expands them.
//!
//! # Reading a node
//!
//! `Zero` and `One` are shared between sets and relations, so a node on its own
//! does not say which it is; the caller has to know. This matters most for `One`,
//! which as a set is all of `Ω` but as a relation is the **identity**, not `Ω × Ω`.
//! That is why the operations come in `_set` and `_rel` pairs.
//!
//! ```
//! use mxdcore::prelude::*;
//!
//! let mut m = MxdManager::new();
//! m.defvar("x", 2);
//! m.defvar("y", 3);
//!
//! // Every component invariant: the identity relation over the whole state space.
//! let id = m.mxd_singleton(&[Src::Any, Src::Any], &[Dst::Same, Dst::Same]);
//! assert_eq!(m.cardinality_relation(id), m.state_space_size());
//! ```

pub mod enumerate;
pub mod minterm;
pub mod mxd;
pub mod mxd_ops;
pub mod nodes;

pub mod prelude {
    pub use crate::enumerate::{StateVec, Transition};
    pub use crate::minterm::{Dst, Src};
    pub use crate::mxd::{HeaderKind, MxdManager, Node, VarInfo};
    pub use crate::nodes::NonTerminalMxD;
    pub use common::prelude::*;
}
