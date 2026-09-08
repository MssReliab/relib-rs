//! Cross-check against MEDDLY.
//!
//! `tests/fixtures/meddly_cases.txt` is produced by `tools/gen_meddly_fixture.jl`
//! running against MEDDLY through Meddly.jl. It records, per case, the inputs as
//! raw MEDDLY sentinel vectors plus the cardinality and full membership of every
//! derived object. This test rebuilds the same inputs here and checks we agree.
//!
//! The fixture is committed, so the check runs without a Julia or MEDDLY
//! toolchain; regenerating it needs both (and a local `libmeddly_c` build, since
//! `CROSS` is absent from the published `libmeddly_c_jll`).
//!
//! # What this does and does not compare
//!
//! **Cardinality and membership only.** Node counts are deliberately not compared:
//! this crate fuses each variable's source and target into one node where MEDDLY
//! interleaves two levels, so the counts cannot agree and their disagreement would
//! not mean anything.
//!
//! The fixture decides membership by intersecting with an explicit singleton and
//! reading the cardinality, rather than by walking the diagram. Walking it would
//! re-derive MEDDLY's layout and identity reduction inside the generator — the very
//! semantics being cross-checked — so a mistake there could agree with a mistake in
//! this crate's enumerator and hide both. Going through `cardinality` keeps the two
//! sides independent.
//!
//! `transpose` is absent because Meddly.jl exposes no converse operation; it is
//! covered by the internal oracle in `test_cross` instead.

use mxdcore::prelude::*;
use std::collections::HashSet;

const FIXTURE: &str = include_str!("fixtures/meddly_cases.txt");

#[derive(Default)]
struct Case {
    singletons: Vec<(Vec<i32>, Vec<i32>)>,
    lower: Vec<Vec<i32>>,
    upper: Vec<Vec<i32>>,
    cards: Vec<(String, u64)>,
    rel_members: Vec<(String, Transition)>,
    set_members: Vec<(String, StateVec)>,
}

fn nums(s: &str) -> Vec<i32> {
    s.split_whitespace().map(|t| t.parse().unwrap()).collect()
}

fn usizes(s: &str) -> Vec<usize> {
    s.split_whitespace().map(|t| t.parse().unwrap()).collect()
}

fn parse(text: &str) -> (Vec<usize>, Vec<Case>) {
    let mut domains = Vec::new();
    let mut cases = Vec::new();
    let mut cur: Option<Case> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, rest) = match line.split_once(' ') {
            Some((k, r)) => (k, r.trim()),
            None => (line, ""),
        };
        match key {
            "domains" => domains = usizes(rest),
            "case" => cur = Some(Case::default()),
            "end" => cases.push(cur.take().expect("`end` outside a case")),
            _ => {
                let c = cur.as_mut().expect("data line outside a case");
                match key {
                    "singleton" => {
                        let (u, p) = rest.split_once('/').expect("singleton needs `/`");
                        c.singletons.push((nums(u), nums(p)));
                    }
                    "lower" => c.lower.push(nums(rest)),
                    "upper" => c.upper.push(nums(rest)),
                    "card" => {
                        let (name, n) = rest.split_once(' ').unwrap();
                        c.cards.push((name.to_string(), n.trim().parse().unwrap()));
                    }
                    "member" => {
                        let (name, body) = rest.split_once(' ').unwrap();
                        match body.split_once('/') {
                            Some((f, t)) => c.rel_members.push((
                                name.to_string(),
                                (usizes(f), usizes(t)),
                            )),
                            None => c.set_members.push((name.to_string(), usizes(body))),
                        }
                    }
                    other => panic!("unknown fixture key `{other}`"),
                }
            }
        }
    }
    assert!(cur.is_none(), "unterminated case");
    (domains, cases)
}

/// A set minterm from a MEDDLY value vector, where `-1` is DONT_CARE.
fn set_from_sentinels(m: &mut MxdManager, vals: &[i32]) -> NodeId {
    let pattern: Vec<Src> = vals
        .iter()
        .map(|&v| if v == -1 { Src::Any } else { Src::Val(v as usize) })
        .collect();
    m.set_minterm(&pattern)
}

