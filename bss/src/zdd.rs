use bddcore::prelude::*;
use crate::zdd_count;
use crate::zdd_path::ZddPath;

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::rc::Weak;

/// Minimum live-node count at which automatic gc may fire.
const GC_FLOOR: usize = 1 << 16;

/// gc-root bookkeeping shared between a [`ZddMgr`] and its [`ZddNode`] handles
/// (mirrors the `BddMgr` machinery): every live handle pins its node here.
#[derive(Debug)]
struct GcState {
    roots: BddHashMap<NodeId, u32>,
    threshold: usize,
    floor: usize,
}

fn maybe_gc(zdd: &Rc<RefCell<ZddManager>>, gc: &Rc<RefCell<GcState>>) {
    if zdd.borrow().live_node_count() < gc.borrow().threshold {
        return;
    }
    let roots: Vec<NodeId> = gc.borrow().roots.keys().copied().collect();
    let live = {
        let mut z = zdd.borrow_mut();
        z.gc(&roots);
        z.live_node_count()
    };
    let mut s = gc.borrow_mut();
    s.threshold = live.saturating_mul(2).max(s.floor);
}

/// Manager (forest owner) for **set families** (zero-suppressed decision diagrams).
///
/// Used two ways: (1) owned by a [`BssMgr`](crate::bss::BssMgr) as the forest that
/// [`minpath`](crate::bss::BssMgr::minpath) / [`mincut`](crate::bss::BssMgr::mincut) results
/// live in; (2) standalone, to build and manipulate set families directly with
/// [`empty`](Self::empty) / [`base`](Self::base) / [`singleton`](Self::singleton) /
/// [`from_sets`](Self::from_sets). Either way it hands out [`ZddNode`] handles supporting the
/// set algebra (`union`/`intersect`/`setdiff`/`product`/`divide`). There is no public
/// "convert a BDD to a ZDD" entry point.
pub struct ZddMgr {
    zdd: Rc<RefCell<ZddManager>>,
    gc: Rc<RefCell<GcState>>,
    /// Element label -> header id, so `singleton`/`from_sets` reuse one header (and one
    /// level, assigned in order of first appearance) per element name.
    vars: HashMap<String, HeaderId>,
}

impl ZddMgr {
    pub fn new() -> Self {
        ZddMgr {
            zdd: Rc::new(RefCell::new(ZddManager::new())),
            gc: Rc::new(RefCell::new(GcState {
                roots: BddHashMap::default(),
                threshold: GC_FLOOR,
                floor: GC_FLOOR,
            })),
            vars: HashMap::default(),
        }
    }

    /// The empty family `∅` (no sets at all).
    pub fn empty(&self) -> ZddNode {
        self.zero()
    }

    /// The unit family `{∅}` (containing just the empty set) — the identity for `product`.
    pub fn base(&self) -> ZddNode {
        self.one()
    }

    /// The singleton family `{{label}}`. The element's header is created on first use
    /// (levels are assigned in order of first appearance) and reused afterwards.
    pub fn singleton(&mut self, label: &str) -> ZddNode {
        let hid = match self.vars.get(label) {
            Some(&h) => h,
            None => {
                let level = self.vars.len();
                let h = self.zdd.borrow_mut().create_header(level, label);
                self.vars.insert(label.to_string(), h);
                h
            }
        };
        let node = {
            let mut zdd = self.zdd.borrow_mut();
            let (z, o) = (zdd.zero(), zdd.one());
            zdd.create_node(hid, z, o)
        };
        self.wrap(node)
    }

    /// Build a family from an explicit list of sets, e.g. `[["x","y"], ["z"], []]`
    /// → `{ {x,y}, {z}, ∅ }`. Each set is the `product` of its element singletons
    /// (starting from [`base`](Self::base)); the sets are combined with `union`.
    pub fn from_sets(&mut self, sets: &[Vec<String>]) -> ZddNode {
        let mut result = self.empty();
        for s in sets {
            let mut fam = self.base();
            for elem in s {
                let single = self.singleton(elem);
                fam = fam.product(&single);
            }
            result = result.union(&fam);
        }
        result
    }

