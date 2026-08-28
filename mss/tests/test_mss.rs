use std::collections::HashMap;

use mss::prelude::*;

/// Every vector `mincut(φ).extract([v])` yields must satisfy the **definition** of a minimal
/// cut vector at level `v`: `φ(x) == v`, and raising *any* single component by one state must
/// push `φ` above `v` (maximality within `{x : φ(x) <= v}`). Conversely every genuine maximal
/// cut vector must appear somewhere in the family.
///
/// This is checked directly rather than against a `{x : φ(x) <= v}` oracle, because the family
/// files each vector under the label equal to its **own** `φ(x)`: a vector with `φ(x) < v` can
/// be maximal within `{x : φ(x) <= v}` yet live in a lower stratum. `extract_level` is what
/// reassembles the classical set — see `test_mincut_levels_vs_strata`.
#[test]
fn test_mincut_matches_bruteforce() {
    use std::collections::HashSet;

    // Enumerate every state vector and evaluate φ at each point via point-mass probabilities.
    fn eval_all(
        node: &mut MddNode<i32>,
        vars: &[(&str, usize)],
        maxval: i32,
    ) -> Vec<(Vec<usize>, i32)> {
        let mut assigns: Vec<Vec<usize>> = vec![vec![]];
        for &(_, m) in vars {
            let mut next = Vec::new();
            for a in &assigns {
                for s in 0..m {
                    let mut b = a.clone();
                    b.push(s);
                    next.push(b);
                }
            }
            assigns = next;
        }
        assigns
            .into_iter()
            .map(|a| {
                let pv: HashMap<String, Vec<f64>> = vars
                    .iter()
                    .zip(a.iter())
                    .map(|(&(name, m), &s)| {
                        let mut e = vec![0.0; m];
                        e[s] = 1.0;
                        (name.to_string(), e)
                    })
                    .collect();
                let v = (0..=maxval).find(|&v| node.prob(&pv, &[v]) > 0.5).unwrap();
                (a, v)
            })
            .collect()
    }

    fn check(node: &mut MddNode<i32>, cut: &ZmddNode<i32>, vars: &[(&str, usize)], maxval: i32) {
        let table = eval_all(node, vars, maxval);
        let phi = |x: &[usize]| table.iter().find(|(a, _)| a == x).unwrap().1;
        let dense = |d: &HashMap<String, usize>| -> Vec<usize> {
            vars.iter().map(|&(n, _)| d[n]).collect()
        };

        let mut returned: Vec<(Vec<usize>, i32)> = Vec::new();
        for v in 0..=maxval {
            let ss: HashSet<i32> = [v].into_iter().collect();
            for d in cut.extract(&ss) {
                let x = dense(&d);
                // (1) the label is φ's own value at x
                assert_eq!(phi(&x), v, "vector {x:?} filed under {v} but φ = {}", phi(&x));
                // (2) x is maximal within {y : φ(y) <= v}: every +1 step exceeds v
                for (i, &(_, m)) in vars.iter().enumerate() {
                    if x[i] + 1 < m {
                        let mut y = x.clone();
                        y[i] += 1;
                        assert!(
                            phi(&y) > v,
                            "vector {x:?} at level {v} is not maximal: raising component {i} \
                             gives φ = {}",
                            phi(&y)
                        );
                    }
                }
                returned.push((x, v));
            }
        }

        // (3) nothing genuine is missing: every x that is maximal within {y : φ(y) <= φ(x)}
        // must be in the family.
        for (x, v) in &table {
            let maximal = !table
                .iter()
                .any(|(y, w)| y != x && y.iter().zip(x).all(|(a, b)| a >= b) && w <= v);
            if maximal {
                assert!(
                    returned.contains(&(x.clone(), *v)),
                    "genuine maximal cut vector {x:?} (φ = {v}) missing from the family"
                );
            }
        }
    }

    // value forest: max(min(x,y), z), K = 3
    {
        let mut m: MssMgr<i32> = MssMgr::new();
        let x = m.defvar("x", 3);
        let y = m.defvar("y", 3);
        let z = m.defvar("z", 3);
        let mut phi = x.min(&y).max(&z);
        let cut = m.mincut(&phi).expect("coherent");
        check(&mut phi, &cut, &[("x", 3), ("y", 3), ("z", 3)], 2);
    }

    // boolean forest: [x>=1] & [y>=2]  (Bool-tagged), K = 2 (values 0/1)
    {
        let mut m: MssMgr<i32> = MssMgr::new();
        let x = m.defvar("x", 3);
        let y = m.defvar("y", 3);
        let one = m.value(1);
        let two = m.value(2);
        let mut phi = x.ge(&one).and(&y.ge(&two));
        let cut = m.mincut(&phi).expect("coherent");
        check(&mut phi, &cut, &[("x", 3), ("y", 3)], 1);
    }

    // asymmetric state counts, with a variable that forces φ to its minimum on its own.
    // (This is the shape that exposes the stratum-vs-level distinction; see
    // `test_mincut_levels_vs_strata`.)
    {
        let mut m: MssMgr<i32> = MssMgr::new();
        let a = m.defvar("a", 2);
        let b = m.defvar("b", 3);
        let c = m.defvar("c", 3);
        let mut phi = a.min(&b.max(&c));
        let cut = m.mincut(&phi).expect("coherent");
        check(&mut phi, &cut, &[("a", 2), ("b", 3), ("c", 3)], 2);
    }

    // non-coherent -> None
    {
        let mut m: MssMgr<i32> = MssMgr::new();
        let x = m.defvar("x", 3);
        let y = m.defvar("y", 3);
        let diff = x.sub(&y); // decreases in y
        assert!(m.mincut(&diff).is_none(), "x - y is not coherent");
    }
}

