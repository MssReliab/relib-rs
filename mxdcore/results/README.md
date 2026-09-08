# Non-monotone case study: relib-mxd vs MEDDLY

`distribution_system(n)` — Sedlacek, Zaitseva, Levashenko, Kvassay (2021),
RESS 215:107824 §4.2. `n` factories in three states each, production
`P(x) = 3·Σxᵢ`, and

```
φ(x) = 0  if P < 5      1  if P > 20      2  otherwise
```

Non-monotone: producing more can drop φ from 2 to 1 (the warehouse overflows), so
minimal path/cut vectors do not apply and the boundary operator
`B_j = R ∩ (L_j × U_j)` is what is left. `R` is any single component stepping one
level up or down.

- **Rust**: `cargo run --release -p relib-mxd --example boundary_nonmonotone`
  → `boundary_nonmonotone_rust.csv` (this directory).
- **MEDDLY**: `MDDMinsol/scripts/boundary_nonmonotone.jl` via Meddly.jl
  → `MDDMinsol/results/boundary_nonmonotone_seed20260907.csv` in the research
  repository.

Both measured on the same machine: Apple M4, macOS 26.1, rustc 1.96.0.

## Correctness

**All 42 `(n, j)` rows agree exactly**, `n = 2..22`, `j = 1, 2`, for `|B_j|`,
`|B_j^down|` and `|R|`.

Three rows initially looked off by a few units. They are not: the Julia script
writes cardinalities through `%.6g`, so the CSV keeps only six significant digits.
Re-running those cases with full precision confirmed exact agreement — e.g.
`n = 21, j = 2` is `3881115` on both sides, which the CSV had recorded as
`3.88112e+06`.

Selected values:

| n | \|B_1\| | \|B_2\| |
|---|---|---|
| 5 | 25 | 155 |
| 10 | 100 | 24460 |
| 22 | 484 | 5314100 |

`|B_1| = n²` exactly across the whole range; `test_distribution_system.rs` pins
both that closed form and the MEDDLY table for `n ≤ 10`.

Note `|Ω| = 3^22 ≈ 3.1 × 10^10` at the top of the range, and none of this
enumerates the state space.

## Timing

Seconds; Julia from the committed combined run (which amortises warm-up across all
`n` and is therefore the *more favourable* of its two measurements — running it one
process per `n` gives ~9 ms at `n = 22` rather than ~4 ms). Rust is the median of
five runs.

| n | `t_rel` MEDDLY | `t_rel` rust | ×   | `t_bnd` MEDDLY | `t_bnd` rust | ×   |
|---|---|---|---|---|---|---|
| 8  | 0.000142 | 0.000049 | 2.9 | 0.000875 | 0.000277 | 3.2 |
| 12 | 0.000292 | 0.000091 | 3.2 | 0.001558 | 0.000443 | 3.5 |
| 16 | 0.000522 | 0.000125 | 4.2 | 0.002244 | 0.000576 | 3.9 |
| 20 | 0.000808 | 0.000182 | 4.4 | 0.003132 | 0.000639 | 4.9 |
| 22 | 0.000963 | 0.000204 | 4.7 | 0.004076 | 0.000682 | 6.0 |

### What this does and does not say

These are sub-millisecond measurements of two quite different systems, so read
the ratios as an order of magnitude, not a benchmark result.

- It is **end-to-end wall clock for the same computation**, not a controlled
  comparison of algorithms. The Julia side's timed path is C++ MEDDLY reached
  through `ccall`; almost none of it is Julia.
- The two use **different relation representations**, and for this particular `R`
  the difference is large in this crate's favour: fusing a variable's source and
  target into one node makes "component `i` steps up or down" a single node, so
  `R` costs `n` nodes here against MEDDLY's `4n - 1`. Part of the speed is that,
  not implementation quality.
- `t_build` / `t_phi` are **not** compared. MEDDLY builds φ as an MTMDD and
  thresholds it; this crate has no value-carrying diagram and builds the level
  sets `{x : φ(Σxᵢ) ⋛ j}` directly. Different work.

## A note on the `live_nodes` column

`boundary_nonmonotone_rust.csv` carries a `live_nodes` column. It is the size of
the **whole arena**, intermediates included — not a property of any result. It
therefore changes when the example is restructured even though nothing computed
changes: building `degrade` and `repair` as separate relations and then uniting
them retains a few more intermediates than accumulating one union does, which
moved the column by about `n` without moving a single cardinality or per-diagram
node count. Read `b_nodes` / `bd_nodes` for the size of an actual result.

