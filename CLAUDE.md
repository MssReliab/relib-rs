# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

`relib-rs` (formerly `rust-dd`) is a Cargo workspace of decision-diagram libraries (BDD, ZDD, MDD, MTMDD, MTMDD2) and reliability-analysis layers built on top of them. The lower crates implement the data structures; the upper crates (`bss`, `mss`) apply them to binary-state and multi-state system reliability (probability, path/cut enumeration, minimal solutions, k-of-n, counting).

Repo: `MssReliab/relib-rs` (the org is split by research area — `SwReliab` for software reliability, `MssReliab` for MSS). **The local checkout directory is still named `rust-dd/`; that name is historical.** The crates are published to crates.io as `relib-*` and the `relibmss` Python package consumes the **published** crates, not this checkout — see "Publishing" below.

## Commands

```bash
cargo build                         # build the whole workspace
cargo test                          # run all tests (unit tests + tests/ integration tests)
cargo test -p relib-bdd             # test a single crate
cargo test -p relib-mss --test test_mss  # run one integration test file (mss/tests/test_mss.rs)
cargo test -p relib-mxd             # the MxD engine (9 integration files)
cargo test -p relib-mss --test test_meddly_minsol  # minsol vs MEDDLY (fixture-driven)
cargo run --release -p relib-mxd --example bench_boundary  # where the boundary time goes
cargo run --release -p relib-mxd --example boundary_nonmonotone  # the non-monotone case study
cargo test -p relib-bdd bdd::tests  # run an inline #[cfg(test)] module
cargo test some_test_name           # run a single test by name across the workspace
cargo test --doc                    # run doctests (the rpn grammar examples live here)
```

Two traps here:

- **`-p` takes the crates.io package name (`relib-bdd`), not the lib/import name
  (`bddcore`).** `cargo test -p bddcore` fails with "package ID specification `bddcore`
  did not match any packages" — the two names differ on purpose (see Publishing below).
- **`--test` is required to run one integration file.** Without it the argument is a test
  *name* filter, so `cargo test -p relib-mss test_mss` reports `ok` having run **nothing**
  ("0 passed; 10 filtered out") — a silent no-op that looks like a pass.

Tests live both inline (`#[cfg(test)] mod tests` inside `src/*.rs`) and as integration tests under each crate's `tests/` directory.

## CI

`.github/workflows/ci.yml`, added 2026-09-09 — before that there was no `.github`
directory at all and PRs merged on locally-run tests alone. Two jobs on `ubuntu-latest`,
using the runner's preinstalled toolchain:

- **test**: `build`, `test --no-fail-fast`, `test --doc`, `build --examples`.
- **lint**: the same build under `RUSTFLAGS=-D warnings`, examples likewise, and
  `cargo doc --no-deps` under `RUSTDOCFLAGS=-D warnings`.

`--locked` throughout, since `Cargo.lock` is tracked and this also catches a lockfile left
behind by a manifest change. No Julia or MEDDLY toolchain is needed — both cross-check
fixtures are committed and pulled in with `include_str!`.

Two deliberate gaps: **warnings are denied only for code that ships** (test code carries
~30 unused-binding warnings and clearing them belongs in its own change), and there is **no
MSRV job** even though `rust-version = "1.75"` is declared, because a 1.75 build was never
verified and a job that goes red on its first run is worse than no job.

Note the triggers are `push: [main]` and `pull_request`, so **pushing a topic branch runs
nothing until a PR exists**.

## Crate layout and dependency direction

Dependencies flow strictly upward; lower crates never depend on higher ones.

- **`common`** — shared primitives: type aliases (`NodeId`, `HeaderId`, `Level`, `OperationId`), the `BddHashMap`/`BddHashSet` aliases (std hashmaps with a `wyhash` hasher), the core traits (`Terminal`, `NonTerminal`, `NodeHeader`, `DDForest`, `Dot`). Every crate re-exports through a `prelude` module — `use common::prelude::*` is the standard entry point.
- **`bddcore`** — Binary Decision Diagrams (`BddManager`) and Zero-suppressed BDDs (`ZddManager`). Each has `_ops` (apply/operation logic), `_dot` (Graphviz output), and a `_stack` variant.
- **`mddcore`** — Multi-valued DDs: `MddManager` (boolean MDD), `MtMddManager<V>` (multi-terminal, value-carrying), and `MtMdd2Manager<V>` which **composes** an `MddManager` (boolean part) and an `MtMddManager<V>` (value part) into one structure; its `Node` enum tags a node as `Bool(NodeId)` or `Value(NodeId)`.
- **`mxdcore`** — Matrix Decision Diagrams (`MxdManager`): **relations** over multi-valued state vectors, plus the sets they act on, in one arena and one level space. Unlike MEDDLY, a variable occupies **one** level carrying an `n × n` block (child `a*n+b` = the transition `from=a, to=b`) rather than an interleaved unprimed/primed pair — see the design note below. Sets are fully reduced, relations **identity reduced**. Ops: `and`/`or`/`not`/`setdiff` per kind, `cross`, `transpose`, `post_image`, `pre_image`, `boundary`. Depends only on `common`; nothing depends on it, and it is **not** exposed to `relibmss`. **Most callers should use `mxdcore::analysis`** rather than the manager: `System` owns the forest and builds, and the `StateSet` / `Transitions` handles carry the operations — the same division, and the same `Rc`/`Weak` gc-rooted handles, as `bss` and `mss`.
- **`bss`** — Binary State System reliability over BDDs. Public `BddMgr`/`BddNode` wrapper plus modules: `bdd_prob`, `bdd_path`, `bdd_minsol`, `bdd_count`, `bdd_kofn`.
- **`mss`** — Multi-State System reliability over MTMDD2. Mirrors the `bss` 3-file layout: `mdd.rs` (`MddMgr<V>`/`MddNode<V>` structure-function wrapper), `mss.rs` (`MssMgr<V>` — owns `MddMgr`+`ZmddMgr`, provides `minpath` returning a genuine `ZmddNode`), `zmdd.rs` (`ZmddMgr<V>`/`ZmddNode<V>` set families). Plus `mdd_prob`, `mdd_path`, `mdd_minsol`, `mdd_count`, `zmdd_convert` (private).

## Publishing (crates.io)