/// The family stratifies by the vector's **own** `φ(x)`, while the classical minimal cut
/// vectors at level `v` are the maximal elements of `{x : φ(x) <= v}`. The two coincide at the
/// extreme labels and differ in between; `extract_level` bridges them.
///
/// `φ = min(4a, b+c)` with `a` binary (= "a is down ⟹ the system is down"): `(0,2,2)` has
/// `φ = 0`, so it sits in stratum 0, yet it stays maximal within `{x : φ(x) <= v}` for every
/// `v < 4` and therefore belongs to levels 1..3 as well.
#[test]
fn test_mincut_levels_vs_strata() {
    use std::collections::HashSet;

    let mut m: MssMgr<i32> = MssMgr::new();
    let a = m.defvar("a", 2);
    let b = m.defvar("b", 3);
    let c = m.defvar("c", 3);
    let four = m.value(4);
    let phi = a.mul(&four).min(&b.add(&c));
    let cut = m.mincut(&phi).expect("coherent");

    let names = ["a", "b", "c"];
    let vecs = |it: Vec<HashMap<String, usize>>| {
        let mut v: Vec<Vec<usize>> = it
            .iter()
            .map(|d| names.iter().map(|n| d[*n]).collect())
            .collect();
        v.sort();
        v
    };
    let stratum = |v: i32| {
        let ss: HashSet<i32> = [v].into_iter().collect();
        vecs(cut.extract(&ss).collect())
    };
    let level = |v: i32| vecs(cut.extract_level(v));

    assert!(cut.is_cut(), "mincut produces a cut family");
    assert_eq!(cut.labels(), vec![0, 1, 2, 3, 4]);

    // Stratum v holds only the vectors whose own φ is exactly v...
    assert_eq!(stratum(1), vec![vec![1, 0, 1], vec![1, 1, 0]]);
    // ...while level v is the classical maximal{x : φ(x) <= v}: (0,2,2) comes along from
    // stratum 0, which the stratum reading alone misses.
    assert_eq!(
        level(1),
        vec![vec![0, 2, 2], vec![1, 0, 1], vec![1, 1, 0]]
    );
    assert_eq!(
        level(3),
        vec![vec![0, 2, 2], vec![1, 1, 2], vec![1, 2, 1]]
    );

    // At the extreme labels the two agree: at the bottom because φ <= 0 ⟺ φ = 0, at the top
    // because the all-max vector dominates everything else.
    assert_eq!(stratum(0), level(0));
    assert_eq!(stratum(4), level(4));
    assert_eq!(stratum(4), vec![vec![1, 2, 2]], "the baseline member");

    // Same distinction on the path side, mirrored (minimal{φ >= v}).
    let path = m.minpath(&phi).expect("coherent");
    assert!(!path.is_cut());
    let pstratum = |v: i32| {
        let ss: HashSet<i32> = [v].into_iter().collect();
        vecs(path.extract(&ss).collect())
    };
    assert_eq!(pstratum(0), vec![vec![0, 0, 0]], "the baseline member");
    assert_eq!(pstratum(0), vecs(path.extract_level(0)));
}
#[test]
fn test_bmeas_matches_pinned_prob() {
    fn check(mgr_build: impl Fn(&mut MddMgr<i32>) -> MddNode<i32>, k: usize) {
        let mut mgr: MddMgr<i32> = MddMgr::new();
        let mut node = mgr_build(&mut mgr);
        let states = 3usize;
        // Arbitrary non-degenerate probability vectors per variable.
        let base: HashMap<String, Vec<f64>> = [
            ("x", vec![0.2, 0.3, 0.5]),
            ("y", vec![0.5, 0.1, 0.4]),
            ("z", vec![0.25, 0.25, 0.5]),
        ]
        .iter()
        .map(|(s, v)| (s.to_string(), v.clone()))
        .collect();
        let ss: Vec<i32> = (1..k as i32).collect(); // success = performance >= 1

        // P(φ∈ss | var = j): pin `var` to state j (unit vector e_j).
        let cond = |node: &mut MddNode<i32>, var: &str, j: usize| {
            let mut pinned = base.clone();
            let mut e = vec![0.0; states];
            e[j] = 1.0;
            pinned.insert(var.to_string(), e);
            node.prob(&pinned, &ss)
        };

        let bm = node.bmeas(&base, &ss);
        for (var, vec) in &bm {
            assert_eq!(vec.len(), states - 1, "one difference per state boundary");
            for (d, &g) in vec.iter().enumerate() {
                let expected = cond(&mut node, var, d + 1) - cond(&mut node, var, d);
                assert!(
                    (g - expected).abs() < 1e-9,
                    "bmeas[{var}][{d}] = {g}, expected P(.|{var}={}) - P(.|{var}={d}) = {expected}",
                    d + 1
                );
            }
        }
    }
    // value forest: max(min(x,y), z), K = 3 (values 0..2)
    check(
        |mgr| {
            let x = mgr.defvar("x", 3);
            let y = mgr.defvar("y", 3);
            let z = mgr.defvar("z", 3);
            x.min(&y).max(&z)
        },
        3,
    );
    // boolean forest: [x>=1] & [y>=2]  (a Bool-tagged structure function)
    check(
        |mgr| {
            let x = mgr.defvar("x", 3);
            let y = mgr.defvar("y", 3);
            let one = mgr.value(1);
            let two = mgr.value(2);
            x.ge(&one).and(&y.ge(&two))
        },
        2,
    );
}