## Node counts

**Set** diagrams use the same convention on both sides — fully reduced, one level
per variable — so their sizes are comparable, and they match exactly: the largest
level set is `7n − 16` nodes on both (138 at `n = 22`).

**Relation** diagrams are not comparable and are not compared. This crate gives a
variable one level with an `n × n` block; MEDDLY interleaves an unprimed and a
primed level. At `n = 22`, `R` is 22 nodes here and 87 there, and `B_2` is 275
here against 563 there. Neither number is wrong; they count different things.

---

# Repair and restart boundaries (non-monotone)

Same system, four transition relations instead of one, and both crossing
directions per level. This is the case the boundary operator exists for: minimal
path and cut vectors are undefined here, so none of these numbers has an
alternative derivation.

| | |
|---|---|
| `dec` | `x_i → x_i − 1`, one component degrades a step |
| `inc` | `x_i → x_i + 1`, one component is repaired a step |
| `restart` | `x_i → top`, one component is replaced outright |
| `loop` | `dec ∪ restart`, degrade gradually, restore to new |

```
up_j   = R ∩ (L_j × U_j)   entering {φ ≥ j}
down_j = R ∩ (U_j × L_j)   leaving it
```

- **Rust**: `cargo run --release -p relib-mxd --example repair_restart`
  → `repair_restart_rust.csv` (this directory), `n = 2..22`.
- **MEDDLY**: no script existed — `repair_relation` was exported by `MDDMinsol`
  but had neither a script nor a test, so the reference was generated for this
  comparison and is not part of the research repository's committed results.

## Correctness

**All 104 `(n, relation, level)` rows agree exactly** with MEDDLY over `n = 2..14`,
for both directions and all four relations. `tests/test_repair_restart.rs` pins 32
of them.

## The numbers, at level 2 (`U_2 = {φ = 2}`, the productive band)

| n | dec up | dec down | inc up | inc down | restart up | restart down |
|---|---|---|---|---|---|---|
| 4 | 16 | 16 | 16 | 16 | 20 | 28 |
| 8 | 4 984 | 64 | 64 | 4 984 | 72 | 7 112 |
| 12 | 86 328 | 144 | 144 | 86 328 | 156 | 113 652 |
| 16 | 615 888 | 256 | 256 | 615 888 | 272 | 773 136 |
| 20 | 2 790 720 | 400 | 400 | 2 790 720 | 420 | 3 391 500 |
| 22 | 5 313 616 | 484 | 484 | 5 313 616 | 506 | 6 375 754 |

## What is worth reading off them

**Degrading enters the productive band, and repairing leaves it.** `dec up` and
`inc down` are large and grow with `n`; in a monotone system both are identically
zero. This is the overflow: above `ymax` the system is in `φ = 1`, so *reducing*
production returns it to `φ = 2`, and *increasing* production pushes it out.

**The two are the same fact.** `dec up = inc down` at every `n`, necessarily —
`inc` is the converse of `dec`, so their boundaries are converses too. Pinned in
`test_boundaries_are_converse_under_transpose` via `transpose`, so it is checked
rather than observed.

**Restart overshoots harder than gradual repair.** `restart down` exceeds
`inc down` throughout (6 375 754 vs 5 313 616 at `n = 22`): sending a component
straight to its top state leaves the productive band more often than raising it
one step. That is the operational cost of restart-on-failure in this system, and
it is visible only as a transition count — there is no state-based quantity it
corresponds to.

**Level 1 stays ordinary.** `dec up` is 0 at `j = 1` for every `n`: below `ymin`
nothing overflows, so the system is locally monotone there. The non-monotonicity
is confined to the upper band, and the boundary operator localises it.

## Scale

`|Ω| = 3^22 ≈ 3.1 × 10^10` at the top of the range, and nothing enumerates it.
The whole `n = 2..22` sweep — four relations, two levels, both directions — runs
in **0.37 s** wall clock.

## The complement

`relib-mss`'s `minpath` and `mincut` **refuse this φ**, returning `None`, which is
correct: minimal vectors are not defined for a non-monotone structure function.
`mss/tests/test_nonmonotone_minpath.rs` pins that, together with the check that
the same construction *is* accepted at `n = 3`, where the sum cannot reach the
overflow threshold and φ is monotone after all.

So the two halves of this workspace say the same thing from both sides: for this
system one route is undefined and the other is not.
