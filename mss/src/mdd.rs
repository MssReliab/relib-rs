use mddcore::prelude::*;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::{Add, Mul, Sub};
use std::rc::{Rc, Weak};

use crate::mdd_prob;
use crate::mdd_count;
use crate::mdd_path::MddPath;

/// Minimum live-node count at which automatic gc may fire.
const GC_FLOOR: usize = 1 << 16;

/// State shared between an `MddMgr` and all of its `MddNode` handles, enabling
/// reference-counted gc roots. Keyed by the tagged `Node` (the value and bool
/// sub-forests have independent id spaces). Not generic over `V`.
#[derive(Debug)]
struct GcState {
    roots: BddHashMap<Node, u32>,
    /// Auto-gc fires once live occupancy reaches this; re-armed to 2x the
    /// surviving live set (but never below `floor`) after each collection.
    threshold: usize,
    floor: usize,
}

/// Fire a garbage collection if live occupancy has reached the threshold.
/// Must be called only when no `MtMdd2Manager` borrow is held.
fn maybe_gc<V>(mdd: &Rc<RefCell<MtMdd2Manager<V>>>, gc: &Rc<RefCell<GcState>>)
where
    V: MddValue,
{
    if mdd.borrow().live_node_count() < gc.borrow().threshold {
        return;
    }
    let roots: Vec<Node> = gc.borrow().roots.keys().copied().collect();
    let live = {
        let mut m = mdd.borrow_mut();
        m.gc(&roots);
        m.live_node_count()
    };
    let mut s = gc.borrow_mut();
    s.threshold = live.saturating_mul(2).max(s.floor);
}

/// Manager (forest owner) for building and analyzing multi-state structure functions.
///
/// Wraps the arena-based `MtMdd2Manager<V>` in `Rc<RefCell<..>>` and hands out
/// [`MddNode`] handles. Create variables with [`MddMgr::defvar`], build expressions with
/// [`MddMgr::rpn`] or the node operators, then evaluate with methods such as
/// [`MddNode::prob`] or [`MddNode::mdd_count`] (minimal path vectors live on
/// [`MssMgr::minpath`](crate::mss::MssMgr::minpath)).
pub struct MddMgr<V> {
    mdd: Rc<RefCell<MtMdd2Manager<V>>>,
    gc: Rc<RefCell<GcState>>,
    vars: HashMap<String, MddNode<V>>,
}

/// A handle to a node in an [`MddMgr`]'s forest.
///
/// Holds a `Weak` back-reference to the manager plus the node id, and acts as a gc root
/// while alive. Supports value-style operators (`add`/`mul`/`min`/`max`/…) and analysis
/// methods (`prob`, `minpath`, `mdd_count`, …).
#[derive(Debug)]
pub struct MddNode<V> {
    parent: Weak<RefCell<MtMdd2Manager<V>>>,
    gc: Weak<RefCell<GcState>>,
    node: Node,
}

impl<V> MddNode<V> {
    fn from_weak(
        parent: Weak<RefCell<MtMdd2Manager<V>>>,
        gc: Weak<RefCell<GcState>>,
        node: Node,
    ) -> Self {
        if let Some(g) = gc.upgrade() {
            *g.borrow_mut().roots.entry(node).or_insert(0) += 1;
        }
        MddNode { parent, gc, node }
    }

    fn new(parent: &Rc<RefCell<MtMdd2Manager<V>>>, gc: &Rc<RefCell<GcState>>, node: Node) -> Self {
        Self::from_weak(Rc::downgrade(parent), Rc::downgrade(gc), node)
    }
}

impl<V> Clone for MddNode<V> {
    fn clone(&self) -> Self {
        MddNode::from_weak(self.parent.clone(), self.gc.clone(), self.node)
    }
}