#[test]
fn test_mdd_mgr() {
    let mut mgr: MddMgr<i32> = MddMgr::new();
    let x = mgr.defvar("x", 3);
    let y = mgr.defvar("y", 3);
    let z = mgr.defvar("z", 3);
    // let zero = mgr.zero();
    // let one = mgr.one();
    // let two = mgr.val(2);
    let mut vars = HashMap::new();
    vars.insert("x".to_string(), 3);
    vars.insert("y".to_string(), 3);
    vars.insert("z".to_string(), 3);
    let rpn = "x y z + *";
    if let Ok(node) = mgr.rpn(rpn, &vars) {
        println!("{}", node.dot());
    }
}

#[test]
fn test_minpath_coherence_detection() {
    let mut mgr: MssMgr<i32> = MssMgr::new();
    let x = mgr.defvar("x", 3);
    let y = mgr.defvar("y", 3);

    // Coherent value functions (non-decreasing in every component) -> Some.
    let sum = x.add(&y); // x + y
    assert!(mgr.minpath(&sum).is_some(), "x + y is coherent");
    let mx = x.max(&y); // max(x, y)
    assert!(mgr.minpath(&mx).is_some(), "max(x,y) is coherent");

    // Non-coherent value: x - y decreases in y.
    let diff = x.sub(&y);
    assert!(mgr.minpath(&diff).is_none(), "x - y is not coherent");

    // Boolean structure functions via comparison.
    let one = mgr.value(1);
    let ge = x.ge(&one); // [x >= 1] : non-decreasing in x -> coherent
    assert!(mgr.minpath(&ge).is_some(), "[x>=1] is coherent");
    let lt = x.lt(&y); // [x < y] : decreasing in x -> not coherent
    assert!(mgr.minpath(&lt).is_none(), "[x<y] is not coherent");
    let eq = x.eq(&y); // [x == y] : not monotone
    assert!(mgr.minpath(&eq).is_none(), "[x==y] is not coherent");
}

