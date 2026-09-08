//! Ergonomic wrapper over [`ZmddManager`]: `ZmddMgr` / `ZmddNode` for the set families of
//! minimal path vectors. Families are produced by [`MssMgr::minpath`](crate::mss::MssMgr::minpath)
//! and support the label-wise set operations `intersect` / `setdiff` (see `mddcore::zmdd_ops`),
//! plus `count` / `extract`.

use mddcore::prelude::*;
use mddcore::mtmdd::Node as ZNode;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use crate::zmdd_convert;

const GC_FLOOR: usize = 1 << 16;

#[derive(Debug)]
struct GcState {
    roots: BddHashMap<NodeId, u32>,
    threshold: usize,
    floor: usize,
}

fn maybe_gc<V>(zmdd: &Rc<RefCell<ZmddManager<V>>>, gc: &Rc<RefCell<GcState>>)
where
    V: MddValue,
{
    if zmdd.borrow().live_node_count() < gc.borrow().threshold {
        return;
    }
    let roots: Vec<NodeId> = gc.borrow().roots.keys().copied().collect();
    let live = {
        let mut z = zmdd.borrow_mut();
        z.gc(&roots);
        z.live_node_count()
    };
    let mut s = gc.borrow_mut();
    s.threshold = live.saturating_mul(2).max(s.floor);
}

/// Manager (forest owner) for **minimal path/cut vector families** (ZMDDs). Owned by an
/// [`MssMgr`](crate::mss::MssMgr); `minpath` results live here. Two families must come from
/// the same `ZmddMgr` to be combined with the [`ZmddNode`] set operations.
pub struct ZmddMgr<V> {
    zmdd: Rc<RefCell<ZmddManager<V>>>,
    gc: Rc<RefCell<GcState>>,
}

impl<V> ZmddMgr<V>
where
    V: MddValue,
{
    pub fn new() -> Self {
        ZmddMgr {
            zmdd: Rc::new(RefCell::new(ZmddManager::new())),
            gc: Rc::new(RefCell::new(GcState {
                roots: BddHashMap::default(),
                threshold: GC_FLOOR,
                floor: GC_FLOOR,
            })),
        }
    }

    fn wrap(&self, node: NodeId, reverse: bool, vars: Rc<Vec<(String, usize)>>) -> ZmddNode<V> {
        let n = ZmddNode::new(&self.zmdd, &self.gc, node, reverse, vars);
        maybe_gc(&self.zmdd, &self.gc);
        n
    }

    /// Convert a minsol result (the fake-ZMDD `Node` in a `MtMdd2Manager`) into a genuine
    /// ZMDD family in this manager. Used by [`MssMgr::minpath`](crate::mss::MssMgr::minpath).
    pub(crate) fn convert(
        &self,
        src_rc: &Rc<RefCell<MtMdd2Manager<V>>>,
        node: &Node,
        vars: Rc<Vec<(String, usize)>>,
        baseline: V,
    ) -> ZmddNode<V> {
        let id = {
            let src = src_rc.borrow();
            let mut dst = self.zmdd.borrow_mut();
            let id = zmdd_convert::to_zmdd(&src, node, &mut dst, false);
            dst.set_baseline(id, baseline)
        };
        self.wrap(id, false, vars)
    }

    /// Convert a **maxsol** result (fake-ZMDD, baseline on the top edge) into a genuine ZMDD
    /// with edges reversed to bottom-baseline "levels below max" (`d`) coordinates. The
    /// resulting node is flagged `reverse`, so its `extract` reports each recorded component
    /// as the true state `edge_num-1 - d`. Used by [`MssMgr::mincut`](crate::mss::MssMgr::mincut).
    pub(crate) fn convert_rev(
        &self,
        src_rc: &Rc<RefCell<MtMdd2Manager<V>>>,
        node: &Node,
        vars: Rc<Vec<(String, usize)>>,
        baseline: V,
    ) -> ZmddNode<V> {
        let id = {
            let src = src_rc.borrow();
            let mut dst = self.zmdd.borrow_mut();
            let id = zmdd_convert::to_zmdd(&src, node, &mut dst, true);
            dst.set_baseline(id, baseline)
        };
        self.wrap(id, true, vars)
    }

    pub fn size(&self) -> (usize, usize, usize, usize) {
        self.zmdd.borrow().size()
    }

    pub fn clear_cache(&mut self) {
        self.zmdd.borrow_mut().clear_cache();
    }
}