    /// The underlying arena (crate-internal: used by [`BssMgr`](crate::bss::BssMgr) to convert a
    /// minsol result into this manager).
    pub(crate) fn arena(&self) -> &Rc<RefCell<ZddManager>> {
        &self.zdd
    }

    /// Wrap a freshly produced node into a pinned handle; may run gc (no borrow held).
    pub(crate) fn wrap(&self, node: NodeId) -> ZddNode {
        let n = ZddNode::new(&self.zdd, &self.gc, node);
        maybe_gc(&self.zdd, &self.gc);
        n
    }

    pub fn set_gc_threshold(&self, threshold: usize) {
        let mut s = self.gc.borrow_mut();
        s.threshold = threshold;
        s.floor = threshold;
    }

    pub fn live_node_count(&self) -> usize {
        self.zdd.borrow().live_node_count()
    }

    pub fn size(&self) -> (usize, usize, usize) {
        self.zdd.borrow().size()
    }

    pub fn zero(&self) -> ZddNode {
        let z = self.zdd.borrow().zero();
        self.wrap(z)
    }

    pub fn one(&self) -> ZddNode {
        let o = self.zdd.borrow().one();
        self.wrap(o)
    }

    pub fn clear_cache(&mut self) {
        self.zdd.borrow_mut().clear_cache();
    }
}

/// A handle to a set family in a [`ZddMgr`]'s forest.
#[derive(Debug)]
pub struct ZddNode {
    parent: Weak<RefCell<ZddManager>>,
    gc: Weak<RefCell<GcState>>,
    node: NodeId,
}

impl ZddNode {
    fn new(zdd: &Rc<RefCell<ZddManager>>, gc: &Rc<RefCell<GcState>>, node: NodeId) -> Self {
        Self::from_weak(Rc::downgrade(zdd), Rc::downgrade(gc), node)
    }

    fn from_weak(
        parent: Weak<RefCell<ZddManager>>,
        gc: Weak<RefCell<GcState>>,
        node: NodeId,
    ) -> Self {
        if let Some(g) = gc.upgrade() {
            *g.borrow_mut().roots.entry(node).or_insert(0) += 1;
        }
        ZddNode { parent, gc, node }
    }

    fn rewrap(&self, zdd: &Rc<RefCell<ZddManager>>, node: NodeId) -> ZddNode {
        let n = ZddNode::from_weak(self.parent.clone(), self.gc.clone(), node);
        if let Some(gc) = self.gc.upgrade() {
            maybe_gc(zdd, &gc);
        }
        n
    }

    pub fn get_mgr(&self) -> Rc<RefCell<ZddManager>> {
        self.parent.upgrade().unwrap()
    }

    pub fn get_id(&self) -> NodeId {
        self.node
    }

    pub fn get_header(&self) -> Option<HeaderId> {
        let mgr = self.parent.upgrade().unwrap();
        let zdd = mgr.borrow();
        zdd.get_node(&self.node)?.headerid()
    }

    pub fn get_level(&self) -> Option<Level> {
        let mgr = self.parent.upgrade().unwrap();
        let zdd = mgr.borrow();
        let hid = zdd.get_node(&self.node)?.headerid()?;
        Some(zdd.get_header(&hid)?.level())
    }

    pub fn get_label(&self) -> Option<String> {
        let mgr = self.parent.upgrade().unwrap();
        let zdd = mgr.borrow();
        let hid = zdd.get_node(&self.node)?.headerid()?;
        Some(zdd.get_header(&hid)?.label().to_string())
    }

