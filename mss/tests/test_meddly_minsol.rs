//! Cross-check `minpath` (手法A, minimal path vectors) against MEDDLY.
//!
//! `tests/fixtures/minsol_cases.txt` is produced by `tools/gen_minsol_fixture.jl`
//! driving `MDDMinsol.all_mpv_minsol` over MEDDLY. For each system it records the
//! **full truth table** of φ plus, for every level `j`, the minimal path vectors
//! `MPV(j) = minimal{x : φ(x) ≥ j}`.
//!
//! The truth table is the point. This side rebuilds φ from it node by node rather
//! than re-deriving it from the same formula, so the two implementations are
//! compared on a function that is demonstrably identical — not on one each side
//! constructed for itself, where a shared misreading of the formula would cancel.
//!
//! Everything is in **x-order** (component index), never level order. The two
//! engines assign levels differently (`MDDMinsol` uses `assign_levels(order=:good)`,
//! this side uses declaration order), and minimal vectors are a property of the
//! function, so they must not depend on it. That the two agree anyway is part of
//! what is being checked.
//!
//! Why this matters: the two minsol implementations deliberately differ in how a
//! skipped level is read — this crate is ZMDD-flavoured (`Undet` terminal, skipped
//! level = state 0) while MEDDLY is fully reduced (skipped = DONT_CARE), which is
//! why `MDDMinsol` rewrote its own rather than porting `mdd_minsol.rs`. Both are
//! separately verified by brute force; until now they had never been checked
//! against each other.
//!
//! The fixture is committed, so this runs without a Julia or MEDDLY toolchain.

use mss::prelude::*;
use std::collections::{HashMap, HashSet};

const FIXTURE: &str = include_str!("fixtures/minsol_cases.txt");

#[derive(Default)]
struct Case {
    name: String,
    domains: Vec<usize>,
    m: i32,
    coherent: bool,
    /// x-order state vector -> φ(x)
    table: HashMap<Vec<usize>, i32>,
    /// level j -> the minimal path vectors MEDDLY reports
    mpv: HashMap<i32, HashSet<Vec<usize>>>,
}

fn usizes(s: &str) -> Vec<usize> {
    s.split_whitespace().map(|t| t.parse().unwrap()).collect()
}

fn parse(text: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut cur: Option<Case> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "case" => {
                cur = Some(Case {
                    name: rest.to_string(),
                    ..Default::default()
                })
            }
            "end" => cases.push(cur.take().expect("`end` outside a case")),
            _ => {
                let c = cur.as_mut().expect("data outside a case");
                match key {
                    "domains" => c.domains = usizes(rest),
                    "m" => c.m = rest.parse().unwrap(),
                    "coherent" => c.coherent = rest == "true",
                    "phi" => {
                        let v = usizes(rest);
                        let (x, val) = v.split_at(v.len() - 1);
                        c.table.insert(x.to_vec(), val[0] as i32);
                    }
                    "mpv" => {
                        let v = usizes(rest);
                        c.mpv
                            .entry(v[0] as i32)
                            .or_default()
                            .insert(v[1..].to_vec());
                    }
                    other => panic!("unknown fixture key `{other}`"),
                }
            }
        }
    }
    cases
}

/// Rebuild φ from its truth table, bottom-up through the MDD.
///
/// Component `i` is declared `i`-th, so it sits at level `i`, and level `K-1` is the
/// root. The recursion fixes one component per level from the root down; at the
/// bottom every component is assigned and the leaf is φ at that point.
fn build_phi(
    mgr: &MssMgr<i32>,
    headers: &[HeaderId],
    domains: &[usize],
    table: &HashMap<Vec<usize>, i32>,
    level: i64,
    assign: &mut Vec<usize>,
) -> MddNode<i32> {
    if level < 0 {
        let v = table[assign.as_slice()];
        return mgr.value(v);
    }
    let l = level as usize;
    let children: Vec<MddNode<i32>> = (0..domains[l])
        .map(|v| {
            assign[l] = v;
            build_phi(mgr, headers, domains, table, level - 1, assign)
        })
        .collect();
    mgr.create_node(headers[l], &children)
}