**Five of the six** crates are live on crates.io (first release 0.4.0, 2026-07-16), published
under the `relib-` prefix (the workspace is the Rust engine behind the
[`relibmss`](https://github.com/MssReliab/relibmss) Python package). **Latest published:
0.14.1** (2026-08-28; the five published crates move in lockstep). Recent milestones: 0.8.0 BSS dual/mincut,
0.9.x minsol non-minimal fix + genuine ZDD set algebra, 0.10.0 ZMDD set algebra + `MssMgr`,
0.11.0 MSS Birnbaum `bmeas`, 0.12.0 direct MSS `mincut` (maxsol). See the session log for
per-version details. The crates.io **package** name differs from the **lib** name so `use`
paths stay unchanged:

| crates.io `[package] name` | `[lib] name` (`use ...`) | dir |
|---|---|---|
| `relib-common` | `common`  | `common/` |
| `relib-bdd`    | `bddcore` | `bddcore/` |
| `relib-mdd`    | `mddcore` | `mddcore/` |
| `relib-bss`    | `bss`     | `bss/` |
| `relib-mss`    | `mss`     | `mss/` |
| `relib-mxd`    | `mxdcore` | `mxdcore/` |

Shared metadata lives in `[workspace.package]` (root `Cargo.toml`): version, MIT license,
repository, rust-version. Path deps carry `version` + `package` so publishing works, e.g.
`common = { path = "../common", version = "0.5.0", package = "relib-common" }`. Publish in
dependency order: `relib-common` → `relib-bdd`/`relib-mdd` → `relib-bss`/`relib-mss`.
Further DD families (SDD, …) follow the same `relib-<family>` scheme.

**`relib-mxd` is published too, but goes last.** It was held back behind
`publish = false` while its API moved; the `analysis` module is what settled that, since
it is the surface callers are meant to use. Its only real dependency is `relib-common`,
but its `dev-dependency` on `relib-mss` (for the bridging-theorem check) has to resolve
on crates.io once it is published, so the order is
`relib-common` → `relib-bdd` → `relib-mdd` → `relib-bss` → `relib-mss` → **`relib-mxd`**.
A dev-dependency needs a `version` alongside its `path` for exactly this reason — without
one, `cargo publish` rejects the manifest.

### Cutting a new release

Now that 0.4.0 is out, a published version is **immutable** — its metadata, docs, and the
files inside the `.crate` can never be changed, only superseded. So before any publish:

- Bump `[workspace.package] version` (all six move together) and the `version` in the
  path deps.
- Add a **CHANGELOG entry per crate**. The CHANGELOGs ship inside the `.crate`, so a stale
  one is what users read.
- `cargo package --list -p <name>` to see what actually ships. cargo packages **only
  git-tracked files** — a new file that isn't `git add`ed is silently absent (this is how
  the per-crate `LICENSE` was missed in the 0.4.0 prep).
- `cargo doc --no-deps --workspace` and `cargo test --doc` — docs.rs renders the rustdoc,
  **not** the README, and it is frozen per version.
- `cargo publish -p <name> --dry-run`. Upper crates fail dry-run with "no matching package
  named relib-common" until the lower ones are actually published — that's the expected
  dry-run limitation, not a defect.

Publishing needs `cargo login` **and** a verified email on the crates.io account.
Pass the token via stdin (`cargo login`, no argument) — an inline `cargo login <token>`
leaks it into shell history.

## Core architectural pattern

Every DD manager is a **forest/arena**, not a tree of heap-allocated nodes:

- Nodes and headers are stored in `Vec`s on the manager; everything else holds `NodeId`/`HeaderId` indices into those vectors. There are no `Rc`/`Box` node graphs inside the core crates.
- A **unique table** (`utable`) maps a node's structural key (header + children ids) to its `NodeId`, guaranteeing canonical, shared (hash-consed) nodes — identical subgraphs are never duplicated.
- An **operation cache** memoizes apply results so operations run in time proportional to the product of operand sizes. Since 0.5.0 this is the shared **direct-mapped, lossy `ComputeCache`** (`common::compute_cache`, CUDD-style: fixed `[k0,k1,k2,val]` array, overwrite on collision) rather than a growing `HashMap` — a miss only recomputes, so lossiness is safe; `gc` drops entries touching reclaimed slots via `retain_live` (op-keyed) / `retain_live3` (all three key words are node ids). Native `ite` (BDD, boolean MDD, and value-side MtMdd2) each carry their **own** ternary `ite`/`vite` cache keyed on `(f,g,h)`.
- Commutative ops (`and`/`or`/`xor`, and MTMDD `add`/`mul`/`min`/`max`) canonicalize their operand order (`if f > g { swap }`) before the cache key, so `op(a,b)` and `op(b,a)` share an entry.
- Terminals are special-cased (e.g. BDD `Node::Zero`/`One`/`Undet`); non-terminals reference a shared `NodeHeader` (carrying level, label, edge count) by `HeaderId`.
- Managers implement the `DDForest` trait (`get_node`, `get_header`, `level`, …) so generic algorithms (dot output, counting) work across DD types.

The `bss`/`mss` wrappers exist because the core managers are arena-based and require a `&mut` manager for every operation. They wrap the manager in `Rc<RefCell<...>>` and hand out `BddNode`/`MddNode` handles that hold a `Weak` back-reference plus a `NodeId`, giving an ergonomic value-style API (operator overloading via `Add`/`Sub`/`Mul`) on top of the arena. This is the layer to use/extend for user-facing reliability computations.

## Conventions

- New operations on a DD type go in that type's `_ops.rs` module and are dispatched through its operation enum + cache; mirror the existing `and`/`or`/`apply` implementations rather than recursing over nodes directly, so hash-consing and caching are preserved.
- Graphviz/visualization code stays in the `_dot.rs` modules behind the `Dot` trait.
- When adding a public item, also re-export it from the crate's `prelude`.

### Docs go in the same commit as the change (no lagging)

Docs here have repeatedly lagged the code. When a commit changes a **public API or
behavior**, update every surface below **in that same commit** — treat a diff that touches
`bss/`/`mss` public items but none of these as incomplete:

1. **rustdoc** — the `///` on the changed item **and** the crate-level `//!` in `src/lib.rs`
   (its one-line feature list). docs.rs renders rustdoc, not the README, and freezes per version.
2. **`<crate>/CHANGELOG.md`** — an entry under the (bumped) version; it ships inside the
   `.crate`. Lockstep crates each get at least a "lockstep, no functional change" line.
3. **`<crate>/README.md`** — only if the one-line feature summary changed.
4. **`docs/developer-guide.md`** — the analysis-features list (§3.5) and the method tables (§8).
5. **Version bump** — `[workspace.package] version` + the `version=` in every path dep (all six move together).

`CLAUDE.md` (untracked) is updated separately as the session log. Quick self-check before
committing an API change: `git diff --name-only` should include a CHANGELOG and, for a
signature/behavior change, `lib.rs` or the item's rustdoc.

## MxD design note (why `mxdcore` does not look like MEDDLY)

MEDDLY gives each variable **two interleaved levels**, unprimed then primed. `mxdcore`
**fuses them**: one node per variable carrying an `n × n` block, child `a*n+b` meaning the
transition `(from = a, to = b)`.

- **Cost.** Target sub-graphs can no longer be shared across different source values, and
  the unique-table key grows from `n` to `n²` words. Acceptable because `n` is a
  component's state count (2–5 in practice).
- **Benefit, and the reason for the choice.** The diagonal becomes **local to one node**,
  so identity reduction is a plain predicate. Under the interleaved layout a primed node
  cannot tell which unprimed edge reached it, so the same rewrite has to happen in the
  parent with hash-consing in between — which was the single riskiest part of the design
  before this was settled.
- **Why this was even available.** The original motivation for a Rust MxD was to compare
  node counts with MEDDLY, and that was retired as an evaluation axis
  (`MDDMinsol/CLAUDE.md`, 2026-07-17). Interleaving had no other strong justification.

**The boundary does not go through `cross`.** `R ∩ (L × U)` is a three-way recursion over
the three operands, not `and_rel(cross(L, U), R)`. The product is dense exactly where the
intersection is sparse — at 60 components of five states `cross` alone was 451 ms of a
454 ms boundary, for a result 8× smaller than the product it built. Its memo is a
`BddHashMap`, not the shared `ComputeCache`, because the key `(lower, upper, rel, level)`
is four words and that table holds three.

**Relation families are built as a chain, not by union.** At each component, either it
takes an admitted step and everything below is the identity, or it holds and something
below stepped — so `degrade`/`repair`/`restart`/`fail` are `k` node creations with no apply
at all. Unioning singletons instead cost ~480× more and left several times the garbage.

Consequences to keep in mind when reading or extending this crate:

- **Relation node counts are not comparable with MEDDLY's**, by design. **Set** node counts
  *are* — sets use the same fully-reduced, one-level-per-variable convention on both sides,
  and they match exactly on the case study (`7n − 16`).
- **A skipped level means opposite things for the two kinds**: don't-care for a set,
  **identity** for a relation. So the `One` terminal read as a relation is the *diagonal*,
  not `Ω × Ω`; complementing cannot bottom out there, and relation operations must carry an
  explicit level (which is why they get one compute table each — the level occupies a key
  word, leaving no room for an op code).
- `Zero`/`One` are shared between the two kinds, so a node alone does not say how to read
  it. That is why the operations are named `_set` / `_rel` rather than overloaded.
- Cardinalities are **`u128`**: a relation lives in `Ω × Ω` and the case studies reach
  `|Ω| = 3^22`, so `u64` would wrap on exactly the systems this crate exists for.

## MEDDLY cross-checks (committed fixtures)

Two of this workspace's results are checked against MEDDLY, the implementation the
research pipeline is being moved off. Both follow the same shape, and a third should
too if one is ever added:

| what | generator | fixture | reference |
|---|---|---|---|
| minsol / MPV (手法A) | `mss/tools/gen_minsol_fixture.jl` | `mss/tests/fixtures/minsol_cases.txt` | `MDDMinsol.all_mpv_minsol` |
| boundary operator (手法B) | `mxdcore/tools/gen_meddly_fixture.jl` | `mxdcore/tests/fixtures/meddly_cases.txt` | `Meddly.jl` + `MDDMinsol` |
| repair/restart boundaries | `mxdcore/tools/gen_repair_reference.jl` | `mxdcore/results/repair_restart_meddly.csv` | `MDDMinsol.repair_relation` |

A third Julia script, `mxdcore/tools/bench_meddly.jl`, is a **timing** harness rather than a
fixture generator, but it verifies every cardinality against the Rust side before any number
is taken — 45 `(n, states, relation)` conditions. Its results live in
`mxdcore/results/bench_meddly.csv`, paired with `bench_rust.csv` from
`examples/bench_boundary.rs`.

**The fixtures are committed**, so the tests run with no Julia or MEDDLY toolchain.
Regenerating needs both, plus — for the MxD one only — a local `libmeddly_c` build, since
`MEDDLY::CROSS` is absent from the published `libmeddly_c_jll`. Each generator's header
carries its exact invocation.

Conventions that make these comparisons mean something; keep them if you extend either:

- **Compare cardinality and membership, never node counts.** The two engines use different
  node layouts on the relation side, so a disagreement there would carry no information.
- **Do not let both sides construct the input from the same formula.** The minsol fixture
  ships φ's whole truth table and the Rust side rebuilds φ from it node by node, so a shared
  misreading of a formula cannot cancel out and let the test pass for the wrong reason.
- **Do not decide membership by walking the reference's diagram.** The MxD fixture intersects
  with an explicit singleton and reads the cardinality instead; walking would re-derive
  MEDDLY's layout and identity reduction inside the generator — the very semantics under
  test — and an error there could agree with an error here and hide both.
- **Use component (x-) order, not level order.** The engines assign levels differently
  (`MDDMinsol` uses `assign_levels(order=:good)`), and the results being compared are
  properties of the function, so they must not depend on it.

`extract_level(j)` is the reader that matches the classical `minimal{x : φ(x) ≥ j}`; plain
`extract([j])` returns the stratum where φ is *exactly* `j`, which is a different question.

## Not part of the workspace build

- **`evmdd_core/`** (package name `evmdd`, edge-valued MDDs) is **not** listed in the root `Cargo.toml` `members`, so `cargo build`/`cargo test` ignore it. It is in-progress (`TODO.md` at the repo root tracks "edge-valued mdd" as unchecked). **It does not build at all today**: `src/` holds only `evplus_mdd.rs` with no `lib.rs`, so `cargo build --manifest-path evmdd_core/Cargo.toml` fails with "either src/lib.rs, src/main.rs, a [lib] section, or [[bin]] section must be present". Reviving it means adding a `lib.rs`, and joining the workspace additionally means `relib-evmdd` as the package name plus the usual metadata and a version-pinned `common` dep.
- **`benches/`** contains legacy benchmark files that reference an old `dd::` crate which no longer exists in this workspace. They are not wired into any crate's `Cargo.toml` and will not compile as-is.

## Session log

### 2026-09-09 (3) — MEDDLY 比較の測り直しと訂正 (PR #14)

**Done** — `main` = `28112ca`.

- **公開していた「3–6× faster」は誤りだった。** 同じ作業を測っていなかった:
  MEDDLY 側の数値は `MDDMinsol` のスクリプト由来で、その計測ブロックは φ からの水準集合
  導出（`sys.phi >= j`）と節点数カウントを含み、Rust 側は含んでいなかった。旧スクリプトを
  実際に走らせると n=22 で 8.3 ms、条件を揃えた測定では同じ計算が 2.0 ms。
  加えて Rust 側の値は #13 より前のままだった（#13 は CSV を再生成したが README を触っていない）。
- **結論が逆転した。同一アルゴリズムでは MEDDLY の方が速い** — 境界で約2倍、関係構築で
  1.5–5倍（n が大きいほど広がる）。Rust が総合で勝つ（境界 1.1–1.5×、関係構築 15–27×）のは
  **呼び出し側のアルゴリズム選択のみ**によるもので、どちらも MEDDLY 利用者にも可能。
  比較表は「エンジン」と「アルゴリズム」に分解して `mxdcore/results/README.md` に掲載。
- **#13 で自分が出した倍率も2つとも誇大だった。** 「451 ms / 50–68×」は水準集合 1125 節点の
  即席プローブの数字で、それを帰した配送システム（411 節点）では約 2.6×。「~480×」は
  温まったフォレストでの測定で、コールドでは ~190×（100 成分5状態）/ 13×（22 成分3状態）。
  利得は本物、公表値が過大。**訂正は元の記述の場所に残してある**（消していない）。
- 新しい測定器: `mxdcore/tools/bench_meddly.jl`（`Meddly.jl` を直接叩く。`MDDMinsol` の
  `distribution_system` は3状態固定なので状態数を振れない）と `examples/bench_boundary.rs`。
  生データは `results/bench_meddly.csv` / `bench_rust.csv`。
- 副産物として**正しさの検証範囲が広がった**: カーディナリティが **45 条件**
  （n ≤ 100、状態数 3–5、関係族3つ）で一致。従来の 42 行 / 104 行より広い。

**Pending** — 前エントリから変わらず（`publish = false` の判断、Julia 実験の移植、
橋渡し定理の数値確認）。`docs/method_b_comparison.md` は Pending から外した —
2026-07-17 に改訂済みで、古かったのは `MDDMinsol/CLAUDE.md` の Pending の方だった。

**Notes for next session**

- **メモ化された演算は繰り返し測定できない。** 同じ系を何度も測ると計算表を測ることになる。
  `bench_boundary.rs` の草稿がまさにそれで、融合境界を直積方式より3倍**遅い**と報告した。
  現在は毎回系を作り直すコールド測定。
- **どの系を測ったかを記録する。** 「50–68×」は系の取り違えだった。ベンチの `build()` が
  パラメータを取る形だと、あとから数字だけ見て別の系のものと混同しやすい。
- **他人の計測スクリプトの計測ブロックを読む。** 何が clock の内側にあるかで倍数が変わる。
  今回は MEDDLY 側だけが水準集合の導出を課金されていた。

### 2026-09-09 (2) — CI, repair/restart, API 整備, 高速化 (PR #8–#13)

**Done** — `main` = `3a4e6f4`, 39 suites green.

- **#8 CI.** There was no `.github` directory; PRs #6 and #7 merged on locally-run tests
  alone. See the CI section above for what runs and what deliberately does not. Also fixed
  the four warnings `relib-mss` already had, without which the lint job could not have been
  green on its first run.
- **#9 repair/restart numbers** — the manuscript's 本命, quantities only the boundary
  operator can produce. Four relations (`dec`, `inc`, `restart`, `dec ∪ restart`), both
  crossing directions, `n = 2..22`. **All 104 rows agree with MEDDLY** over `n = 2..14`.
  `MDDMinsol` exports `repair_relation` but had neither a script nor a test for it, so the
  reference was generated here. The headline: **degrading enters the productive band and
  repairing leaves it** (5,313,616 each way at n=22, against 0 for both in any monotone
  system), and restart overshoots harder than gradual repair.
- **#10 + #12 the `analysis` layer.** `phi`/`level_set`/`relation` had been copied into
  five files; a case study now reads as one. #10 landed it with its own handle scheme,
  #12 moved it onto the `Rc`/`Weak` gc-rooted handles `bss` and `mss` use.
- **#11 cross-forest guards in `bss`/`mss`** — a latent correctness hazard in the public
  Rust API, found while checking whether `analysis` matched the workspace's conventions.
- **#13 performance.** Three changes; see the MxD design note above for the two that are
  design facts rather than tuning.

**Pending**

- `relib-mxd` is still `publish = false`. The API has settled now, so this is a decision
  that can actually be taken rather than deferred.
- The Julia experiments still drive MEDDLY; porting the ORSJ scripts has not started. Both
  methods are cross-checked, so the precondition is met.
- `idea.md` §3's bridging theorem is still 要証明. `sources` is the ∃ projection and the
  theorem is the ∀ condition, so it is now expressible: `MCV(j) = {x ∈ L_j : every
  admissible increment from x crosses}` = `L_j` minus the sources of the non-crossing
  increments. Worth checking numerically against `mincut` on monotone systems.

**Notes for next session**

- **Measure the phases before optimising** — but check *what system* and *what cache
  state* you measured. Both of the first speedup figures published for #13 were artefacts:
  the boundary one was taken on an ad-hoc probe with much larger level sets than the
  distribution system it was attributed to (~530× there, ~2.6× on the real one), and the
  relation one was taken on a warm forest where every node already existed (~480× claimed,
  ~190× cold). The wins are real; the numbers were not. Corrected 2026-09-09.
- **Memoized operations cannot be benchmarked by repetition.** Timing the same system
  repeatedly measures the compute table. A draft of `bench_boundary.rs` did that and
  reported the fused boundary as three times *slower* than the product one.
- **Hash-consing gives a free exactness check.** Building a relation two ways and asserting
  the *same node* is exact equality of meaning, not a sample — much stronger than comparing
  cardinalities, and it is what verified the chain construction.
- **Automated patches under-apply silently.** In #11 a regex hit `BddMgr::and` instead of
  `BddNode::and` (same name, manager first in the file), leaving four methods unguarded;
  only the `should_panic` tests caught it. In #9 two entries of a hand-typed reference table
  were numbers appearing nowhere in the data; the test caught that too, but by disagreeing
  rather than by accidentally matching.
- **`live_nodes` is not a result.** It counts the whole arena, so it moves whenever a
  construction changes even though nothing computed does. It has now moved twice for that
  reason.

### 2026-09-09 (1) — minsol (手法A) cross-checked against MEDDLY, merged as PR #7

**Done**

- `mss/tests/test_meddly_minsol.rs` + fixture + generator (`main` = `daaf2ca`, squash of
  PR #7, +758, **no behaviour change**). Closes the gap PR #6 left: 手法B had been checked
  against MEDDLY, 手法A had not — and 手法A is the half where the two implementations
  *deliberately* differ, this crate reading a skipped level as state 0 (ZMDD-flavoured,
  `Undet` terminal) against MEDDLY's fully-reduced DONT_CARE. That is why `MDDMinsol`
  rewrote its own minsol rather than porting `mdd_minsol.rs`. Both had been brute-force
  verified separately; neither against the other.
- 14 systems (series, parallel, k-of-n, monotonised random), **all levels agree**, coherence
  verdicts included. The baseline case is genuinely exercised — four systems have
  `φ(0,…,0) ≥ 1`, where `MPV(1)` is the all-zero vector alone, and both sides report it.
- Answered in passing: the **coherence check** (`memo.md`'s local invariant,
  `min(c[i-1], c[i]) == c[i-1]` with O(1) id compare and early abort) and the **multi-valued
  minsol skip handling** were both already implemented here — in `mss`, long before this
  session, and *not* in `mxdcore`, which only does 手法B. What was missing was never the
  implementation, only the evidence that it and MEDDLY agree.

**Pending**

- Unchanged from the previous entry: `relib-mxd` is still `publish = false`, and the repo
  still has **no CI** (PR #7 gated on nothing but locally-run tests, like #6).
- The Julia experiments still drive MEDDLY. Porting the ORSJ scripts is the actual 一本化
  step and has not started — but with both methods now cross-checked, its precondition is met.

**Notes for next session**

- **The existing brute-force MSS tests do not cover level gaps.** Mutating the level-gap arm
  of `vwithout` to descend a non-zero edge fails *only* the new cross-check; every existing
  suite passes it. Their hand-picked structure functions apparently never produce the gaps
  that the fixture's random monotone systems do. Treat "brute-forced" as "brute-forced over
  the inputs someone chose", not as coverage.
- Both MEDDLY cross-checks now follow one convention — see the section above before adding
  a third or changing either.

### 2026-09-08 — new crate `relib-mxd` (MxD), merged as PR #6

**Done**

- **`mxdcore/` from nothing to a working MxD engine** (`main` = `45bd677`, squash-merged
  from `feat/mxdcore`, 27 files / +6198). Purpose: run the multi-state boundary operator
  `B = R ∩ (L × U)` without the Julia + MEDDLY leg — the 「relib-rs 一本化」 goal that
  `MDDMinsol/CLAUDE.md` deferred on 2026-07-17. That deferral's reasoning also *shaped*
  this: node counts were retired as an evaluation axis, which removed the only strong
  argument for MEDDLY's interleaved layout. See the MxD design note above.
- Built in stages, each verified before the next: forest skeleton + enumeration →
  boolean ops → `cross`/`transpose`/`boundary` → `post_image`/`pre_image` →
  **then** identity reduction as a one-rule diff, so the whole existing suite acted as a
  differential oracle for it. All of it passed unchanged.
- Verification: explicit-enumeration oracles (~2000 randomised cases over eight shapes,
  1–4 variables, domains 1–4), a committed MEDDLY fixture (`tests/fixtures/`, generated by
  `tools/gen_meddly_fixture.jl`), and the non-monotone `distribution_system(n)` case study.
- **Agrees with MEDDLY exactly on all 42 `(n, j)` rows** of that case study, n = 2..22.
  Three rows looked off by a few units and were not — the Julia script writes `%.6g`, so its
  CSV keeps six significant digits; re-running at full precision confirmed exact agreement.
- ~~Timing on the same M4: 3–6× faster end-to-end than Julia+MEDDLY.~~ **Superseded
  2026-09-09.** That measurement was not like-for-like: the MEDDLY clock also covered
  deriving the level sets from φ and counting nodes, which the Rust clock did not. Re-done
  with both sides doing the same work, **MEDDLY is the faster engine** — about 2× on the
  boundary, 1.5–5× on relation construction. The Rust pipeline still wins end to end
  (1.1–1.5× boundary, 15–27× relation) but through caller-side algorithm choices, not
  engine speed. `mxdcore/results/README.md` has the decomposition.
- Also this session: pulled `relib-rs` and shipped the `lte` fix it carried as
  **relib-\* 0.14.1** (crates.io, all five) and **relibmss 0.21.1** (PyPI). `mddcore`'s
  `lte` tested `veq(f,g) == one()`, but `veq` returns a *diagram*, not a boolean, so the
  branch never fired and `x <= y` silently computed `x < y`. Now `not(vlt(g,f))`.

**Pending**

- `relib-mxd` is `publish = false`. Decide when the API has settled; removing that line puts
  it in the lockstep release (publish after `relib-common`).
- **This repository has no CI.** PR #6 ran nothing automatically; the 33 suites were only
  ever run locally. A `cargo test --workspace` workflow would make PRs actually gate.
- `mxdcore` has no `mss` bridge, so an `MddNode` cannot be handed to it. Deliberate for now
  (the level space is shared, so a `zmdd_convert`-style structural copy is the route when
  it is wanted).

**Notes for next session**

- **Mutation-test anything load-bearing here.** It repeatedly found what the tests did not:
  a four-arm match in the relation ops was three-quarters *unreachable* (the commutative
  operand swap sorts terminals into `f`), and the `full_relation` memo's gc clearing was
  covered by no test at all until one was added. Both were rewritten/covered, not left.
- Symmetric test data hides orientation bugs. The identity and sentinel tests survive a
  transposed `a*n+b` index; only the asymmetric oracles and `post ∘ transpose == pre` catch
  it. Keep an asymmetric case in any new suite.
- The `Src`/`Dst` types exist to make MEDDLY's `-1`/`-2` footgun unwritable — an invariant
  component is `(Src::Any, Dst::Same)`. `from_meddly_sentinels` replays raw Julia vectors.

### 2026-07-22 — direct MSS `mincut` (maxsol, no dual MDD) (`relib-*` 0.12.0, released)

**Done**
- BSS gets `mincut` via `minpath(dual(φ))` (BDD dual is cheap). The **MSS dual is expensive**
  (reverse every variable's `M_i` edges + remap all terminal values `v→K−1−v`, can blow up), so
  (user's call) `mincut` is implemented **directly, never materializing the dual**.
- **`mdd_minsol::maxsol`** = exact top-baseline mirror of `minsol`: same coherence check, baseline
  = top edge `M-1`, subtract via the *higher* cofactor `c_{i+1}`, `upwithout` recurses the top edge.
  Bool forest: the `{∅}` member sits at φ's **failure (Zero)** leaf (terminals swap vs bminsol).
- **`MssMgr::mincut(&node) -> Option<ZmddNode>`** (`mss.rs`), mirrors `minpath` but calls
  `maxsol` + `zmdd.convert_rev`. **`zmdd_convert::to_zmdd` gained `reverse`** (reverses each node's
  edge order, putting the baseline back on edge 0 in "levels below max" coords); the bool member
  maps to `value(0)` when reversed (φ's failure value, keeping φ's own scale). **`ZmddNode` carries
  a `reverse` flag**; the `ZmddPath` reader undoes the coordinate (`state = edge_num-1 - i`).
- **Semantics (verified by brute force)**: `mincut(φ).extract([v])` = maximal elements of
  `{x : φ(x) ≤ v}`, sparse with **unlisted = max state**; terminal label = φ's own value (user chose
  "keep original v", not the dual's `K−1−v`). So a boolean fault-tree failure is read `extract([0])`.
  The all-max "empty cut" may appear at the top level (a mirror artifact; filtered in the test).
- Test `test_mincut_matches_bruteforce` (value forest `max(min(x,y),z)`, bool forest, non-coherent→
  None). relib `cargo test` (all suites) + doctests green, `cargo doc` warning-0. Version bumped
  **0.11.0 → 0.12.0** (5-crate lockstep). Folded in the `bmeas` interval-docs `## Unreleased` note.
- relibmss follow (v0.19.0, built via path-deps, **pytest 49 green**): `src/mdd.rs` `_mincut`,
  `mdd.py` `MddNode.mincut()`, `mss.py` `MSS.mincut`, README cut example, CHANGELOG.

**Released**
- All 5 crates **published to crates.io at 0.12.0** (order common→bdd/mdd→bss/mss),
  commit `1f8fed8` on `main`. relibmss followed with **v0.19.0** on PyPI (CI green, clean install
  verified `mincut`).
- **Independent correctness check** (beyond the in-repo test): a brute-force verifier compared
  `mincut().extract` against the definition (maximal elements of `{x: φ(x) ≤ v}`) on 7 structure
  functions — series/parallel/mixed, **asymmetric state counts** (X:2,Y:3,Z:4), a bool forest, and
  a 4-variable case — **all matched**. `mincut` is confirmed correct.

### 2026-07-21 — MSS Birnbaum importance (`bmeas`) via backward-diff (`relib-*` 0.11.0, released)

**Done**
- **New `mss::MddNode::bmeas`** (`mdd_prob::bmeas` + `vbmeas`/`bbmeas`) — multi-state Birnbaum
  importance by one **backward-differentiation** (reverse-mode gradient) pass, the n-ary
  generalization of `bss::bdd_prob::bmeas`. Dispatches value/bool forest like `prob`; a
  post-order-reversed topological walk propagates the adjoint weight `w_f` (prob of reaching
  `f`, `w_root=1`, `w_{edge_j} += w_f·p_{i,j}`) and accumulates the importance at each node.
- **Key correction found by the test** (`test_bmeas_matches_pinned_prob`): the raw partial
  `∂P/∂p_{i,j}` is NOT `P(φ∈ss|x_i=j)` on a **reduced** diagram — variables skipped on a path
  are dropped, so e.g. `∂P/∂p_{y,0}` for `max(min(x,y),z)` gave 0.25 vs the true `P(φ|y=0)=0.75`.
  The correct, skip-safe, BSS-faithful quantity is the **adjacent-state difference**
  `D_{i,j}=P(φ∈ss|x_i=j)−P(φ∈ss|x_i=j−1)` (length `M_i−1`; skipped paths cancel, exactly like
  BSS's `p1−p0`). BSS is the binary case. Implemented as `w_f·(prob(edge_j)−prob(edge_{j-1}))`.
- Exact oracle test: `bmeas[x][d] == prob(pin x=d+1) − prob(pin x=d)` for every (var,transition)
  on the value forest (`max(min(x,y),z)`) and bool forest (`[x≥1]&[y≥2]`). relib `cargo test`
  21 suites + doctests green, `cargo doc` warning-0.
- Docs: rustdoc on `MddNode::bmeas` + `mdd_prob::bmeas`, `mss` `lib.rs` `//!`, CHANGELOGs
  (mss = feature; other 4 lockstep), developer-guide §3.5 + §8 MSS table.
- **Scope: engine only** (user: engine（Rust）のみ). Version bumped **0.10.0 → 0.11.0** (5-crate
  lockstep, additive/minor).

**Released**
- All 5 crates **published to crates.io at 0.11.0** (order common→bdd/mdd→bss/mss),
  commit `84e2d4e` on `main`. relibmss followed with **v0.18.0** (Python `bmeas`/`bmeas_interval`)
  and **v0.18.1** (interval-semantics docs). Interval version turned out trivial (the generic `T`
  already flows `Interval`), so it shipped in the same relibmss round.

### 2026-07-21 — native ZMDD minimal set + mss reorg (`relib-*` 0.10.0, released)

**Done**
- **ZMDD theory locked with the domain expert** (see the plan file / memory): a ZMDD denotes
  `f: R → 2^S` — sparse-vector families stratified by terminal LABEL (`Terminal(v)` = label,
  `Undet` = empty, `X=0` = not-in-vector). Set ops are label-wise (like BDD apply); intersect
  and setdiff preserve the disjoint/partition form (union does NOT → deferred). Reductions:
  zero-suppression `if children[1..].all(==undet) { children[0] }` + unique table; **NOT** the
  full-reduction "merge if all equal".
- **`mddcore/src/zmdd.rs`**: `ZmddManager<V>` (multi-terminal, `Undet`, zero-suppression
  create_node), mirroring `MtMddManager` but with the ZMDD reduction. **`zmdd_ops.rs`**:
  `intersect`/`setdiff` — the level-mismatch arm descends the reference's 0-edge (same principle
  as the minsol `without` fix), so partition is preserved.
- **`mss`**: `zmdd_convert.rs` (`pub(crate)` fake-ZMDD `MtMdd2` → `ZmddManager`, structural copy
  that's family-preserving), `zmdd.rs` (`ZmddMgr`/`ZmddNode` + `from_minsol` + reader
  `ZmddSetPath` [0-edge records nothing] + `count`). `minpath` unchanged (returns `MddNode`);
  wrap with `ZmddMgr::from_minsol` for set algebra.
- Verified: `mss/tests::test_zmdd_intersect_setdiff` (max(min(x,y),z) ∩ min(x,y) = {x=1,y=1},
  {x=2,y=2}; setdiff = {z=1},{z=2}). relib 21 suites green. relibmss `MddNode.minpath()` →
  `ZmddNode` with `&`/`-`; pytest 47 green.
- **This is the MINIMAL set** (user: 当面必要な実装だけ). Everything else is TODO in the plan file:
  union, arithmetic apply / semiring, dominance, minimalization, threshold, relabel, standalone
  `ms.ZMDD()`, header dedup across conversions, ZMDD `dot`, the `∏_r 2^S` / stratification
  algebra writeup.

**Follow-up (same unpublished 0.10.0) — mss reorg mirroring bss + fake-ZMDD reader removal:**
- `git mv mss/src/mss.rs → mdd.rs` (`MddMgr`/`MddNode`); **new `mss/src/mss.rs` = `MssMgr<V>`**
  (owns `MddMgr`+`ZmddMgr`, `minpath(&node) -> Option<ZmddNode>`; delegates the MDD building API).
  Now bss↔mss are symmetric: `bdd.rs`↔`mdd.rs`, `bss.rs`↔`mss.rs`, `zdd.rs`↔`zmdd.rs`.
- **Breaking**: `minpath` moved off `MddNode` onto `MssMgr` and returns a genuine `ZmddNode`
  (was `MddNode` + `ZmddMgr::from_minsol`). Removed the fake-ZMDD readers `MddNode::{zmdd_extract,
  zmdd_count}`, `mdd_path::ZMddPath`, `mdd_count::{zmdd_count,vzmdd_count,bzmdd_count}` (kept
  `mdd_extract`/`mdd_count`). `ZmddSetPath` → `ZmddPath`. `ZmddMgr::from_minsol` →
  `pub(crate) convert(src_rc, node)`.
- Verified: relib `cargo test` (mss suite 4 + 21 workspace) green, `cargo doc` warning-0.
  relibmss `PyMddMgr(MssMgr<i32>)`, `MddNode.count/extract` → mdd-only (default `type='mdd'`);
  built via path-deps into a fresh **uv Python 3.12** venv (homebrew 3.14 has a broken pyexpat),
  `pytest` 47 green; deps reverted to `0.10` crates.io pins.

**Released**
- All 5 crates **published to crates.io at 0.10.0** (order common→bdd/mdd→bss/mss),
  commit `068df50` on `main`. relibmss followed with **v0.17.0** on PyPI. Clean install
  from PyPI verified the genuine `ZmddNode` minpath + `&`/`-` set algebra.
- **Post-release (relibmss docs only, no version bump)**: rewrote the MSS `ZmddNode`
  set-algebra example in relibmss `README.md` to be self-contained with annotated verified
  output (`max(min(X,Y),Z)` intersect/setdiff `min(X,Y)`), mirroring the BSS `ZddNode`
  example; logged it under a new `# Unreleased` section in relibmss `CHANGELOG.md`. Verified
  the README renders correctly on GitHub (GFM API). relibmss commits `a4f2208` (README) /
  `d2c2275` (CHANGELOG) on `main`.

### 2026-07-21 — MSS minsol non-minimal fix (`relib-*` 0.9.1, published)

**Done / Released**
- Found the BSS minsol bug's twin in **MSS**: `mdd_minsol::{vwithout,bwithout}` `(NonTerminal f,
  Terminal/One g)` expanded every branch of the reference cofactor `f` when the minsol family
  `g` was a terminal, fabricating non-minimal path/cut vectors (e.g. `max(min(x,y),z)` gained
  `(y=1,z=2)`; boolean `x&y|z` gained `{y,z}`). **Root cause & fix (user-derived):** g terminal ⟹
  the candidate vector has every remaining variable at 0, so recurse into f's **zero branch**
  only (`fnode.edge(0)`), same as the existing `level(f)>level(g)` arm. Applied to both `vwithout`
  and `bwithout`.
- **Unified BSS too:** rewrote the `bdd_minsol::without` `(One, NonTerminal) => f` shortcut as the
  same zero-branch recursion `without(f, g.edge(0))` (behaviorally identical — non-constant
  monotone g has g(∅)=0; exhaustive n≤4 test still passes).
- **Verified exhaustively** (Python, brute-force minimal path vectors via `zmdd` extract): all
  monotone functions of n=3/K=2 (boolean) and n=2/K=3 (value, incl. nonzero-baseline cofactors)
  match; multi-var value battery (`Max(Min,Z)` etc.) redundant=0. Rust regression
  `mss/tests::test_minpath_no_spurious_vectors`. relib 21 suites + pytest 45 green.
- **Published `relib-* 0.9.1`** (commit `55cabf2`); relibmss **v0.16.1** on PyPI (5 files), clean
  install verified (`Max(Min(X,Y),Z)` → exactly 4 minimal vectors).
- **MSS minsol/extract contract (for the MDD/ZMDD phase):** minsol result is read with the
  **`zmdd` extract** (`type='zmdd'`), values `[1..K-1]`; unlisted components = 0. `type='mdd'`
  gives full assignments and is the WRONG reader for minsol families. MPV oracle = ∪ over levels j
  of {minimal x : φ(x) ≥ j}.

### 2026-07-21 — minsol bug fix + BssMgr / ZDD set algebra (`relib-*` 0.9.0, published)

**Released:** all five crates published to crates.io at **0.9.0** (order common→bdd/mdd→bss/mss),
commit `88ad27a` on `main`. relibmss followed with **v0.16.0** on PyPI. This closes the BDD/ZDD
phase; MDD/ZMDD (multi-state genuine ZMDD + set algebra) is the next version (relib 0.10.0 /
relibmss 0.17.0) — **check `mss/mdd_minsol` for the same `without` non-minimal bug then.**


**Done**
- **Correctness bug fixed in `bdd_minsol::without`.** The `(One, NonTerminal)` case recursed
  over the operand and fabricated non-minimal sets — e.g. `minpath(x&y|z)` gave
  `{x,y},{y,z},{z}` instead of `{x,y},{z}` (simple gates unaffected, so it was latent since
  the first minsol release). Fixed to `=> f` (return the `{∅}` family unchanged; a non-constant
  monotone `g` has `g(∅)=0`). New `test_minpath_exhaustive_brute_force` verifies **every**
  boolean function of n≤3 in-suite (checked to n=4 = 65536 functions during dev).
- **New: ZDD set-family algebra.** New `BssMgr { bdd: BddMgr, zdd: ZddMgr }` owns both forests.
  `minpath`/`mincut` **moved off `BddNode` onto `BssMgr`** and now return a genuine `ZddNode`
  (was a `BddNode` read with ZDD semantics). New `bss/src/zdd.rs` (`ZddMgr`/`ZddNode` mirroring
  `BddMgr`/`BddNode` + `union`/`intersect`/`setdiff`/`product`/`divide`/`count`/`extract`),
  `zdd_convert.rs` (**private** `pub(crate) to_zdd`: fake-ZDD BddManager node → real ZddManager;
  naive structural copy is correct — proof in the file), `zdd_count.rs`, `zdd_path.rs`
  (`ZddSetPath`). `BddNode::dual` stays (pure BDD op). Set ops come from `bddcore::zdd_ops`.
- **Design rationale (see [[design-clean-separation]] memory):** kept the leaky BDD→ZDD
  converter private; composed managers via `BssMgr` rather than embedding a `ZddManager` in
  `BddMgr`; minpath/mincut return the honest representation (a real ZDD). Did NOT build the ZDD
  directly in minsol — `without` entangles φ (BDD) and output in one arena (`id`-equality
  fast-path), so cross-arena would cost more; the O(size) post-pass converter is cost-neutral.
- **Source reorg** (`bss/src`): BDD wrapper `bss.rs` → **`bdd.rs`** (`BddMgr`/`BddNode`);
  `bss_mgr.rs` → **`bss.rs`** (`BssMgr`); `zdd.rs` is the ZDD wrapper. Removed the now-dead
  fake-ZDD readers `BddNode::zdd_count`/`zdd_extract`, `bdd_path::ZddPath`, `bdd_count::zdd_count`
  (superseded by the genuine `ZddNode`). Prelude re-exports unchanged, so `use bss::prelude::*`
  still works.
- **Follow-up (same unreleased 0.9.0):** renamed the ZDD iterator `ZddSetPath` → `ZddPath`
  (parity with `BddPath`); added standalone `ZddMgr` builders `empty`/`base`/`singleton`/
  `from_sets` (element→header map like `BddMgr::defvar`). Exposed in relibmss as `ms.ZDD()`
  (v0.16.0) — a separate forest from minpath/mincut results (mixing raises `ValueError`).
- **Breaking** → 0.8.0→0.9.0 (minor, pre-1.0). CHANGELOGs, rustdoc, developer-guide, bss/README
  updated; `cargo test` 21 suites + doctests green, rustdoc warning-free.

**Pending**
- **Not yet published.** Publish `relib-* 0.9.0` (order common→bdd/mdd→bss/mss), then relibmss
  0.16.0 (deps bumped to 0.9 already). MSS-side ZDD algebra deferred (BSS only this round).

### 2026-07-21 — BSS dual / mincut (`relib-*` 0.8.0, published)

**Done**
- New `bss/src/bdd_dual.rs`: `dual(dd, node, cache)` computes the dual structure function
  `φ^D(x) = ¬φ(¬x)` by an O(size) memoized recursion — at each `NonTerminal` swap the two
  edges and recurse (`create_node(header, dual(hi), dual(lo))`), map `Zero↔One`, `Undet→Undet`.
  Monotonicity-preserving. `pub mod bdd_dual;` + prelude re-export in `bss/src/lib.rs`.
- `bss/src/bss.rs`: `BddNode::dual() -> BddNode` and `BddNode::mincut() -> Option<BddNode>`
  (= `self.dual().minpath()`, the minimal **cut** vectors). `minpath` doc reframed to the
  structure-function view; both are dual (series `x&y`: path `{x,y}`, cut `{x},{y}`; parallel
  reverse). Added `test_dual_and_mincut` to `bss/tests/test_bss.rs` (dual(x&y)==x|y, counts,
  involution, xor→None). `cargo test` 21 suites green; doctests + rustdoc clean.
- Versions bumped **0.7.0 → 0.8.0** (all five lockstep; additive/minor). CHANGELOGs: `relib-bss`
  gets the dual/mincut entry, the other four get lockstep entries (`relib-mss` notes the
  multi-state dual is deferred). `docs/developer-guide.md` updated (minpath/dual/mincut).
- **MSS (multi-state) dual is intentionally deferred** — it needs component state reversal
  (j → M_i−1−j) + output value reversal (v → K−1−v) + maxval precompute + level semantics.
  BSS-only this round (user: 「まずBSSだけでお願いします」).

**Published**
- All five crates published to crates.io at **0.8.0** (order `relib-common`→`-bdd`/`-mdd`→
  `-bss`/`-mss`). Commit `6539ed0` on `main`. relibmss followed with **v0.15.0** (deps bumped
  to `relib-* 0.8`, on PyPI).

### 2026-07-16 — crates.io publishing prep (`relib-*` 0.4.0)

**Done**
- Discussed structure/positioning: rust-dd is the Rust engine behind the `relibmss`
  Python package (PyPI); goal is to publish versioned crates so relibmss can pin them
  instead of a moving git branch. Decided **not** to split engines per DD type.
- Merged `perf/bddcore-hotpath-u32` (u32-narrowing + mark-and-sweep gc) into `main` and
  pushed it; created branch `release/relib-0.4.0`.
- Renamed packages to `relib-*` (kept `[lib]` names → `use` paths unchanged), added
  `[workspace.package]` metadata + `resolver = "2"`, per-crate description/keywords/
  categories/readme, version-pinned path deps, `docs.rs` config.
- Added MIT `LICENSE`, workspace + per-crate READMEs, crate-level + public-type docs for
  `bss`/`mss`, and runnable examples (`bss/examples/bss_reliability.rs`,
  `mss/examples/mss_reliability.rs`).
- Verified: `cargo build` / `cargo test` / `cargo test --doc` pass; both examples run;
  `cargo publish -p relib-common --dry-run` succeeds.
- Committed (`3e1d13b`), pushed, opened PR #3 (base `main`). `CLAUDE.md` intentionally
  left out of that commit.

### 2026-07-16 — org 移設 (`MssReliab/relib-rs`)

**Done**
- GitHub org を研究エリア軸で分ける方針を決定（既存の `SwReliab` に倣い `MssReliab` を新設。
  略語は全大文字にしないので `MSSReliab` ではなく `MssReliab`）。`relib` (org) と `Reliab`
  (user) は他者が取得済みで使用不可。
- `okamumu/rust-dd` → `MssReliab/relib-rs` へ transfer + rename。`okamumu/relibmss` →
  `MssReliab/relibmss` へ transfer。ローカル remote も張り替え済み。
- `repository` URL と README/`lib.rs` 内の relibmss リンクを新 URL に更新（コミット
  `11d776c`）。publish 後は `repository` を変更できないため、publish 前に実施。
- PR #3 を squash マージ（`fe23566`）。`cargo test` / `cargo test --doc` /
  `cargo publish -p relib-common --dry-run` は通過。

### 2026-07-16 — crates.io 公開完了 (`relib-*` 0.4.0)

**Done**
- publish 前レビューで見つけた5点を修正（コミット `0f17f05`）。いずれも publish 後は
  0.4.1 を切らないと直せないもの:
  1. 各 CHANGELOG に 0.4.0 の項が無かった（`.crate` に同梱されるのに 0.3.3 止まり）。
  2. `LICENSE` が root にしか無く、どの `.crate` にも入っていなかった → 各 crate に配置。
     cargo は **git 管理下のファイルのみ**パッケージするので `git add` が必須。
  3. `relib-common`/`-bdd`/`-mdd` に crate-level doc (`//!`) が無く docs.rs が空だった。
     README は docs.rs に出ないので「直接使うな」の誘導が届いていなかった。
  4. `rpn`（実質の主入口、`&str` DSL）の文法が未文書 → 文法表 + doctest を追加。
  5. `BddNode::new` が `pub` なのに private な `GcState` を受け取り、下流に警告を出していた
     → private 化（`MddNode::new` と一致）。
- 修正後の検証: `cargo test` 21スイート通過、rustdoc 警告 0（`mismatched_lifetime_syntaxes`
  2件も解消）、ビルド警告 0、doctest 2件 → 7件、両 example 動作。
- **5 crate すべて 0.4.0 で publish 済み**。`repository` は全て `MssReliab/relib-rs`。
  docs.rs も5クレートともビルド成功。
- **relibmss** 側の依存を crates.io 版に差し替え（コミット `20fdf70`）。
  `maturin develop` + `pytest` 23件通過。その後 v0.9.0 (`a523ee9`) / v0.10.0 (`2695c84`)
  がリリースされている。

**Notes**
- crates.io は publish に**メールアドレスの verify が必須**（未認証だと 400 で弾かれる。
  ただしアップロード前に落ちるのでバージョンは消費されない）。
- `cargo publish` は publish 後に index 反映を待ってくれるので、依存順に連続実行できる。
- pyo3 の `extension-module` を使う relibmss は素の `cargo build` だと必ずリンクエラーに
  なる（`_PyBaseObject_Type` 等が undefined）。異常ではないので `maturin develop` で確認する。

**Notes**
- `LICENSE` is at repo root only (not bundled per-crate); the SPDX `license = "MIT"`
  field is what crates.io displays.
- `evmdd_core` remains out of the workspace and unrefactored — future work; candidate to
  fold into the `mss` side later.

### 2026-07-19 — apply 高速化（0.4.1 公開 ＋ 0.5.0 準備）

crossリポジトリの `../bdd-bench`（relib BDD vs CUDD、relib MDD vs MEDDLY 0.18.2）で apply の
固定コストを計測しながら段階的に高速化。

**0.4.1（公開済み）:**
- BDD の operation cache を成長 `HashMap` → 直マップ・lossy な `ComputeCache`（CUDD 流儀）に。
  large-DD（n-queens）で CUDD にほぼパリティ。
- `bss::kofn` の指数時間バグ修正（`(k,start)` メモ化で `O(2^n)`→`O(n·k)`、結果 BDD 不変）。

**0.5.0（branch `perf/bdd-native-ite-mdd-cache`、未公開）:**
- `ComputeCache` を `bddcore` → **`common` に移して BDD/MDD 共有**。`retain_live`（op キー）に
  加え `retain_live3`（3 語すべて node id＝ite キャッシュ用）を追加。**公開 API**（prelude 露出）。
- **可換オペランド正規化**（`if f>g {swap}`）: BDD `and`/`or`/`xor`、MDD 同、MTMDD
  `add`/`mul`/`min`/`max`。対称関数でキャッシュ共有 → queens ~2.5×、MTMDD sum ~2.2–2.6×。
- **native `ite`**: 合成（BDD `or(and,and(not))`＝4 apply／MtMdd2 値側 `replace(vif,vif)`＝4 パス）
  を単一 Shannon 再帰＋専用キャッシュに。BDD kofn 5–12×、MDD `mvlkofn`(boolean) ~1.9×・
  `switch`(値側) ~1.3×。いずれも結果 canonical 不変・全 21 テスト通過。値側 `vite` は既存の
  クロス forest 二項再帰 `vif` の三項版（f=bool forest, g/h/結果=value forest、gc は cache を
  clear するのでクロスアリーナ liveness 不要）。
- **Breaking**: MDD の公開 `get_cache`/`get_bcache`/`get_vcache` 削除（キャッシュは実装詳細に）。
  → マイナー昇格 0.4.1→0.5.0。5 crate lockstep、CHANGELOG 追記、`cargo doc` 警告0・doctest 通過。

**Notes**
- **公開手順（0.5.0 を出す場合）**: 依存順 `relib-common`→`-bdd`/`-mdd`→`-bss`/`-mss` で
  `cargo publish`。その後 relibmss 側の依存を 0.5 に上げて `maturin develop`＋`pytest`。
- `../bdd-bench` は別リポジトリ（git 管理外）。MEDDLY ハーネスは `../orsj/Meddly.jl/c` の
  `libmeddly_c` シム（MEDDLY 静的リンク済み）をリンク。ノード数は ZMDD vs 完全簡約で非可比、
  比較軸は build 時間＋確率オラクル一致。
