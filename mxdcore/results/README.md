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

**Superseded, and the previous version was wrong in both directions.** The table
that stood here until 2026-09-09 said this crate was "3–6× faster" than MEDDLY.
It was not a like-for-like measurement:

- The MEDDLY column came from `MDDMinsol/scripts/boundary_nonmonotone.jl`, whose
  timed block also derives the level sets from φ (`sys.phi >= j`) and counts nodes
  twice per level. The Rust column did neither inside its clock. MEDDLY was being
  charged for work the Rust side was not doing.
- The Rust column predated the optimisations in `#13` and was never refreshed.

`tools/bench_meddly.jl` and `examples/bench_boundary.rs` replace it. Both time the
same thing: level sets built beforehand, then every level's boundary in both
directions plus its cardinality. Raw data in `bench_meddly.csv` / `bench_rust.csv`.

### The comparison has to be decomposed

Three separate effects, and merging them into one ratio is what produced the old
claim. Both phases are therefore measured **two ways on the Rust side**: once with
the algorithm MEDDLY is driven with, once with this crate's.

| | |
|---|---|
| relation, `union` | a singleton per (component, step), unioned in — what `MDDMinsol` does |
| relation, `chain` | built directly, one node per component |
| boundary, `product` | `R ∩ cross(L, U)`, materialising the product — what `MDDMinsol` does |
| boundary, `fused` | a three-way recursion that never builds the product |

`dec ∪ restart`, MEDDLY taking the faster of its two protocols (one process per
condition, or all conditions in one process) so the comparison is conservative:

| n | states | MEDDLY | rust, same algorithm | engine | rust, best | net |
|---|---|---|---|---|---|---|
| **relation** | | | | | | |
| 8 | 3 | 34 µs | 22 µs | 1.5× | 2 µs | 15.5× |
| 22 | 3 | 99 µs | 159 µs | **MEDDLY 1.6× faster** | 7 µs | 15.0× |
| 60 | 3 | 298 µs | 939 µs | **MEDDLY 3.1× faster** | 20 µs | 14.6× |
| 100 | 3 | 509 µs | 2498 µs | **MEDDLY 4.9× faster** | 24 µs | 20.8× |
| 16 | 5 | 168 µs | 186 µs | **MEDDLY 1.1× faster** | 7 µs | 24.2× |
| 60 | 5 | 678 µs | 2683 µs | **MEDDLY 4.0× faster** | 25 µs | 26.6× |
| **boundary** | | | | | | |
| 8 | 3 | 58 µs | 101 µs | **MEDDLY 1.7× faster** | 50 µs | 1.2× |
| 22 | 3 | 212 µs | 495 µs | **MEDDLY 2.3× faster** | 187 µs | 1.1× |
| 60 | 3 | 594 µs | 1208 µs | **MEDDLY 2.0× faster** | 457 µs | 1.3× |
| 100 | 3 | 1025 µs | 2276 µs | **MEDDLY 2.2× faster** | 814 µs | 1.3× |
| 16 | 5 | 231 µs | 418 µs | **MEDDLY 1.8× faster** | 158 µs | 1.5× |
| 60 | 5 | 895 µs | 1870 µs | **MEDDLY 2.1× faster** | 722 µs | 1.2× |

### What it says

**Run the same algorithm on both and MEDDLY is faster** — about 2× on the
boundary throughout, and 1.5–5× on relation construction, the gap widening with
`n`. That is the engine comparison, and it does not favour this crate.

**The end-to-end win is algorithmic, not engine.** Relation construction is
15–27× faster here because it builds a chain instead of unioning singletons, and
the boundary is 1.1–1.5× faster because it never materialises the product. Both
are caller-side choices: the chain is available to a MEDDLY user immediately, and
the fused boundary would be too if MEDDLY grew an operation for it (it has
`CROSS`, `INTERSECTION` and the images, but nothing that fuses them).

So: **"the Rust pipeline as written beats the Julia pipeline as written, by 1.1–1.5×
on the boundary and 15–27× on relation construction, while the C++ engine
underneath MEDDLY is about twice as fast as this one."** Anything shorter than
that is misleading.

### Caveats that still apply

- Sub-millisecond measurements. Read the ratios as an order of magnitude.
- These operations are **memoized**, so timing the same system repeatedly measures
  the cache. Every number above is from a cold run — the harness rebuilds the
  system for each timed pass. An earlier draft did not, and reported the fused
  boundary as *slower* than the product one.
- The two use different relation representations (`n × n` fused blocks here,
  interleaved unprimed/primed in MEDDLY), so part of the engine gap is
  representation rather than implementation quality.
- φ construction is not compared. MEDDLY builds it as an MTMDD and thresholds;
  this crate builds the level sets directly. Different work.

### Correctness across the whole grid

Cardinalities agree on **all 45 `(n, states, relation)` conditions** — `n` up to
100, three to five states per component, three relation families. That is a wider
check than the 42-row and 104-row agreements recorded above, and it is the part of
this comparison that is not sensitive to how anything was measured.

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