impl<V> Drop for MddNode<V> {
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

impl<V> MddNode<V>
where
    V: MddValue,
{
    /// Wrap a node freshly computed by an op on `self`, then let the collector
    /// run (safe here: no manager borrow is held, and the result is pinned).
    fn rewrap(&self, mdd: &Rc<RefCell<MtMdd2Manager<V>>>, node: Node) -> Self {
        let n = MddNode::from_weak(self.parent.clone(), self.gc.clone(), node);
        if let Some(gc) = self.gc.upgrade() {
            maybe_gc(mdd, &gc);
        }
        n
    }
}

impl<V> MddMgr<V>
where
    V: MddValue,
{
    pub fn new() -> Self {
        MddMgr {
            mdd: Rc::new(RefCell::new(MtMdd2Manager::new())),
            gc: Rc::new(RefCell::new(GcState {
                roots: BddHashMap::default(),
                threshold: GC_FLOOR,
                floor: GC_FLOOR,
            })),
            vars: HashMap::new(),
        }
    }

    pub fn size(&self) -> (usize, usize, usize, usize) {
        self.mdd.borrow().size()
    }

    /// Live-node count at which automatic gc fires (for tuning / tests). The
    /// collector re-arms to 2x the surviving live set after each run, but never
    /// below this value.
    pub fn set_gc_threshold(&self, threshold: usize) {
        let mut s = self.gc.borrow_mut();
        s.threshold = threshold;
        s.floor = threshold;
    }

    /// Current number of live (non-reclaimed) nodes across both sub-forests.
    pub fn live_node_count(&self) -> usize {
        self.mdd.borrow().live_node_count()
    }

    /// Wrap a freshly produced node into a pinned handle and give the collector
    /// a chance to run. Call only with no `MtMdd2Manager` borrow held.
    fn wrap(&self, node: Node) -> MddNode<V> {
        let n = MddNode::new(&self.mdd, &self.gc, node);
        maybe_gc(&self.mdd, &self.gc);
        n
    }

    /// Garbage-collect the underlying MtMdd2 forest.
    ///
    /// Keeps every defined variable plus the supplied `keep` nodes (and
    /// everything reachable from them); reclaims the rest. Returns reclaimed
    /// slots as `(value_forest, bool_forest)`. The collector does not compact,
    /// so kept nodes stay valid — but any `MddNode` not covered by `keep` (nor a
    /// variable / descendant of a kept node) must no longer be used.
    pub fn gc(&self, keep: &[&MddNode<V>]) -> (usize, usize) {
        // All live handles (including variables) are pinned roots already; the
        // explicit `keep` is accepted for API symmetry but is redundant.
        let mut roots: Vec<Node> = self.gc.borrow().roots.keys().copied().collect();
        roots.extend(keep.iter().map(|n| n.node));
        self.mdd.borrow_mut().gc(&roots)
    }

    pub fn boolean(&self, other: bool) -> MddNode<V> {
        let node = {
            let mdd = self.mdd.borrow();
            if other {
                mdd.one()
            } else {
                mdd.zero()
            }
        };
        self.wrap(node)
    }

    pub fn value(&self, value: V) -> MddNode<V> {
        let node = self.mdd.borrow_mut().value(value);
        self.wrap(node)
    }

    pub fn undet_boolean(&self) -> MddNode<V> {
        let node = self.mdd.borrow().undet_boolean();
        self.wrap(node)
    }

    pub fn undet_value(&self) -> MddNode<V> {
        let node = self.mdd.borrow().undet_value();
        self.wrap(node)
    }

    pub fn create_node(&self, h: HeaderId, nodes: &[MddNode<V>]) -> MddNode<V> {
        let xs = nodes.iter().map(|x| x.node).collect::<Vec<_>>();
        let node = self.mdd.borrow_mut().create_node(h, &xs);
        self.wrap(node)
    }

    pub fn defvar(&mut self, label: &str, range: usize) -> MddNode<V> {
        if let Some(node) = self.vars.get(label) {
            return node.clone();
        }
        let level = self.vars.len();
        let node = {
            let mut mdd = self.mdd.borrow_mut();
            let nodes = (0..range).map(|x| mdd.value(V::from(x as i32))).collect::<Vec<_>>();
            let h = mdd.create_header(level, label, range);
            mdd.create_node(h, &nodes)
        };
        // Variables stay alive for the manager's lifetime via a pinned handle.
        let result = MddNode::new(&self.mdd, &self.gc, node);
        self.vars.insert(label.to_string(), result.clone());
        result
    }

    pub fn get_varorder(&self) -> Vec<(String, usize)> {
        let mut result = vec![("?".to_string(), 0); self.vars.len()];
        for (k, v) in self.vars.iter() {
            let headerid = v.get_header().unwrap();
            let mdd = self.mdd.borrow();
            let header = mdd.mtmdd().get_header(&headerid).unwrap();
            let level = header.level() as usize;
            result[level] = (k.clone(), header.edge_num());
        }
        result
    }

    /// Builds an MTMDD2 node from an expression in Reverse Polish Notation.
    ///
    /// This is the main entry point for building a diagram from a string, and it is what
    /// the [`relibmss`](https://github.com/MssReliab/relibmss) Python layer calls. Tokens
    /// are separated by whitespace and consumed left to right against a stack.
    ///
    /// `vars` maps each variable name to its **number of states**; a variable must appear
    /// in `vars` before it can be used. Nodes are tagged as either boolean or value
    /// (`MtMdd2` composes both), so arithmetic and comparison operators produce different
    /// node kinds — comparisons take values and yield booleans.
    ///
    /// # Grammar
    ///
    /// | Token | Arity | Meaning |
    /// |---|---|---|
    /// | `True`, `False` | — | boolean constants |
    /// | `+`, `-`, `*`, `/` | 2 | arithmetic on values |
    /// | `min`, `max` | 2 | minimum / maximum of two values |
    /// | `==`, `!=`, `<`, `<=`, `>`, `>=` | 2 | comparison; values in, boolean out |
    /// | `&&`, `\|\|` | 2 | boolean and / or |
    /// | `!` | 1 | boolean not |
    /// | `?` | 3 | if-then-else: `cond then else ?` |
    /// | `save(id)` | — | remember the top of the stack under `id` (does not pop) |
    /// | `load(id)` | — | push the node previously saved as `id` |
    ///
    /// Any other token is parsed as an **integer literal** if it parses as `i32`,
    /// otherwise as a **variable name** looked up in `vars`.
    ///
    /// `save` / `load` let a shared subexpression be written once and reused, so the
    /// expression is a DAG rather than a tree.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `load(id)` names something never saved, if `save(id)` is used on
    /// an empty stack, or if the expression does not reduce to exactly one node.
    ///
    /// # Panics
    ///
    /// Panics if a token is neither an integer nor a key of `vars`.
    ///
    /// # Example
    ///
    /// ```
    /// use mss::prelude::*;
    /// use std::collections::HashMap;
    ///
    /// let mut mgr: MddMgr<i32> = MddMgr::new();
    /// let mut vars = HashMap::new();
    /// vars.insert("x".to_string(), 3usize); // x has states 0, 1, 2
    /// vars.insert("y".to_string(), 3usize);
    ///
    /// // system state is min(x, y); is it at least 1?
    /// let node = mgr.rpn("x y min 1 >=", &vars).unwrap();
    /// assert!(node.is_boolean());
    /// ```
    pub fn rpn(&mut self, rpn: &str, vars: &HashMap<String, usize>) -> Result<MddNode<V>, String> {
        let mut stack = Vec::new();
        let mut cache = HashMap::new();
        for token in rpn.split_whitespace() {
            match token {
                "+" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.add(a, b);
                    stack.push(tmp);
                }
                "-" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.sub(a, b);
                    stack.push(tmp);
                }
                "*" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.mul(a, b);
                    stack.push(tmp);
                }
                "/" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.div(a, b);
                    stack.push(tmp);
                }
                "min" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.min(a, b);
                    stack.push(tmp);
                }
                "max" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.max(a, b);
                    stack.push(tmp);
                }
                "==" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.eq(a, b);
                    stack.push(tmp);
                }
                "!=" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.neq(a, b);
                    stack.push(tmp);
                }
                "<" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.lt(a, b);
                    stack.push(tmp);
                }
                "<=" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.lte(a, b);
                    stack.push(tmp);
                }
                ">" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.gt(a, b);
                    stack.push(tmp);
                }
                ">=" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.gte(a, b);
                    stack.push(tmp);
                }
                "&&" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.and(a, b);
                    stack.push(tmp);
                }
                "||" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.or(a, b);
                    stack.push(tmp);
                }
                "!" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.not(a);
                    stack.push(tmp);
                }
                "?" => {
                    let mut mdd = self.mdd.borrow_mut();
                    let c = stack.pop().unwrap();
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let tmp = mdd.ite(a, b, c);
                    stack.push(tmp);
                }
                "True" => {
                    let node = {
                        let mdd = self.mdd.borrow();
                        mdd.one()
                    };
                    stack.push(node);
                }
                "False" => {
                    let node = {
                        let mdd = self.mdd.borrow();
                        mdd.zero()
                    };
                    stack.push(node);
                }
                _ if token.starts_with("save(") && token.ends_with(")") => {
                    let name = &token[5..token.len() - 1];
                    if let Some(node) = stack.last() {
                        cache.insert(name.to_string(), node.clone());
                    } else {
                        return Err("Stack is empty for save operation".to_string());
                    }
                }
                _ if token.starts_with("load(") && token.ends_with(")") => {
                    let name = &token[5..token.len() - 1];
                    if let Some(node) = cache.get(name) {
                        stack.push(node.clone());
                    } else {
                        return Err(format!("No cached value for {}", name));
                    }
                }
                _ => {
                    // parse whether it is a number or a variable
                    match token.parse::<i32>() {
                        Ok(val) => {
                            let node = {
                                let mut mdd = self.mdd.borrow_mut();
                                mdd.value(V::from(val))
                            };
                            stack.push(node);
                        }
                        Err(_) => match vars.get(token) {
                            Some(range) => {
                                let node = self.defvar(token, range.clone());
                                stack.push(node.node.clone());
                            }
                            None => panic!("Unknown variable: {}", token),
                        },
                    }
                }
            }
        }
        if stack.len() == 1 {
            Ok(self.wrap(stack.pop().unwrap()))
        } else {
            Err("Invalid expression".to_string())
        }
    }

    pub fn and(&self, nodes: &[MddNode<V>]) -> MddNode<V> {
        let result = {
            let mut mdd = self.mdd.borrow_mut();
            let mut result = mdd.one();
            for x in nodes {
                result = mdd.and(result, x.node);
            }
            result
        };
        self.wrap(result)
    }

    pub fn or(&self, nodes: &[MddNode<V>]) -> MddNode<V> {
        let result = {
            let mut mdd = self.mdd.borrow_mut();
            let mut result = mdd.zero();
            for x in nodes {
                result = mdd.or(result, x.node);
            }
            result
        };
        self.wrap(result)
    }

    pub fn min(&self, nodes: &[MddNode<V>]) -> MddNode<V> {
        let result = {
            let mut mdd = self.mdd.borrow_mut();
            let mut result = nodes[0].node;
            for x in &nodes[1..] {
                result = mdd.min(result, x.node);
            }
            result
        };
        self.wrap(result)
    }

    pub fn max(&self, nodes: &[MddNode<V>]) -> MddNode<V> {
        let result = {
            let mut mdd = self.mdd.borrow_mut();
            let mut result = nodes[0].node;
            for x in &nodes[1..] {
                result = mdd.max(result, x.node);
            }
            result
        };
        self.wrap(result)
    }

    pub fn clear_cache(&mut self) {
        let mut mdd = self.mdd.borrow_mut();
        mdd.clear_cache();
    }
}