#[test]
fn test_matches_meddly() {
    let (domains, cases) = parse(FIXTURE);
    assert!(!cases.is_empty(), "fixture parsed to no cases");
    assert_eq!(domains, vec![2, 3, 2], "fixture domains changed unexpectedly");

    let mut checked_cards = 0usize;
    let mut checked_members = 0usize;

    for (idx, case) in cases.iter().enumerate() {
        let mut m = MxdManager::new();
        for (i, &n) in domains.iter().enumerate() {
            m.defvar(&format!("v{i}"), n);
        }

        // Rebuild the inputs from the raw sentinel vectors MEDDLY was given.
        let mut rel = m.zero();
        for (u, p) in &case.singletons {
            let e = m.from_meddly_sentinels(u, p);
            rel = m.or_rel(rel, e);
        }
        let mut lower = m.zero();
        for v in &case.lower {
            let e = set_from_sentinels(&mut m, v);
            lower = m.or_set(lower, e);
        }
        let mut upper = m.zero();
        for v in &case.upper {
            let e = set_from_sentinels(&mut m, v);
            upper = m.or_set(upper, e);
        }

        let prod = m.cross(lower, upper);
        let boundary = m.and_rel(prod, rel);
        let post = m.post_image(lower, rel);
        let pre = m.pre_image(upper, rel);
        let post_b = m.post_image(lower, boundary);
        let pre_b = m.pre_image(upper, boundary);

        let named_rel = |n: &str| -> NodeId {
            match n {
                "rel" => rel,
                "cross" => prod,
                "boundary" => boundary,
                other => panic!("unknown relation `{other}`"),
            }
        };
        let named_set = |n: &str| -> NodeId {
            match n {
                "lower" => lower,
                "upper" => upper,
                "post" => post,
                "pre" => pre,
                "post_b" => post_b,
                "pre_b" => pre_b,
                other => panic!("unknown set `{other}`"),
            }
        };

        for (name, expected) in &case.cards {
            let got = match name.as_str() {
                "rel" | "cross" | "boundary" => m.cardinality_relation(named_rel(name)),
                _ => m.cardinality_set(named_set(name)),
            };
            assert_eq!(
                got, *expected,
                "case {idx}: |{name}| disagrees with MEDDLY"
            );
            checked_cards += 1;
        }

        // Membership both ways: everything MEDDLY listed must be present, and we
        // must not hold anything it did not list.
        for which in ["rel", "cross", "boundary"] {
            let expected: HashSet<Transition> = case
                .rel_members
                .iter()
                .filter(|(n, _)| n == which)
                .map(|(_, t)| t.clone())
                .collect();
            let got: HashSet<Transition> =
                m.enumerate_relation(named_rel(which)).into_iter().collect();
            assert_eq!(got, expected, "case {idx}: `{which}` membership");
            checked_members += expected.len();
        }
        for which in ["lower", "upper", "post", "pre", "post_b", "pre_b"] {
            let expected: HashSet<StateVec> = case
                .set_members
                .iter()
                .filter(|(n, _)| n == which)
                .map(|(_, s)| s.clone())
                .collect();
            let got: HashSet<StateVec> = m.enumerate_set(named_set(which)).into_iter().collect();
            assert_eq!(got, expected, "case {idx}: `{which}` membership");
            checked_members += expected.len();
        }
    }

    // Guard against a fixture that silently parses to nothing meaningful.
    assert!(
        checked_cards >= 9 * cases.len(),
        "expected nine cardinalities per case, got {checked_cards} over {} cases",
        cases.len()
    );
    assert!(checked_members > 0, "fixture recorded no members at all");
}

/// The fixture must actually exercise the sentinel distinction, otherwise the
/// cross-check would pass while saying nothing about the footgun it is meant to
/// cover.
#[test]
fn test_fixture_covers_the_sentinels() {
    let (_, cases) = parse(FIXTURE);
    let mut dont_change = 0;
    let mut dont_care_primed = 0;
    let mut fully_specified = 0;
    for c in &cases {
        for (u, p) in &c.singletons {
            dont_change += p.iter().filter(|&&v| v == -2).count();
            dont_care_primed += p.iter().filter(|&&v| v == -1).count();
            if u.iter().all(|&v| v >= 0) && p.iter().all(|&v| v >= 0) {
                fully_specified += 1;
            }
        }
    }
    assert!(dont_change > 0, "no DONT_CHANGE component in the fixture");
    assert!(
        dont_care_primed == 0,
        "the generator should never emit DONT_CARE on the primed side"
    );
    assert!(fully_specified > 0, "no fully specified transition");
}
