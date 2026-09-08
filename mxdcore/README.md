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

Relation nodes are **identity reduced**: a level where the component does not
move is elided, so an invariant component is free and a transition costs one
node however many components stand still around it.

Boolean combination, `cross`, `transpose`, `post_image`, `pre_image` and the
boundary operator `B = R ∩ (L × U)` all work, and diagrams render to Graphviz.
Still to come: the driver for the non-monotone (repair/restart) case studies,
and a cross-check against the Julia/MEDDLY implementation.

Part of the Rust engine behind the
[`relibmss`](https://github.com/MssReliab/relibmss) Python package.

## License

MIT
