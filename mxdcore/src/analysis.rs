//! Multi-state boundary analysis: the layer most callers should use.
//!
//! [`MxdManager`] is the engine. This is the vocabulary of the problem it was
//! built for — components with states, a structure function, transition relations,
//! and the boundaries between performance levels — so that a case study reads as
//! one and does not have to re-derive level sets and relation families by hand
//! each time.
//!
//! ```
//! use mxdcore::analysis::*;
//!
//! // Three components, three states each. φ is the worst component (a series
//! // system), so the system performs at level j exactly when all components do.
//! let sys = System::new(&[3, 3, 3]);
//! let levels = sys.levels_from_states(|x| *x.iter().min().unwrap());
//! let degrade = sys.degrade();
//!
//! // Transitions that drop the system out of {φ ≥ 2}.
//! let b = degrade.boundary_down(&levels, 2);
//! assert_eq!(b.count(), 3, "one component falls from 2 to 1, the others are at 2");
//! ```
//!
//! # Shape of the API
//!
//! [`System`] owns the forest and builds things; the handles it hands out carry the
//! operations. That is the division `relib-bss` and `relib-mss` use, and the
//! handles work the same way they do there: a handle is a **garbage-collection
//! root while it is alive**, so a collection can never invalidate one you are still
//! holding, and it knows which forest it came from, so mixing two systems panics
//! instead of computing on the wrong one.
//!
//! # Sets and relations do not mix
//!
//! The engine represents both with plain node ids, and its `Zero` / `One` terminals
//! are shared between them — so there a node alone does not say which it is, and
//! `One` read as a relation means the *identity*, not everything. Here they are
//! [`StateSet`] and [`Transitions`], two distinct types, and the compiler keeps
//! them apart.
//!
//! # Costs
//!
//! Nothing enumerates the state space unless you ask it to. The exception is
//! [`System::levels_from_states`], which evaluates φ at every point and is
//! therefore exponential in the number of components — fine into the millions,
//! useless beyond. For larger systems give φ as a fold instead
//! ([`System::levels_from_fold`]), which is linear in the diagram rather than the
//! state space.

use crate::enumerate::{StateVec, Transition};
use crate::minterm::{Dst, Src};
use crate::mxd::MxdManager;
use common::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::{Rc, Weak};

/// Auto-collection does not fire below this many live nodes.
const GC_FLOOR: usize = 1 << 16;

#[derive(Debug)]
struct GcState {
    roots: BddHashMap<NodeId, u32>,
    /// Auto-gc fires once live occupancy reaches this; re-armed to twice the
    /// surviving live set (never below `floor`) after each collection.
    threshold: usize,
    floor: usize,
}

/// Collect if live occupancy has reached the threshold. Must be called with no
/// manager borrow held.
fn maybe_gc(mxd: &Rc<RefCell<MxdManager>>, gc: &Rc<RefCell<GcState>>) {
    if mxd.borrow().live_node_count() < gc.borrow().threshold {
        return;
    }
    let roots: Vec<NodeId> = gc.borrow().roots.keys().copied().collect();
    let live = {
        let mut m = mxd.borrow_mut();
        m.gc(&roots);
        m.live_node_count()
    };
    let mut s = gc.borrow_mut();
    s.threshold = live.saturating_mul(2).max(s.floor);
}

/// Lets the cross-forest check accept either kind of handle.
trait Forested {
    fn forest(&self) -> &Weak<RefCell<MxdManager>>;
}

