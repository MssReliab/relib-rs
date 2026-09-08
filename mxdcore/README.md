# relib-mxd

Matrix Decision Diagrams (MxD) for the [relib](https://github.com/MssReliab/relib-rs)
reliability toolkit: **relations** over multi-valued state vectors, alongside the
**sets** they act on, in a single forest.

A set denotes a subset of `Ω = Π_i {0..n_i-1}`; a relation denotes a subset of
`Ω × Ω`. The target application is the boundary operator `B = R ∩ (L × U)` for
multi-state system analysis. Unlike minimal path/cut vector methods, it does not
require the structure function to be monotone, so it extends to repair and restart.

```rust
use mxdcore::analysis::*;

// Two components, three states each; φ is the worst of them (a series system).
let sys = System::new(&[3, 3]);
let levels = sys.levels_from_states(|x| *x.iter().min().unwrap());
let degrade = sys.degrade();

// Transitions that drop the system out of {φ ≥ 2}.
let leaving = degrade.boundary_down(&levels, 2);
assert_eq!(leaving.count(), 2);
```

`analysis` is the layer most callers want; `MxdManager` underneath it is the raw
engine, where sets and relations are both plain node ids.

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
Results are cross-checked against MEDDLY (via Meddly.jl) on a committed fixture
of randomised cases, comparing cardinality and full membership — not node
counts, which cannot agree given the different node layout.

A non-monotone case study is in `examples/boundary_nonmonotone.rs`, with results
and a comparison against MEDDLY in `results/`.

Part of the Rust engine behind the
[`relibmss`](https://github.com/MssReliab/relibmss) Python package.

## Publishing

Not on crates.io: `publish = false` while the API is still moving. The crate is a
workspace member and runs under `cargo test` like any other.

## License

MIT