/// Regression for the minsol non-minimal bug (the `without` (NonTerminal, Terminal)
/// case must recurse into f's zero branch, not expand every branch). `max(min(x,y), z)`
/// is the multi-state analogue of `x&y|z`; its minimal path vectors are exactly
/// {z=1},{z=2},{x=1,y=1},{x=2,y=2} — the old code fabricated non-minimal vectors like
/// {y=1,z=2}.
#[test]
fn test_minpath_no_spurious_vectors() {
    use std::collections::HashSet;
    let mut mgr: MssMgr<i32> = MssMgr::new();
    let x = mgr.defvar("x", 3);
    let y = mgr.defvar("y", 3);
    let z = mgr.defvar("z", 3);

    let phi = x.min(&y).max(&z); // max(min(x,y), z)
    let mp = mgr.minpath(&phi).expect("max(min(x,y),z) is coherent");

    let ss: HashSet<i32> = [1, 2].into_iter().collect();
    let mut got: Vec<Vec<(String, usize)>> = mp
        .extract(&ss)
        .map(|d| {
            let mut v: Vec<(String, usize)> =
                d.into_iter().filter(|(_, val)| *val != 0).collect();
            v.sort();
            v
        })
        .collect();
    got.sort();

    let mk = |items: &[(&str, usize)]| {
        let mut v: Vec<(String, usize)> =
            items.iter().map(|(s, n)| (s.to_string(), *n)).collect();
        v.sort();
        v
    };
    let mut expected = vec![
        mk(&[("z", 1)]),
        mk(&[("z", 2)]),
        mk(&[("x", 1), ("y", 1)]),
        mk(&[("x", 2), ("y", 2)]),
    ];
    expected.sort();

    assert_eq!(got, expected, "minpath must be exactly the 4 minimal path vectors");
}