/// A handle to a set family in a [`ZmddMgr`]'s forest.
///
/// `reverse` distinguishes a **cut** family (`mincut`, edges reversed to "levels below max"
/// coordinates) from a **path** family (`minpath`); it decides how `extract` reports the
/// per-component state (`true state = edge_num-1 - d` when reversed) and which baseline the
/// unrecorded components take (max state vs `0`). Query it with [`is_cut`](Self::is_cut).
///
/// The family is **stratified by the vector's own `φ(x)`** — see [`extract`](Self::extract)
/// and [`extract_level`](Self::extract_level) — and always contains the baseline member (the
/// all-0 vector for paths, the all-max vector for cuts), which is trivial but correct.
#[derive(Debug)]
pub struct ZmddNode<V> {
    parent: Weak<RefCell<ZmddManager<V>>>,
    gc: Weak<RefCell<GcState>>,
    node: NodeId,
    reverse: bool,
    /// Every variable of the structure function with its number of states, as of the
    /// `minpath`/`mincut` call. Used to fill in the components a sparse vector omits.
    vars: Rc<Vec<(String, usize)>>,
}

impl<V> ZmddNode<V>
where
    V: MddValue,
{
    fn from_weak(
        parent: Weak<RefCell<ZmddManager<V>>>,
        gc: Weak<RefCell<GcState>>,
        node: NodeId,
        reverse: bool,
        vars: Rc<Vec<(String, usize)>>,
    ) -> Self {
        if let Some(g) = gc.upgrade() {
            *g.borrow_mut().roots.entry(node).or_insert(0) += 1;
        }
        ZmddNode { parent, gc, node, reverse, vars }
    }

    fn new(
        zmdd: &Rc<RefCell<ZmddManager<V>>>,
        gc: &Rc<RefCell<GcState>>,
        node: NodeId,
        reverse: bool,
        vars: Rc<Vec<(String, usize)>>,
    ) -> Self {
        Self::from_weak(Rc::downgrade(zmdd), Rc::downgrade(gc), node, reverse, vars)
    }

    fn rewrap(&self, zmdd: &Rc<RefCell<ZmddManager<V>>>, node: NodeId) -> Self {
        let n = ZmddNode::from_weak(
            self.parent.clone(),
            self.gc.clone(),
            node,
            self.reverse,
            self.vars.clone(),
        );
        if let Some(gc) = self.gc.upgrade() {
            maybe_gc(zmdd, &gc);
        }
        n
    }

    pub fn get_mgr(&self) -> Rc<RefCell<ZmddManager<V>>> {
        self.parent.upgrade().unwrap()
    }

    pub fn get_id(&self) -> NodeId {
        self.node
    }

    /// `true` for a **cut** family (from [`MssMgr::mincut`](crate::mss::MssMgr::mincut)),
    /// `false` for a **path** family (from [`MssMgr::minpath`](crate::mss::MssMgr::minpath)).
    /// The two are read on opposite baselines — see [`extract`](Self::extract).
    pub fn is_cut(&self) -> bool {
        self.reverse
    }

    /// Every variable of the structure function with its number of states, as of the
    /// `minpath`/`mincut` call.
    pub fn vars(&self) -> &[(String, usize)] {
        &self.vars
    }

    /// The terminal labels (performance values) this family stratifies over, ascending.
    ///
    /// A vector is filed under the label **equal to its own `φ(x)`**, so this is the set of
    /// levels for which the family holds anything.
    pub fn labels(&self) -> Vec<V> {
        let mgr = self.parent.upgrade().unwrap();
        let dd = mgr.borrow();
        let mut seen = BddHashSet::default();
        let mut out = Vec::new();
        let mut stack = vec![self.node];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            match dd.get_node(&id).unwrap() {
                ZNode::Undet => (),
                ZNode::Terminal(t) => out.push(t.value()),
                ZNode::NonTerminal(f) => stack.extend(f.iter()),
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// The **classical** minimal path / cut vectors *at level* `v`:
    /// `minimal{x : φ(x) ≥ v}` for a path family, `maximal{x : φ(x) ≤ v}` for a cut family.
    ///
    /// [`extract`](Self::extract) alone answers a different question — it returns the stratum
    /// whose `φ(x)` is **exactly** `v`. The classical set is the union of the strata on the
    /// relevant side of `v` with the dominated vectors removed, which is what this does.
    /// The two agree at the extreme labels but differ in between: for a cut family a vector
    /// with `φ(x) < v` can still be maximal within `{x : φ(x) ≤ v}`, and it lives in a lower
    /// stratum.
    pub fn extract_level(&self, v: V) -> Vec<HashMap<String, usize>> {
        let ss: HashSet<V> = self
            .labels()
            .into_iter()
            .filter(|w| if self.reverse { *w <= v } else { *w >= v })
            .collect();
        let vectors: Vec<HashMap<String, usize>> = self.extract(&ss).collect();
        // Keep the extreme elements: maximal for cuts, minimal for paths.
        vectors
            .iter()
            .filter(|x| {
                !vectors.iter().any(|y| {
                    y != *x
                        && self.vars.iter().all(|(n, _)| {
                            let (a, b) = (y[n], x[n]);
                            if self.reverse { a >= b } else { a <= b }
                        })
                })
            })
            .cloned()
            .collect()
    }

    /// Label-wise intersection with another family from the same manager.
    /// Panics unless `other` came from the same forest as `self`.
    ///
    /// Handles carry only a node id, and two managers number their nodes the same
    /// way — so mixing them would not fail, it would compute on the wrong diagram
    /// and return a plausible wrong answer. The Python layer in `relibmss` has
    /// always checked this; Rust callers were unprotected.
    #[track_caller]
    fn assert_same_forest(&self, other: &Self) {
        assert!(
            Weak::ptr_eq(&self.parent, &other.parent),
            "these {} handles come from different managers; a node id is only \
             meaningful in the forest that created it",
            "ZmddNode"
        );
    }

    pub fn intersect(&self, other: &ZmddNode<V>) -> ZmddNode<V> {
        self.assert_same_forest(other);
        let zmdd = self.parent.upgrade().unwrap();
        let result = zmdd.borrow_mut().intersect(self.node, other.node);
        self.rewrap(&zmdd, result)
    }

    /// Label-wise difference (`self − other`).
    pub fn setdiff(&self, other: &ZmddNode<V>) -> ZmddNode<V> {
        self.assert_same_forest(other);
        let zmdd = self.parent.upgrade().unwrap();
        let result = zmdd.borrow_mut().setdiff(self.node, other.node);
        self.rewrap(&zmdd, result)
    }

    /// Number of vectors whose terminal label is in `ss` (the strata selected by `ss`, see
    /// [`extract`](Self::extract) — this counts the baseline member too).
    pub fn count(&self, ss: &HashSet<V>) -> u64 {
        let mgr = self.parent.upgrade().unwrap();
        let dd = mgr.borrow();
        let mut cache = BddHashMap::default();
        zmdd_count(&dd, self.node, ss, &mut cache)
    }

    /// Graphviz source for this family's diagram. Edge labels are the raw edge indices; for
    /// a `reverse` (cut) family `extract` reports `edge_num-1 - d`, but the graph is raw.
    /// The `Undet` terminal (the empty family) and the edges into it are omitted.
    pub fn dot(&self) -> String {
        let mgr = self.parent.upgrade().unwrap();
        let dd = mgr.borrow();
        dd.dot_string(&self.node)
    }

    /// Enumerate the vectors whose terminal label is in `ss`.
    ///
    /// Vectors are **dense**: every variable of the structure function is present, with the
    /// components the diagram does not record filled in at their baseline — state `0` for a
    /// path family, the max state for a cut family (see [`is_cut`](Self::is_cut)). The
    /// variable set is the one the MDD manager held when `minpath`/`mincut` ran, so a variable
    /// irrelevant to `φ` is reported at its baseline rather than omitted.
    ///
    /// A vector is filed under the label equal to **its own `φ(x)`**, so `extract([v])` is
    /// `minimal{x : φ(x) == v}` (paths) / `maximal{x : φ(x) == v}` (cuts). For the classical
    /// `>= v` / `<= v` reading use [`extract_level`](Self::extract_level).
    pub fn extract(&self, ss: &HashSet<V>) -> ZmddPath<V> {
        ZmddPath::new(self.clone(), ss)
    }
}

impl<V> Clone for ZmddNode<V>
where
    V: MddValue,
{
    fn clone(&self) -> Self {
        ZmddNode::from_weak(
            self.parent.clone(),
            self.gc.clone(),
            self.node,
            self.reverse,
            self.vars.clone(),
        )
    }
}

impl<V> Drop for ZmddNode<V> {
    fn drop(&mut self) {
        if let Some(g) = self.gc.upgrade() {
            let mut s = g.borrow_mut();
            if let Some(c) = s.roots.get_mut(&self.node) {
                *c -= 1;
                if *c == 0 {
                    s.roots.remove(&self.node);
                }
            }
        }
    }
}

fn zmdd_count<V>(
    dd: &ZmddManager<V>,
    node: NodeId,
    ss: &HashSet<V>,
    cache: &mut BddHashMap<NodeId, u64>,
) -> u64
where
    V: MddValue,
{
    if let Some(&c) = cache.get(&node) {
        return c;
    }
    let r = match dd.get_node(&node).unwrap() {
        ZNode::Undet => 0,
        ZNode::Terminal(t) => {
            if ss.contains(&t.value()) {
                1
            } else {
                0
            }
        }
        ZNode::NonTerminal(f) => {
            let edges: Vec<NodeId> = f.iter().collect();
            edges.into_iter().map(|e| zmdd_count(dd, e, ss, cache)).sum()
        }
    };
    cache.insert(node, r);
    r
}

enum SV {
    Node(NodeId),
    Push(String, usize),
    Pop(String),
}

/// Enumerates the sparse vectors of a [`ZmddNode`] family (non-baseline components only; the
/// 0-edge records nothing). For a **path** family a recorded edge `i` is the state `i`
/// (unlisted = 0); for a **cut** family (`reverse`) it is the true state `edge_num-1 - i`
/// (unlisted = the variable's max state).
pub struct ZmddPath<V> {
    stack: Vec<SV>,
    path: HashMap<String, usize>,
    /// The baseline value of every variable: `0` for a path family, `states-1` for a cut
    /// family. `path` starts here and a component reverts to it when the walk backtracks.
    baseline: HashMap<String, usize>,
    node: ZmddNode<V>,
    ss: HashSet<V>,
    reverse: bool,
}

impl<V> ZmddPath<V>
where
    V: MddValue,
{
    fn new(node: ZmddNode<V>, ss: &HashSet<V>) -> Self {
        let mut stack = Vec::new();
        stack.push(SV::Node(node.get_id()));
        let reverse = node.reverse;
        let baseline: HashMap<String, usize> = node
            .vars
            .iter()
            .map(|(n, states)| (n.clone(), if reverse { states - 1 } else { 0 }))
            .collect();
        ZmddPath {
            stack,
            path: baseline.clone(),
            baseline,
            node,
            ss: ss.clone(),
            reverse,
        }
    }
}

impl<V> Iterator for ZmddPath<V>
where
    V: MddValue,
{
    type Item = HashMap<String, usize>;

    fn next(&mut self) -> Option<Self::Item> {
        let dd = self.node.get_mgr();
        while let Some(sv) = self.stack.pop() {
            match sv {
                SV::Node(id) => match dd.borrow().get_node(&id).unwrap() {
                    ZNode::Undet => (),
                    ZNode::Terminal(t) => {
                        if self.ss.contains(&t.value()) {
                            return Some(self.path.clone());
                        }
                    }
                    ZNode::NonTerminal(f) => {
                        let label = dd.borrow().label(&id).unwrap().to_string();
                        let edges: Vec<NodeId> = f.iter().collect();
                        let last = edges.len() - 1;
                        for (i, e) in edges.into_iter().enumerate().rev() {
                            if i == 0 {
                                self.stack.push(SV::Node(e)); // 0-edge (baseline) records nothing
                            } else {
                                // path: state = i; cut (reverse): true state = last - i.
                                let state = if self.reverse { last - i } else { i };
                                self.stack.push(SV::Pop(label.clone()));
                                self.stack.push(SV::Node(e));
                                self.stack.push(SV::Push(label.clone(), state));
                            }
                        }
                    }
                },
                SV::Push(x, i) => {
                    self.path.insert(x, i);
                }
                SV::Pop(x) => {
                    // Restore the baseline rather than dropping the key: every vector is
                    // reported dense, with each unrecorded component at its baseline.
                    match self.baseline.get(&x) {
                        Some(&b) => self.path.insert(x, b),
                        None => self.path.remove(&x),
                    };
                }
            }
        }
        None
    }
}
