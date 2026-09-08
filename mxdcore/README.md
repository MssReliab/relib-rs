# relib-mxd

Matrix Decision Diagrams (MxD) for the [relib](https://github.com/MssReliab/relib-rs)
reliability toolkit: **relations** over multi-valued state vectors, alongside the
**sets** they act on, in a single forest.

A set denotes a subset of `Ω = Π_i {0..n_i-1}`; a relation denotes a subset of
`Ω × Ω`. The target application is the boundary operator `B = R ∩ (L × U)` for
multi-state system analysis. Unlike minimal path/cut vector methods, it does not
require the structure function to be monotone, so it extends to repair and restart.

```rust
use mxdcore::prelude::*;

let mut m = MxdManager::new();
m.defvar("x", 2);
m.defvar("y", 3);

// Every component invariant: the identity relation over the whole state space.
let id = m.mxd_singleton(&[Src::Any, Src::Any], &[Dst::Same, Dst::Same]);
assert_eq!(m.cardinality_relation(id), m.state_space_size());
```

## Representation

MEDDLY gives each variable two interleaved levels (unprimed, then primed). This
crate fuses them: one node per variable carrying an `n × n` block, where child
`a * n + b` is the sub-diagram for the transition `(from = a, to = b)`.

The trade is losing the sharing of target sub-graphs between different source
values — acceptable because `n` is a component's state count — in exchange for the
diagonal being local to a single node. That is what turns identity reduction into a
plain predicate rather than a rewrite only a node's parent has enough information to
perform.

**Node counts are therefore not comparable with MEDDLY's**, by design.

## Status

Early. Relation nodes are only **quasi-reduced** — no level is ever skipped.
Boolean combination of sets and of relations works. Identity reduction, and the
`cross` / `post_image` / `pre_image` operations, are still to come; the
enumeration code already implements the skip semantics they will introduce.

Part of the Rust engine behind the
[`relibmss`](https://github.com/MssReliab/relibmss) Python package.

## License

MIT