#[test]
fn test_zmdd_intersect_setdiff() {
    use std::collections::HashSet;

    fn sorted_vecs(z: &ZmddNode<i32>, ss: &HashSet<i32>) -> Vec<Vec<(String, usize)>> {
        let mut v: Vec<Vec<(String, usize)>> = z
            .extract(ss)
            .map(|d| {
                let mut e: Vec<(String, usize)> = d.into_iter().collect();
                e.sort();
                e
            })
            .collect();
        v.sort();
        v
    }
    fn set(items: &[(&str, usize)]) -> Vec<(String, usize)> {
        let mut v: Vec<(String, usize)> = items.iter().map(|(s, n)| (s.to_string(), *n)).collect();
        v.sort();
        v
    }

    let mut mgr: MssMgr<i32> = MssMgr::new();
    let x = mgr.defvar("x", 3);
    let y = mgr.defvar("y", 3);
    let z = mgr.defvar("z", 3);

    // f = max(min(x,y), z): MPV {z=1},{z=2},{x=1,y=1},{x=2,y=2}
    let f = x.min(&y).max(&z);
    // g = min(x,y): MPV {x=1,y=1},{x=2,y=2}
    let g = x.min(&y);
    let a = mgr.minpath(&f).expect("coherent");
    let b = mgr.minpath(&g).expect("coherent");

    let ss: HashSet<i32> = [1, 2].into_iter().collect();

    // intersect (label-wise): {x=1,y=1},{x=2,y=2}
    let inter = a.intersect(&b);
    assert_eq!(
        sorted_vecs(&inter, &ss),
        vec![
            set(&[("x", 1), ("y", 1), ("z", 0)]),
            set(&[("x", 2), ("y", 2), ("z", 0)])
        ]
    );
    assert_eq!(inter.count(&ss), 2);

    // setdiff a - b (label-wise): {z=1},{z=2}
    let diff = a.setdiff(&b);
    assert_eq!(
        sorted_vecs(&diff, &ss),
        vec![
            set(&[("x", 0), ("y", 0), ("z", 1)]),
            set(&[("x", 0), ("y", 0), ("z", 2)])
        ]
    );
    assert_eq!(diff.count(&ss), 2);
}

#[test]
fn test_zmdd_dot() {
    let mut mgr: MssMgr<i32> = MssMgr::new();
    let x = mgr.defvar("x", 3);
    let y = mgr.defvar("y", 3);
    let z = mgr.defvar("z", 3);
    let f = x.min(&y).max(&z);
    let a = mgr.minpath(&f).expect("coherent");

    let dot = a.dot();
    assert!(dot.starts_with("digraph {"));
    // The empty family (Undet) and the edges into it are omitted.
    assert!(!dot.contains("Undet"), "Undet terminal should not be drawn:\n{dot}");
    assert!(dot.trim_end().ends_with('}'));
    for label in ["x", "y", "z"] {
        assert!(dot.contains(&format!("label=\"{}\"", label)), "missing {}", label);
    }
}

/// Every comparison must agree with plain integer comparison at every point of the
/// state space. Checked by pinning both variables to a single state with point-mass
/// probabilities and reading the boolean diagram back.
///
/// This is what catches an operator that quietly implements a neighbouring one:
/// `le` used to return `lt` unless `eq` happened to be the constant-true node, so
/// `x <= y` was `x < y` for every non-degenerate pair.
#[test]
fn test_comparisons_agree_with_integers_pointwise() {
    let states = 3usize;
    type Build = fn(&MddNode<i32>, &MddNode<i32>) -> MddNode<i32>;
    let ops: [(&str, Build, fn(usize, usize) -> bool); 6] = [
        ("<", |a, b| a.lt(b), |i, j| i < j),
        ("<=", |a, b| a.le(b), |i, j| i <= j),
        (">", |a, b| a.gt(b), |i, j| i > j),
        (">=", |a, b| a.ge(b), |i, j| i >= j),
        ("==", |a, b| a.eq(b), |i, j| i == j),
        ("!=", |a, b| a.ne(b), |i, j| i != j),
    ];
    for (name, build, truth) in ops {
        let mut mgr: MddMgr<i32> = MddMgr::new();
        let x = mgr.defvar("x", states);
        let y = mgr.defvar("y", states);
        let mut node = build(&x, &y);
        for i in 0..states {
            for j in 0..states {
                let pv: HashMap<String, Vec<f64>> = [("x", i), ("y", j)]
                    .iter()
                    .map(|&(nm, s)| {
                        let mut e = vec![0.0; states];
                        e[s] = 1.0;
                        (nm.to_string(), e)
                    })
                    .collect();
                let got = node.prob(&pv, &[1]) > 0.5;
                assert_eq!(got, truth(i, j), "x {name} y at x={i}, y={j}");
            }
        }
    }
}