    pub fn get_children(&self) -> Option<(ZddNode, ZddNode)> {
        let mgr = self.parent.upgrade().unwrap();
        let zdd = mgr.borrow();
        match zdd.get_node(&self.node)? {
            Node::Zero | Node::One | Node::Undet => None,
            Node::NonTerminal(fnode) => {
                let f0 = ZddNode::from_weak(self.parent.clone(), self.gc.clone(), fnode.edge(0));
                let f1 = ZddNode::from_weak(self.parent.clone(), self.gc.clone(), fnode.edge(1));
                Some((f0, f1))
            }
        }
    }

    pub fn is_zero(&self) -> bool {
        let mgr = self.parent.upgrade().unwrap();
        let zdd = mgr.borrow();
        matches!(zdd.get_node(&self.node).unwrap(), Node::Zero)
    }

    pub fn is_one(&self) -> bool {
        let mgr = self.parent.upgrade().unwrap();
        let zdd = mgr.borrow();
        matches!(zdd.get_node(&self.node).unwrap(), Node::One)
    }

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
            "ZddNode"
        );
    }

    pub fn eq(&self, other: &ZddNode) -> bool {
        self.assert_same_forest(other);
        self.node == other.node
    }

    /// Graphviz source for this family's diagram. The `0` terminal (the empty family) and the
    /// edges into it are omitted; the 0-edge is still drawn wherever it leads somewhere.
    pub fn dot(&self) -> String {
        let zdd = self.parent.upgrade().unwrap();
        let result = zdd.borrow().dot_string(&self.node);
        result
    }

    /// Union of two set families.
    pub fn union(&self, other: &ZddNode) -> ZddNode {
        self.assert_same_forest(other);
        let zdd = self.parent.upgrade().unwrap();
        let result = zdd.borrow_mut().union(self.node, other.node);
        self.rewrap(&zdd, result)
    }

    /// Intersection of two set families.
    pub fn intersect(&self, other: &ZddNode) -> ZddNode {
        self.assert_same_forest(other);
        let zdd = self.parent.upgrade().unwrap();
        let result = zdd.borrow_mut().intersect(self.node, other.node);
        self.rewrap(&zdd, result)
    }

    /// Set difference (`self \ other`).
    pub fn setdiff(&self, other: &ZddNode) -> ZddNode {
        self.assert_same_forest(other);
        let zdd = self.parent.upgrade().unwrap();
        let result = zdd.borrow_mut().setdiff(self.node, other.node);
        self.rewrap(&zdd, result)
    }

    /// Family product (all pairwise unions of a set from each family).
    pub fn product(&self, other: &ZddNode) -> ZddNode {
        self.assert_same_forest(other);
        let zdd = self.parent.upgrade().unwrap();
        let result = zdd.borrow_mut().product(self.node, other.node);
        self.rewrap(&zdd, result)
    }

    /// Family quotient (`self / other`).
    pub fn divide(&self, other: &ZddNode) -> ZddNode {
        self.assert_same_forest(other);
        let zdd = self.parent.upgrade().unwrap();
        let result = zdd.borrow_mut().divide(self.node, other.node);
        self.rewrap(&zdd, result)
    }

    /// Number of sets in the family (default `ss = [true]`).
    pub fn count(&self, ss: &[bool]) -> u64 {
        let mgr = self.parent.upgrade().unwrap();
        let zdd = mgr.borrow();
        let mut cache = BddHashMap::default();
        zdd_count::zdd_count(&zdd, ss, self.node, &mut cache)
    }

    /// Enumerate the sets of the family as lists of labels.
    pub fn extract(&self, ss: &[bool]) -> ZddPath {
        ZddPath::new(self.clone(), ss)
    }

    pub fn size(&self) -> (u64, u64, u64) {
        let mgr = self.parent.upgrade().unwrap();
        let zdd = mgr.borrow();
        let mut cache = BddHashSet::default();
        let (nn, nv, ne) = zdd_count::node_count(&zdd, self.node, &mut cache);
        (nn, nv, ne - 1)
    }
}

impl Clone for ZddNode {
    fn clone(&self) -> Self {
        ZddNode::from_weak(self.parent.clone(), self.gc.clone(), self.node)
    }
}

impl Drop for ZddNode {
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