/// φ evaluated through the diagram, using point-mass probabilities — the idiom the
/// rest of this suite uses. Confirms the rebuilt diagram really is the fixture's
/// function before any conclusion is drawn from it.
fn eval(node: &mut MddNode<i32>, names: &[String], x: &[usize], domains: &[usize], m: i32) -> i32 {
    let pv: HashMap<String, Vec<f64>> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let mut e = vec![0.0; domains[i]];
            e[x[i]] = 1.0;
            (n.clone(), e)
        })
        .collect();
    (0..m)
        .find(|&v| node.prob(&pv, &[v]) > 0.5)
        .expect("φ must take some value at every point")
}

fn dense(d: &HashMap<String, usize>, names: &[String]) -> Vec<usize> {
    names.iter().map(|n| d[n]).collect()
}

#[test]
fn test_minpath_matches_meddly() {
    let cases = parse(FIXTURE);
    assert!(!cases.is_empty(), "fixture parsed to no cases");
    let mut checked_levels = 0usize;

    for case in &cases {
        let k = case.domains.len();
        let names: Vec<String> = (0..k).map(|i| format!("x{i}")).collect();

        let mut mgr: MssMgr<i32> = MssMgr::new();
        let headers: Vec<HeaderId> = names
            .iter()
            .zip(&case.domains)
            .map(|(n, &d)| {
                mgr.defvar(n, d)
                    .get_header()
                    .expect("a declared variable has a header")
            })
            .collect();

        let mut assign = vec![0usize; k];
        let mut phi = build_phi(
            &mgr,
            &headers,
            &case.domains,
            &case.table,
            k as i64 - 1,
            &mut assign,
        );

        // The rebuilt φ must be the fixture's function, at every point.
        for (x, &want) in &case.table {
            let got = eval(&mut phi, &names, x, &case.domains, case.m);
            assert_eq!(
                got, want,
                "{}: rebuilt φ{x:?} = {got}, fixture says {want}",
                case.name
            );
        }

        let family = mgr.minpath(&phi);
        assert_eq!(
            family.is_some(),
            case.coherent,
            "{}: coherence verdict disagrees with MEDDLY",
            case.name
        );
        let Some(family) = family else { continue };

        for j in 1..case.m {
            let want = case.mpv.get(&j).cloned().unwrap_or_default();
            let got: HashSet<Vec<usize>> = family
                .extract_level(j)
                .iter()
                .map(|d| dense(d, &names))
                .collect();
            assert_eq!(
                got, want,
                "{}: MPV({j}) disagrees with MEDDLY",
                case.name
            );
            checked_levels += 1;
        }
    }

    assert!(
        checked_levels >= cases.len(),
        "expected at least one level per case, got {checked_levels}"
    );
}

/// Every vector MEDDLY reports must genuinely be a minimal path vector of the
/// fixture's own truth table. This does not involve either diagram engine, so it
/// catches the case where both sides agree because they share a mistake.
#[test]
fn test_fixture_mpv_are_really_minimal() {
    for case in parse(FIXTURE) {
        if !case.coherent {
            continue;
        }
        for (&j, vectors) in &case.mpv {
            // Sound: each reported vector reaches level j, and no strictly smaller
            // vector does.
            for x in vectors {
                assert!(case.table[x] >= j, "{}: φ{x:?} < {j}", case.name);
                for (y, &v) in &case.table {
                    if v >= j && y != x && y.iter().zip(x).all(|(a, b)| a <= b) {
                        panic!("{}: {x:?} is not minimal at level {j}: {y:?} is below it", case.name);
                    }
                }
            }
            // Complete: every minimal element of {φ ≥ j} is reported.
            for (x, &v) in &case.table {
                if v < j {
                    continue;
                }
                let minimal = !case
                    .table
                    .iter()
                    .any(|(y, &w)| w >= j && y != x && y.iter().zip(x).all(|(a, b)| a <= b));
                if minimal {
                    assert!(
                        vectors.contains(x),
                        "{}: minimal vector {x:?} missing from MPV({j})",
                        case.name
                    );
                }
            }
        }
    }
}
