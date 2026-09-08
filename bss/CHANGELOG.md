## Unreleased

- **Node handles now refuse to mix forests.** Every binary and ternary operation on a node handle (`and`/`or`/`xor`/`ite` and, on the multi-state side, the arithmetic and comparison operators, plus `ZddNode`/`ZmddNode` set algebra) asserts that both operands came from the same manager, as do the manager-side methods that take node slices. Previously they read only the node **id** from the other operand — and two managers number their nodes identically, so mixing them did not fail: it computed on the wrong diagram and returned a plausible wrong answer. `relibmss` has always checked this on the Python side (`other.mdd is not self.mdd` → `ValueError`), so this closes the hole only for callers using the Rust crates directly. Behaviour within a single forest is unchanged; the check is one pointer comparison.

## relib-bss 0.14.0

- Version bump for workspace lockstep; no functional changes (the ZMDD family changes live in `relib-mdd`/`relib-mss`).

- No change to `BssMgr::minpath`/`mincut`: with a two-valued structure function the stratum and level readings described in `relib-mss` coincide identically, and the `ZddNode` set representation is unchanged.

## relib-bss 0.13.1

- `ZddNode::dot` follows the `relib-bdd` change: the `0` terminal (the empty family) and the edges into it are omitted.

## relib-bss 0.13.0

- Version bump for workspace lockstep; no functional changes (the new ZMDD Graphviz output lives in `relib-mdd`/`relib-mss`).

## relib-bss 0.12.0

- Version bump for workspace lockstep; no functional changes (the new MSS `mincut` lives in `relib-mss`).

## relib-bss 0.11.0

- Version bump for workspace lockstep; no functional changes (the new MSS Birnbaum importance `mss::MddNode::bmeas` lives in `relib-mss`).

## relib-bss 0.10.0

- Version bump for workspace lockstep; no functional changes.

## relib-bss 0.9.1

- Internal consistency: `bdd_minsol::without` `(One, NonTerminal)` now recurses into the
  reference's zero branch (`without(f, g.edge(0))`) instead of the `=> f` shortcut, matching
  the existing `level(f) < level(g)` arm and the `relib-mss` fix. Behaviorally identical
  (a non-constant monotone `g` has `g(∅)=0`); the exhaustive n≤4 minpath test still passes.

## relib-bss 0.9.0

- **Bug fix (correctness): `minpath`/`mincut` produced non-minimal sets for some
  monotone functions.** The `bdd_minsol::without` operation, in the `(One, NonTerminal)`
  case, recursed over the operand and fabricated spurious supersets — e.g.
  `minpath(x&y | z)` returned `{x,y}, {y,z}, {z}` instead of the correct `{x,y}, {z}`.
  Simple gates (`x&y`, `x|y`, `kofn`) were unaffected, which is why it went unnoticed.
  Fixed to return the `{∅}` family unchanged (a non-constant monotone `g` has `g(∅)=0`).
  Verified exhaustively against brute force for **every** boolean function of n ≤ 4
  variables.
- **New: ZDD set-family algebra over minimal path/cut vectors.** Added `BssMgr`, which
  owns a `BddMgr` (boolean structure functions) and a `ZddMgr` (set families).
  `minpath`/`mincut` now live on `BssMgr` and return a genuine **`ZddNode`** set family
  (previously a `BddNode` read with ZDD semantics), supporting `union`/`intersect`/
  `setdiff`/`product`/`divide`, plus `count`/`extract`/`dot`/`size`. The internal
  fake-ZDD → ZDD conversion is private.
- **Breaking**: `BddNode::minpath` / `BddNode::mincut` are removed; use
  `BssMgr::minpath(&node)` / `mincut(&node)` (returning `Option<ZddNode>`). `BddNode::dual`
  is unchanged (it is a pure BDD operation).
- **Breaking**: the fake-ZDD readers `BddNode::zdd_count` / `BddNode::zdd_extract` (and the
  internal `bdd_path::ZddPath` / `bdd_count::zdd_count`) are removed — they only made sense
  for the old `BddNode`-backed minsol result, now superseded by the genuine `ZddNode`.