impl<V> MddNode<V>
where
    V: MddValue,
{
    pub fn get_mgr(&self) -> Rc<RefCell<MtMdd2Manager<V>>> {
        self.parent.upgrade().unwrap()
    }

    pub fn get_node(&self) -> Node {
        self.node.clone()
    }

    pub fn get_id(&self) -> NodeId {
        match &self.node {
            Node::Value(x) => *x,
            Node::Bool(x) => *x,
        }
    }

    pub fn get_id2(&self) -> (NodeId, NodeId) {
        match &self.node {
            Node::Value(x) => (*x, 0),
            Node::Bool(x) => (0, *x),
        }
    }

    pub fn get_header(&self) -> Option<HeaderId> {
        match &self.node {
            Node::Value(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mtmdd().get_node(x)?;
                node.headerid()
            }
            Node::Bool(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mdd().get_node(x)?;
                node.headerid()
            }
        }
    }

    pub fn get_level(&self) -> Option<Level> {
        match &self.node {
            Node::Value(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mtmdd().get_node(x)?;
                let hid = node.headerid()?;
                let header = mdd.mtmdd().get_header(&hid)?;
                Some(header.level())
            }
            Node::Bool(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mdd().get_node(x)?;
                let hid = node.headerid()?;
                let header = mdd.mdd().get_header(&hid)?;
                Some(header.level())
            }
        }
    }

    pub fn get_label(&self) -> Option<String> {
        match &self.node {
            Node::Value(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mtmdd().get_node(x)?;
                let hid = node.headerid()?;
                let header = mdd.mtmdd().get_header(&hid)?;
                Some(header.label().to_string())
            }
            Node::Bool(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mdd().get_node(x)?;
                let hid = node.headerid()?;
                let header = mdd.mdd().get_header(&hid)?;
                Some(header.label().to_string())
            }
        }
    }

    pub fn get_children(&self) -> Option<Vec<MddNode<V>>> {
        match &self.node {
            Node::Value(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mtmdd().get_node(x).unwrap();
                match node {
                    mtmdd::Node::Terminal(_) | mtmdd::Node::Undet => None,
                    mtmdd::Node::NonTerminal(fnode) => {
                        // A borrow is held here, so pin only (no maybe_gc).
                        Some(
                            fnode
                                .iter()
                                .map(|id| {
                                    MddNode::from_weak(
                                        self.parent.clone(),
                                        self.gc.clone(),
                                        Node::Value(id),
                                    )
                                })
                                .collect(),
                        )
                    }
                }
            }
            Node::Bool(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mdd().get_node(x).unwrap();
                match node {
                    mdd::Node::One | mdd::Node::Zero | mdd::Node::Undet => None,
                    mdd::Node::NonTerminal(fnode) => {
                        // A borrow is held here, so pin only (no maybe_gc).
                        Some(
                            fnode
                                .iter()
                                .map(|id| {
                                    MddNode::from_weak(
                                        self.parent.clone(),
                                        self.gc.clone(),
                                        Node::Bool(id),
                                    )
                                })
                                .collect(),
                        )
                    }
                }
            }
        }
    }

    pub fn is_boolean(&self) -> bool {
        match &self.node {
            Node::Value(_) => false,
            Node::Bool(_) => true,
        }
    }

    pub fn is_value(&self) -> bool {
        match &self.node {
            Node::Value(_) => true,
            Node::Bool(_) => false,
        }
    }

    pub fn is_zero(&self) -> bool {
        match &self.node {
            Node::Value(_) => false,
            Node::Bool(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mdd().get_node(x).unwrap();
                match node {
                    mdd::Node::One => false,
                    mdd::Node::Zero => true,
                    mdd::Node::Undet => false,
                    _ => false,
                }
            }
        }
    }

    pub fn is_one(&self) -> bool {
        match &self.node {
            Node::Value(_) => false,
            Node::Bool(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mdd().get_node(x).unwrap();
                match node {
                    mdd::Node::One => true,
                    mdd::Node::Zero => false,
                    mdd::Node::Undet => false,
                    _ => false,
                }
            }
        }
    }

    pub fn is_undet(&self) -> bool {
        match &self.node {
            Node::Value(_) => false,
            Node::Bool(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mdd().get_node(x).unwrap();
                match node {
                    mdd::Node::One => false,
                    mdd::Node::Zero => false,
                    mdd::Node::Undet => true,
                    _ => false,
                }
            }
        }
    }

    pub fn value(&self) -> Option<V> {
        match &self.node {
            Node::Value(x) => {
                let mddmgr = self.parent.upgrade().unwrap();
                let mdd = mddmgr.borrow();
                let node = mdd.mtmdd().get_node(x).unwrap();
                match node {
                    mtmdd::Node::Terminal(fnode) => Some(fnode.value()),
                    _ => None,
                }
            }
            Node::Bool(_) => None,
        }
    }

    pub fn dot(&self) -> String {
        let mddmgr = self.parent.upgrade().unwrap();
        let mdd = mddmgr.borrow();
        mdd.dot_string(&self.node)
    }

    pub fn add(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.add(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn sub(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.sub(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn mul(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.mul(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn div(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.div(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn min(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.min(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn max(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.max(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn eq(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.eq(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn ne(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.neq(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn lt(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.lt(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn le(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.lte(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn gt(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.gt(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn ge(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.gte(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn and(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.and(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn or(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.or(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn xor(&self, other: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.xor(self.node, other.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn not(&self) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.not(self.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn ite(&self, then: &MddNode<V>, els: &MddNode<V>) -> MddNode<V> {
        let mddmgr = self.parent.upgrade().unwrap();
        let mut mdd = mddmgr.borrow_mut();
        let node = mdd.ite(self.node, then.node, els.node);
        drop(mdd);
        self.rewrap(&mddmgr, node)
    }

    pub fn prob<T>(&mut self, pv: &HashMap<String, Vec<T>>, ss: &[V]) -> T
    where
        T: Add<Output = T>
            + Sub<Output = T>
            + Mul<Output = T>
            + Clone
            + Copy
            + PartialEq
            + From<f64>,
    {
        let mgr = self.parent.upgrade().unwrap();
        let mut mdd = mgr.borrow_mut();
        let hashset: HashSet<V> = ss.iter().cloned().collect();
        mdd_prob::prob(&mut mdd, &self.node, pv, &hashset)
    }

    /// Multi-state Birnbaum importance of every variable for the success set `ss`, as
    /// `{var: [D_1, …, D_{M-1}]}` where `M` is the variable's state count and
    /// `D_j = P(φ∈ss | var = j) − P(φ∈ss | var = j−1)` is the importance of raising the
    /// component across the `j−1 → j` state boundary (index `d` = transition `d → d+1`).
    /// For a binary variable this is the single classic Birnbaum measure
    /// `P(φ∈ss | var=1) − P(φ∈ss | var=0)`. Computed in one backward-differentiation pass
    /// (see [`crate::mdd_prob::bmeas`]).
    ///
    /// With an interval type `T` (imprecise probabilities) the result is a *guaranteed but
    /// conservative* enclosure — see [`crate::mdd_prob::bmeas`] for the caveats (dependency
    /// problem, worst-case difference subtraction, `Σ_j p = 1` not enforced).
    pub fn bmeas<T>(&mut self, pv: &HashMap<String, Vec<T>>, ss: &[V]) -> HashMap<String, Vec<T>>
    where
        T: Add<Output = T>
            + Sub<Output = T>
            + Mul<Output = T>
            + Clone
            + Copy
            + PartialEq
            + From<f64>,
    {
        let mgr = self.parent.upgrade().unwrap();
        let mut mdd = mgr.borrow_mut();
        let hashset: HashSet<V> = ss.iter().cloned().collect();
        mdd_prob::bmeas(&mut mdd, &self.node, pv, &hashset)
    }

    // `minpath` lives on [`MssMgr`](crate::mss::MssMgr) (it also needs a `ZmddMgr`);
    // it returns a genuine `ZmddNode` set family.

    pub fn mdd_count(&self, ss: &HashSet<V>) -> u64 {
        let mgr = self.parent.upgrade().unwrap();
        let mdd = mgr.borrow();
        mdd_count::mdd_count(&mdd, &self.node, ss)
    }

    pub fn mdd_extract(&self, ss: &HashSet<V>) -> MddPath<V> {
        MddPath::new(self, ss)
    }

    pub fn size(&self) -> (u64, u64, u64) {
        let mgr = self.parent.upgrade().unwrap();
        let mdd = mgr.borrow();
        mdd_count::mddnode_count(&mdd, &self.node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