macro_rules! handle {
    ($name:ident, $what:literal) => {
        #[doc = concat!("A handle denoting ", $what, ".")]
        ///
        /// Holds a `Weak` back-reference to the forest plus the node id, and acts
        /// as a gc root while alive — so a collection cannot invalidate it.
        #[derive(Debug)]
        pub struct $name {
            parent: Weak<RefCell<MxdManager>>,
            gc: Weak<RefCell<GcState>>,
            node: NodeId,
        }

        impl $name {
            fn from_weak(
                parent: Weak<RefCell<MxdManager>>,
                gc: Weak<RefCell<GcState>>,
                node: NodeId,
            ) -> Self {
                if let Some(g) = gc.upgrade() {
                    *g.borrow_mut().roots.entry(node).or_insert(0) += 1;
                }
                Self { parent, gc, node }
            }

            /// Panics unless `other` came from the same forest.
            ///
            /// Handles carry a node id, and two systems number their nodes the
            /// same way — so mixing them would not fail, it would compute on the
            /// wrong forest and return a plausible wrong answer.
            #[track_caller]
            fn same_forest<T: Forested>(&self, other: &T) {
                assert!(
                    Weak::ptr_eq(&self.parent, other.forest()),
                    "this handle and the other come from different Systems; a node \
                     id is only meaningful in the forest that created it"
                );
            }

            fn mgr(&self) -> Rc<RefCell<MxdManager>> {
                self.parent
                    .upgrade()
                    .expect("the System that owns this handle has been dropped")
            }

            fn rewrap(&self, mxd: &Rc<RefCell<MxdManager>>, node: NodeId) -> Self {
                let n = Self::from_weak(self.parent.clone(), self.gc.clone(), node);
                if let Some(gc) = self.gc.upgrade() {
                    maybe_gc(mxd, &gc);
                }
                n
            }

            /// Diagram size: distinct non-terminal nodes reachable from here.
            pub fn node_count(&self) -> usize {
                self.mgr().borrow().node_count(self.node)
            }

            /// Graphviz source.
            pub fn dot(&self) -> String {
                self.mgr().borrow().dot_string(&self.node)
            }
        }

        impl Forested for $name {
            fn forest(&self) -> &Weak<RefCell<MxdManager>> {
                &self.parent
            }
        }

        impl Clone for $name {
            fn clone(&self) -> Self {
                Self::from_weak(self.parent.clone(), self.gc.clone(), self.node)
            }
        }

        impl Drop for $name {
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

        impl PartialEq for $name {
            /// Equal when they denote the same diagram in the same forest.
            /// Diagrams are canonical, so this is exact.
            fn eq(&self, other: &Self) -> bool {
                Weak::ptr_eq(&self.parent, &other.parent) && self.node == other.node
            }
        }

        impl Eq for $name {}
    };
}

handle!(StateSet, "a set of state vectors");
handle!(Transitions, "a relation over state vectors");

impl StateSet {
    /// How many state vectors. Does not enumerate them.
    pub fn count(&self) -> u128 {
        self.mgr().borrow().cardinality_set(self.node)
    }

    /// Every state vector, listed. Exponential — for inspecting small results, not
    /// for measuring large ones; use [`count`](Self::count) for that.
    pub fn vectors(&self) -> Vec<StateVec> {
        self.mgr().borrow().enumerate_set(self.node)
    }

    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    pub fn union(&self, other: &StateSet) -> StateSet {
        self.same_forest(other);
        let m = self.mgr();
        let node = m.borrow_mut().or_set(self.node, other.node);
        self.rewrap(&m, node)
    }

    pub fn intersect(&self, other: &StateSet) -> StateSet {
        self.same_forest(other);
        let m = self.mgr();
        let node = m.borrow_mut().and_set(self.node, other.node);
        self.rewrap(&m, node)
    }

    pub fn difference(&self, other: &StateSet) -> StateSet {
        self.same_forest(other);
        let m = self.mgr();
        let node = m.borrow_mut().setdiff_set(self.node, other.node);
        self.rewrap(&m, node)
    }

    pub fn complement(&self) -> StateSet {
        let m = self.mgr();
        let node = m.borrow_mut().not_set(self.node);
        self.rewrap(&m, node)
    }

    /// The relation `{ (x, y) : x ∈ self, y ∈ other }`, with `|self| · |other|`
    /// transitions.
    pub fn cross(&self, other: &StateSet) -> Transitions {
        self.same_forest(other);
        let m = self.mgr();
        let node = m.borrow_mut().cross(self.node, other.node);
        let t = Transitions::from_weak(self.parent.clone(), self.gc.clone(), node);
        if let Some(gc) = self.gc.upgrade() {
            maybe_gc(&m, &gc);
        }
        t
    }
}

impl Transitions {
    /// How many transitions. Does not enumerate them.
    pub fn count(&self) -> u128 {
        self.mgr().borrow().cardinality_relation(self.node)
    }

    /// Every transition, listed. Same caveat as [`StateSet::vectors`].
    pub fn pairs(&self) -> Vec<Transition> {
        self.mgr().borrow().enumerate_relation(self.node)
    }

    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    pub fn union(&self, other: &Transitions) -> Transitions {
        self.same_forest(other);
        let m = self.mgr();
        let node = m.borrow_mut().or_rel(self.node, other.node);
        self.rewrap(&m, node)
    }

    pub fn intersect(&self, other: &Transitions) -> Transitions {
        self.same_forest(other);
        let m = self.mgr();
        let node = m.borrow_mut().and_rel(self.node, other.node);
        self.rewrap(&m, node)
    }

    pub fn difference(&self, other: &Transitions) -> Transitions {
        self.same_forest(other);
        let m = self.mgr();
        let node = m.borrow_mut().setdiff_rel(self.node, other.node);
        self.rewrap(&m, node)
    }

    pub fn complement(&self) -> Transitions {
        let m = self.mgr();
        let node = m.borrow_mut().not_rel(self.node);
        self.rewrap(&m, node)
    }

    /// The converse relation: every transition reversed.
    pub fn converse(&self) -> Transitions {
        let m = self.mgr();
        let node = m.borrow_mut().transpose(self.node);
        self.rewrap(&m, node)
    }

    /// The boundary operator `B = self ∩ (from × to)`: the transitions that start
    /// in `from` and land in `to`.
    ///
    /// Nothing here assumes φ is monotone. That is the point — it is what lets
    /// repair, restart and recovery be analysed the same way as failure, and it is
    /// why this is defined for systems where minimal path and cut vectors are not.
    pub fn boundary(&self, from: &StateSet, to: &StateSet) -> Transitions {
        self.same_forest(from);
        self.same_forest(to);
        let m = self.mgr();
        let node = m.borrow_mut().boundary(from.node, to.node, self.node);
        self.rewrap(&m, node)
    }

    /// Transitions that carry the system **into** `{φ ≥ j}`.
    pub fn boundary_up(&self, levels: &Levels, j: usize) -> Transitions {
        self.boundary(&levels.lower(j), &levels.upper(j))
    }

    /// Transitions that carry the system **out of** `{φ ≥ j}`.
    pub fn boundary_down(&self, levels: &Levels, j: usize) -> Transitions {
        self.boundary(&levels.upper(j), &levels.lower(j))
    }

    /// The states these transitions can start from: `{x : ∃y, (x,y) ∈ self}`.
    ///
    /// Note this is the **existential** projection. "Every admissible step from `x`
    /// crosses" is a different, universal condition, and the two come apart exactly
    /// where minimal cut vectors do.
    pub fn sources(&self) -> StateSet {
        let m = self.mgr();
        let node = {
            let mut mm = m.borrow_mut();
            let all = mm.one();
            mm.pre_image(all, self.node)
        };
        self.wrap_set(&m, node)
    }

    /// The states these transitions can land on: `{y : ∃x, (x,y) ∈ self}`.
    pub fn targets(&self) -> StateSet {
        let m = self.mgr();
        let node = {
            let mut mm = m.borrow_mut();
            let all = mm.one();
            mm.post_image(all, self.node)
        };
        self.wrap_set(&m, node)
    }

    /// Where these transitions can take the system, starting from `set`.
    pub fn step_forward(&self, set: &StateSet) -> StateSet {
        self.same_forest(set);
        let m = self.mgr();
        let node = m.borrow_mut().post_image(set.node, self.node);
        self.wrap_set(&m, node)
    }

    /// Where the system must have been to reach `set` under these transitions.
    pub fn step_backward(&self, set: &StateSet) -> StateSet {
        self.same_forest(set);
        let m = self.mgr();
        let node = m.borrow_mut().pre_image(set.node, self.node);
        self.wrap_set(&m, node)
    }

    fn wrap_set(&self, m: &Rc<RefCell<MxdManager>>, node: NodeId) -> StateSet {
        let s = StateSet::from_weak(self.parent.clone(), self.gc.clone(), node);
        if let Some(gc) = self.gc.upgrade() {
            maybe_gc(m, &gc);
        }
        s
    }
}

/// The level sets of a structure function: `U_j = {φ ≥ j}` and `L_j = {φ < j}`.
///
/// Produced by [`System::levels_from_states`] or [`System::levels_from_fold`], and
/// a gc root for as long as it lives.
#[derive(Debug, Clone)]
pub struct Levels {
    upper: Vec<StateSet>,
    lower: Vec<StateSet>,
}

impl Levels {
    /// The number of system states, so φ takes values in `0..states()`.
    pub fn states(&self) -> usize {
        self.upper.len()
    }

    /// The levels worth asking about: `1 ..= states()-1`. Level 0 is trivial —
    /// every state satisfies `φ ≥ 0`.
    pub fn interior(&self) -> std::ops::Range<usize> {
        1..self.states()
    }

    /// `U_j = {x : φ(x) ≥ j}`.
    pub fn upper(&self, j: usize) -> StateSet {
        self.upper[j].clone()
    }

    /// `L_j = {x : φ(x) < j}`.
    pub fn lower(&self, j: usize) -> StateSet {
        self.lower[j].clone()
    }
}

/// A multi-state system: a fixed list of components, each with its own number of
/// states, and the decision-diagram forest they live in.
///
/// Owns the forest and builds things; the operations live on the handles it hands
/// out, as they do in `relib-bss` and `relib-mss`.
#[derive(Debug)]
pub struct System {
    mxd: Rc<RefCell<MxdManager>>,
    gc: Rc<RefCell<GcState>>,
    states: Vec<usize>,
}

impl System {
    /// Declares a system whose component `i` has `states[i]` states, numbered
    /// `0..states[i]` with 0 the worst.
    ///
    /// # Panics
    ///
    /// If `states` is empty or any component has zero states.
    pub fn new(states: &[usize]) -> Self {
        assert!(!states.is_empty(), "a system needs at least one component");
        let mut mgr = MxdManager::new();
        for (i, &n) in states.iter().enumerate() {
            assert!(n > 0, "component {i} must have at least one state");
            mgr.defvar(&format!("x{i}"), n);
        }
        Self {
            mxd: Rc::new(RefCell::new(mgr)),
            gc: Rc::new(RefCell::new(GcState {
                roots: BddHashMap::default(),
                threshold: GC_FLOOR,
                floor: GC_FLOOR,
            })),
            states: states.to_vec(),
        }
    }

    fn wrap_set(&self, node: NodeId) -> StateSet {
        StateSet::from_weak(Rc::downgrade(&self.mxd), Rc::downgrade(&self.gc), node)
    }

    fn wrap_rel(&self, node: NodeId) -> Transitions {
        Transitions::from_weak(Rc::downgrade(&self.mxd), Rc::downgrade(&self.gc), node)
    }

    /// Number of components.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// The number of states of each component.
    pub fn states(&self) -> &[usize] {
        &self.states
    }

    /// `|Ω| = Π n_i`.
    pub fn state_space_size(&self) -> u128 {
        self.mxd.borrow().state_space_size()
    }

    /// Live node slots in the forest, terminals included. The size of the whole
    /// arena, not of any one result — use [`StateSet::node_count`] or
    /// [`Transitions::node_count`] for that.
    pub fn live_node_count(&self) -> usize {
        self.mxd.borrow().live_node_count()
    }

    /// Collect now, keeping every handle still alive. Auto-collection does this on
    /// its own once the arena grows past a threshold; calling it explicitly is
    /// mainly useful before taking a measurement.
    pub fn gc(&self) -> usize {
        let roots: Vec<NodeId> = self.gc.borrow().roots.keys().copied().collect();
        self.mxd.borrow_mut().gc(&roots)
    }

    // ------------------------------------------------------------- level sets

    /// Level sets of a structure function given as a plain function of the state
    /// vector.
    ///
    /// The simplest thing to write, and the right choice while a system is small.
    /// **It evaluates φ at every point of `Ω`**, so its cost is the size of the
    /// state space, not of the diagram. Past a few million states use
    /// [`levels_from_fold`](Self::levels_from_fold).
    ///
    /// # Panics
    ///
    /// If φ ever returns a value so large that the level range would be absurd
    /// (more than `10_000` system states); that is almost always a bug in φ rather
    /// than an intended system.
    pub fn levels_from_states(&self, phi: impl Fn(&[usize]) -> usize) -> Levels {
        let mut table: HashMap<StateVec, usize> = HashMap::new();
        let mut max = 0usize;
        let mut x = vec![0usize; self.states.len()];
        loop {
            let v = phi(&x);
            max = max.max(v);
            assert!(
                max < 10_000,
                "φ returned {v}; a system with that many states is almost certainly a bug"
            );
            table.insert(x.clone(), v);
            let mut i = 0;
            loop {
                if i == x.len() {
                    return self.levels_from_table(max + 1, move |y: &StateVec| table[y]);
                }
                x[i] += 1;
                if x[i] < self.states[i] {
                    break;
                }
                x[i] = 0;
                i += 1;
            }
        }
    }

    fn levels_from_table(&self, m: usize, phi: impl Fn(&StateVec) -> usize) -> Levels {
        let k = self.states.len();
        let mut upper = Vec::with_capacity(m);
        let mut lower = Vec::with_capacity(m);
        for j in 0..m {
            let mut assign = vec![0usize; k];
            let u = self.build_set(k as i64 - 1, &mut assign, &|x| phi(x) >= j);
            let mut assign = vec![0usize; k];
            let l = self.build_set(k as i64 - 1, &mut assign, &|x| phi(x) < j);
            upper.push(self.wrap_set(u));
            lower.push(self.wrap_set(l));
        }
        Levels { upper, lower }
    }

    fn build_set(
        &self,
        level: i64,
        assign: &mut StateVec,
        keep: &dyn Fn(&StateVec) -> bool,
    ) -> NodeId {
        if level < 0 {
            let m = self.mxd.borrow();
            return if keep(assign) { m.one() } else { m.zero() };
        }
        let l = level as usize;
        let children: Vec<NodeId> = (0..self.states[l])
            .map(|v| {
                assign[l] = v;
                self.build_set(level - 1, assign, keep)
            })
            .collect();
        self.mxd.borrow_mut().create_set_node(l, &children)
    }

    /// Level sets of a structure function given as a **fold over the components**:
    /// an accumulator threaded from the last component to the first, then read off
    /// as a performance value.
    ///
    /// This is the scalable form. Cost is the number of distinct
    /// `(component, accumulator)` pairs, not the size of `Ω` — so a system with
    /// `3^22 ≈ 3·10^10` states is still small, *provided the accumulator stays
    /// bounded*. Saturate it if it would otherwise grow without limit: if only
    /// `Σx_i ≥ 7` matters, fold with `acc.min(7)` rather than the true sum, or the
    /// diagram grows with the sum's range for no benefit.
    ///
    /// `step(acc, i, v)` folds component `i` in state `v`; `value(acc)` reads the
    /// final accumulator as φ.
    ///
    /// ```
    /// use mxdcore::analysis::*;
    ///
    /// // φ = 2 while total production is in band, 1 above it, 0 below.
    /// let sys = System::new(&[3; 12]);
    /// let levels = sys.levels_from_fold(
    ///     0usize,
    ///     |acc, _i, v| (acc + v).min(7),
    ///     |&acc| if 3 * acc < 5 { 0 } else if 3 * acc > 20 { 1 } else { 2 },
    /// );
    /// assert_eq!(levels.states(), 3);
    /// ```
    pub fn levels_from_fold<A, S, V>(&self, init: A, step: S, value: V) -> Levels
    where
        A: Clone + Eq + Hash,
        S: Fn(&A, usize, usize) -> A,
        V: Fn(&A) -> usize,
    {
        // Work out φ's range first, from the accumulators reachable at the bottom.
        // Cheap, and it saves the caller from having to declare it.
        let mut seen: Vec<A> = Vec::new();
        let mut stack = vec![(self.states.len() as i64 - 1, init.clone())];
        let mut visited: std::collections::HashSet<(i64, A)> = std::collections::HashSet::new();
        while let Some((level, acc)) = stack.pop() {
            if !visited.insert((level, acc.clone())) {
                continue;
            }
            if level < 0 {
                seen.push(acc);
                continue;
            }
            let l = level as usize;
            for v in 0..self.states[l] {
                stack.push((level - 1, step(&acc, l, v)));
            }
        }
        let m = seen.iter().map(&value).max().unwrap_or(0) + 1;

        let k = self.states.len();
        let mut upper = Vec::with_capacity(m);
        let mut lower = Vec::with_capacity(m);
        for j in 0..m {
            let mut memo = HashMap::new();
            let u = self.fold_set(k as i64 - 1, init.clone(), &step, &|a| value(a) >= j, &mut memo);
            let mut memo = HashMap::new();
            let l = self.fold_set(k as i64 - 1, init.clone(), &step, &|a| value(a) < j, &mut memo);
            upper.push(self.wrap_set(u));
            lower.push(self.wrap_set(l));
        }
        Levels { upper, lower }
    }

    fn fold_set<A, S>(
        &self,
        level: i64,
        acc: A,
        step: &S,
        keep: &dyn Fn(&A) -> bool,
        memo: &mut HashMap<(i64, A), NodeId>,
    ) -> NodeId
    where
        A: Clone + Eq + Hash,
        S: Fn(&A, usize, usize) -> A,
    {
        if level < 0 {
            let m = self.mxd.borrow();
            return if keep(&acc) { m.one() } else { m.zero() };
        }
        if let Some(&hit) = memo.get(&(level, acc.clone())) {
            return hit;
        }
        let l = level as usize;
        let children: Vec<NodeId> = (0..self.states[l])
            .map(|v| {
                let next = step(&acc, l, v);
                self.fold_set(level - 1, next, step, keep, memo)
            })
            .collect();
        let node = self.mxd.borrow_mut().create_set_node(l, &children);
        memo.insert((level, acc), node);
        node
    }

    // -------------------------------------------------------------- relations

    /// One component steps down one state, the rest unchanged: gradual degradation.
    pub fn degrade(&self) -> Transitions {
        self.steps(&|n| (1..n).map(|v| (v, v - 1)).collect())
    }

    /// One component steps up one state, the rest unchanged: gradual repair.
    pub fn repair(&self) -> Transitions {
        self.steps(&|n| (0..n - 1).map(|v| (v, v + 1)).collect())
    }

    /// One component jumps straight to its best state: replacement or restart.
    pub fn restart(&self) -> Transitions {
        self.steps(&|n| (0..n - 1).map(|v| (v, n - 1)).collect())
    }

    /// One component fails straight to its worst state.
    pub fn fail(&self) -> Transitions {
        self.steps(&|n| (1..n).map(|v| (v, 0)).collect())
    }

    /// A single named transition of one component, everything else unchanged.
    ///
    /// # Panics
    ///
    /// If `component` is out of range, or either state is not one of its states.
    pub fn single(&self, component: usize, from: usize, to: usize) -> Transitions {
        let k = self.states.len();
        assert!(
            component < k,
            "component {component} is out of range: the system has {k}"
        );
        let n = self.states[component];
        assert!(from < n, "component {component} has no state {from} (0..{n})");
        assert!(to < n, "component {component} has no state {to} (0..{n})");
        let mut src = vec![Src::Any; k];
        let mut dst = vec![Dst::Same; k];
        src[component] = Src::Val(from);
        dst[component] = Dst::Val(to);
        let node = self.mxd.borrow_mut().mxd_singleton(&src, &dst);
        self.wrap_rel(node)
    }

    /// "Exactly one component takes one of the steps `pattern` admits; every other
    /// component holds."
    ///
    /// Built directly, bottom-up, in one `create_rel_node` per component. The
    /// obvious way is to union a singleton per (component, step), but that is
    /// `k · (states-1)` unions each costing `O(k · n²)` — 13.9 ms for a hundred
    /// components of five states, to produce a hundred nodes.
    ///
    /// The structure is a chain instead. At component `i` the block says either
    ///
    /// - `i` takes an admitted step, and everything below it holds — the cell
    ///   points at `One`, which as a relation *is* the identity; or
    /// - `i` holds (the diagonal), and something below it stepped — the cell points
    ///   at the chain built so far.
    ///
    /// so the whole relation is `k` node creations and no apply at all.
    fn steps(&self, pattern: &dyn Fn(usize) -> Vec<(usize, usize)>) -> Transitions {
        let (zero, one) = {
            let m = self.mxd.borrow();
            (m.zero(), m.one())
        };
        // `below` denotes "some component strictly below this level has stepped,
        // and the rest hold". Nothing has stepped yet at the bottom.
        let mut below = zero;
        for i in 0..self.states.len() {
            let n = self.states[i];
            let mut block = vec![zero; n * n];
            for (from, to) in pattern(n) {
                debug_assert_ne!(from, to, "a step must move the component");
                // This component steps; everything below is the identity.
                block[from * n + to] = one;
            }
            // This component holds; something below stepped.
            for a in 0..n {
                block[a * n + a] = below;
            }
            below = self.mxd.borrow_mut().create_rel_node(i, &block);
        }
        self.wrap_rel(below)
    }

    /// Nothing moves: the identity relation.
    pub fn stay(&self) -> Transitions {
        let k = self.states.len();
        let node = self
            .mxd
            .borrow_mut()
            .mxd_singleton(&vec![Src::Any; k], &vec![Dst::Same; k]);
        self.wrap_rel(node)
    }

    /// No transitions at all.
    pub fn no_transitions(&self) -> Transitions {
        let node = self.mxd.borrow().zero();
        self.wrap_rel(node)
    }

    /// Every state.
    pub fn all_states(&self) -> StateSet {
        let node = self.mxd.borrow().one();
        self.wrap_set(node)
    }

    /// No states.
    pub fn no_states(&self) -> StateSet {
        let node = self.mxd.borrow().zero();
        self.wrap_set(node)
    }

    /// The set of state vectors matching `pattern`, where [`Src::Any`] leaves a
    /// component free.
    pub fn state_pattern(&self, pattern: &[Src]) -> StateSet {
        assert_eq!(
            pattern.len(),
            self.states.len(),
            "pattern must give one entry per component"
        );
        let node = self.mxd.borrow_mut().set_minterm(pattern);
        self.wrap_set(node)
    }

    /// The set containing the single state vector `values`.
    pub fn state(&self, values: &[usize]) -> StateSet {
        assert_eq!(
            values.len(),
            self.states.len(),
            "a state vector needs one entry per component"
        );
        let node = self.mxd.borrow_mut().state(values);
        self.wrap_set(node)
    }
}