- Source reorg: the BDD wrapper is now `bss::bdd` (`BddMgr`/`BddNode`), the manager is
  `bss::bss` (`BssMgr`), and the ZDD wrapper is `bss::zdd` — all still re-exported from the
  crate prelude, so `use bss::prelude::*` is unaffected.
- `ZddMgr` can now build set families standalone: `empty()` (`∅`), `base()` (`{∅}`),
  `singleton(label)` (`{{label}}`), and `from_sets(&[Vec<String>])`, in addition to being the
  forest behind `minpath`/`mincut`. The set-enumeration iterator was renamed
  `ZddSetPath` → **`ZddPath`** (matching `bdd_path::BddPath`).

## relib-bss 0.8.0

- Added `BddNode::dual()` — the dual structure function `φ^D(x) = ¬φ(¬x)`, computed
  by an O(size) memoized recursion (swap each node's children, complement terminals).
  It preserves monotonicity.
- Added `BddNode::mincut() -> Option<BddNode>` — the minimal **cut** vectors of the
  structure function, defined as `dual().minpath()` (the minimal path vectors of the
  dual). `None` when the function is not monotone/coherent. `minpath` gives minimal
  **path** vectors, `mincut` the minimal **cut** vectors; they are dual (series
  `x&y`: path `{x,y}`, cut `{x},{y}`; parallel `x|y` is the reverse). Additive.

## relib-bss 0.7.0

- **Breaking**: `BddNode::minpath` now returns `Option<BddNode>` (`None` when the
  function is not monotone/coherent). The separate `minpath_checked` and the
  panicking behavior are removed — a single `minpath()` returns the minimal
  path/cut sets or `None`. `bdd_minsol::minsol` is unchanged (already `Option`).

## relib-bss 0.6.0

- Coherence-checked `minpath`. `BddNode::minpath_checked() -> Option<BddNode>`
  returns `None` when the function is not monotone (coherent); the Rauzy minsol
  decomposition is only valid for coherent functions. Detection is folded into
  the minsol recursion via the local invariant `and(f0, f1) == f0` (`f0 ⇒ f1`,
  an O(1) id compare on the canonical BDD) and aborts on the first violation, so
  a non-coherent input never builds a meaningless result.
- `minpath()` keeps its signature but now **panics** on a non-coherent input
  (previously it returned a silently-wrong result).
- **Breaking**: `bdd_minsol::minsol` now returns `Option<NodeId>`.

## relib-bss 0.5.1

- Version bump for workspace lockstep; no functional changes.

## relib-bss 0.5.0

- Inherit the `relib-bdd` 0.5.0 native `ite` and commutative operand ordering
  (faster `ite`/`kofn`/`@match` construction). No API change.

## relib-bss 0.4.1

- Fix exponential-time `BddMgr::kofn`: the naive Shannon recursion never memoized
  the `(k, index)` subproblems, so it made `O(2^n)` recursive calls despite the
  polynomial-size result (n=24 took ~40 ms). Memoize on `(k, start)` → `O(n·k)`
  (n=24 now ~0.15 ms). The result BDD is unchanged (canonical); also fixes a
  `usize` underflow that panicked on `kofn(0, ..)`.
- Inherit the `relib-bdd` computed-table speedup (no API change).

## relib-bss 0.4.0

- First release on crates.io, published as `relib-bss` (import name stays `bss`).
- Add garbage collection to `BddMgr`: `gc` (mark-and-sweep), `set_gc_threshold` for
  automatic threshold-triggered collection, and `live_node_count`.
- Inherit the `relib-bdd` u32 node narrowing (roughly half-size BDD nodes) and the
  `apply` hot-path short-circuit.

## bss 0.3.3

- add clear_cache

## bss 0.3.2

- add get_children
- add get_id2 in mss

## bss 0.3.1

- add and, or, kofn

## rust-dd 0.3.0

- make workspace including the following crates:
    - common v0.3.0
    - bddcore v0.3.0
    - mddcore v0.3.0
    - bss v0.3.0
    - mss v0.3.0

## rust-dd 0.2.0

- Add mtmdd2 which uses both boolean and integer values
- Change the interface of bdd, bdd_mut, zdd, zdd_mut by removing `node` method

## rust-dd 0.1.0

- first release

