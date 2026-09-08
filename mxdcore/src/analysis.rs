//! Multi-state boundary analysis: the layer most callers should use.
//!
//! [`MxdManager`] is the engine. This is the vocabulary of
//! the problem it was built for — components with states, a structure function,
//! transition relations, and the boundaries between performance levels — so that a
//! case study reads as one and does not have to re-derive level sets and relation
//! families by hand each time.
//!
//! ```
//! use mxdcore::analysis::*;
//!
//! // Three components, three states each. φ is the worst component (a series
//! // system), so the system performs at level j exactly when all components do.
//! let mut sys = System::new(&[3, 3, 3]);
//! let levels = sys.levels_from_states(|x| *x.iter().min().unwrap());
//! let degrade = sys.degrade();
//!
//! // Transitions that drop the system out of {φ ≥ 2}.
//! let b = sys.boundary_down(&levels, 2, degrade);
//! assert_eq!(sys.count(b), 3, "one component falls from 2 to 1, the others are at 2");
//! ```
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
use crate::mxd::MxdManager;
use common::prelude::NodeId;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes systems, so a handle from one cannot be used with another.
static NEXT_SYSTEM_ID: AtomicU64 = AtomicU64::new(0);

/// A set of state vectors.
///
/// Belongs to the [`System`] that produced it; using it with another panics in
/// debug builds rather than quietly computing nonsense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateSet(NodeId, u64);

/// A set of transitions — a relation over state vectors.
///
/// Belongs to the [`System`] that produced it, as [`StateSet`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Transitions(NodeId, u64);

/// The level sets of a structure function: `U_j = {φ ≥ j}` and `L_j = {φ < j}`.
///
/// Produced by [`System::levels_from_states`] or [`System::levels_from_fold`].
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
        self.upper[j]
    }

    /// `L_j = {x : φ(x) < j}`.
    pub fn lower(&self, j: usize) -> StateSet {
        self.lower[j]
    }
}

/// A multi-state system: a fixed list of components, each with its own number of
/// states, and the decision-diagram forest they live in.
#[derive(Debug)]
pub struct System {
    mgr: MxdManager,
    states: Vec<usize>,
    id: u64,
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
            mgr,
            states: states.to_vec(),
            id: NEXT_SYSTEM_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    #[inline]
    fn set(&self, n: NodeId) -> StateSet {
        StateSet(n, self.id)
    }

    #[inline]
    fn rel(&self, n: NodeId) -> Transitions {
        Transitions(n, self.id)
    }

    #[inline]
    #[track_caller]
    fn s(&self, x: StateSet) -> NodeId {
        debug_assert_eq!(
            x.1, self.id,
            "this StateSet belongs to a different System; handles are not portable \
             between them"
        );
        x.0
    }

    #[inline]
    #[track_caller]
    fn r(&self, x: Transitions) -> NodeId {
        debug_assert_eq!(
            x.1, self.id,
            "these Transitions belong to a different System; handles are not portable \
             between them"
        );
        x.0
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
        self.mgr.state_space_size()
    }

    /// The underlying forest, for anything this layer does not cover.
    pub fn engine(&mut self) -> &mut MxdManager {
        &mut self.mgr
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
    pub fn levels_from_states(&mut self, phi: impl Fn(&[usize]) -> usize) -> Levels {
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
            // odometer over the state space
            let mut i = 0;
            loop {
                if i == x.len() {
                    let value = move |y: &StateVec| table[y];
                    return self.levels_from_table(max + 1, value);
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

    fn levels_from_table(&mut self, m: usize, phi: impl Fn(&StateVec) -> usize) -> Levels {
        let k = self.states.len();
        let mut upper = Vec::with_capacity(m);
        let mut lower = Vec::with_capacity(m);
        for j in 0..m {
            let mut assign = vec![0usize; k];
            let u = self.build_set(k as i64 - 1, &mut assign, &|x| phi(x) >= j);
            let mut assign = vec![0usize; k];
            let l = self.build_set(k as i64 - 1, &mut assign, &|x| phi(x) < j);
            upper.push(self.set(u));
            lower.push(self.set(l));
        }
        Levels { upper, lower }
    }

    fn build_set(
        &mut self,
        level: i64,
        assign: &mut StateVec,
        keep: &dyn Fn(&StateVec) -> bool,
    ) -> NodeId {
        if level < 0 {
            return if keep(assign) {
                self.mgr.one()
            } else {
                self.mgr.zero()
            };
        }
        let l = level as usize;
        let children: Vec<NodeId> = (0..self.states[l])
            .map(|v| {
                assign[l] = v;
                self.build_set(level - 1, assign, keep)
            })
            .collect();
        self.mgr.create_set_node(l, &children)
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
    /// let mut sys = System::new(&[3; 12]);
    /// let levels = sys.levels_from_fold(
    ///     0usize,
    ///     |acc, _i, v| (acc + v).min(7),
    ///     |&acc| if 3 * acc < 5 { 0 } else if 3 * acc > 20 { 1 } else { 2 },
    /// );
    /// assert_eq!(levels.states(), 3);
    /// ```
    pub fn levels_from_fold<A, S, V>(&mut self, init: A, step: S, value: V) -> Levels
    where
        A: Clone + Eq + Hash,
        S: Fn(&A, usize, usize) -> A,
        V: Fn(&A) -> usize,
    {
        // First work out φ's range, by collecting the accumulators reachable at the
        // bottom. Cheap, and it saves the caller from having to declare it.
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
        let m = seen.iter().map(|a| value(a)).max().unwrap_or(0) + 1;

        let k = self.states.len();
        let mut upper = Vec::with_capacity(m);
        let mut lower = Vec::with_capacity(m);
        for j in 0..m {
            let mut memo = HashMap::new();
            let u = self.fold_set(k as i64 - 1, init.clone(), &step, &|a| value(a) >= j, &mut memo);
            let mut memo = HashMap::new();
            let l = self.fold_set(k as i64 - 1, init.clone(), &step, &|a| value(a) < j, &mut memo);
            upper.push(self.set(u));
            lower.push(self.set(l));
        }
        Levels { upper, lower }
    }

    fn fold_set<A, S>(
        &mut self,
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
            return if keep(&acc) {
                self.mgr.one()
            } else {
                self.mgr.zero()
            };
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
        let node = self.mgr.create_set_node(l, &children);
        memo.insert((level, acc), node);
        node
    }

    // -------------------------------------------------------------- relations

    /// One component steps down one state, the rest unchanged: gradual degradation.
    pub fn degrade(&mut self) -> Transitions {
        self.steps(&|n| (1..n).map(|v| (v, v - 1)).collect())
    }

    /// One component steps up one state, the rest unchanged: gradual repair.
    pub fn repair(&mut self) -> Transitions {
        self.steps(&|n| (0..n - 1).map(|v| (v, v + 1)).collect())
    }

    /// One component jumps straight to its best state: replacement or restart.
    pub fn restart(&mut self) -> Transitions {
        self.steps(&|n| (0..n - 1).map(|v| (v, n - 1)).collect())
    }

    /// One component fails straight to its worst state.
    pub fn fail(&mut self) -> Transitions {
        self.steps(&|n| (1..n).map(|v| (v, 0)).collect())
    }

    /// A single named transition of one component, everything else unchanged.
    ///
    /// # Panics
    ///
    /// If `component` is out of range, or either state is not one of its states.
    pub fn single(&mut self, component: usize, from: usize, to: usize) -> Transitions {
        let k = self.states.len();
        assert!(
            component < k,
            "component {component} is out of range: the system has {k}"
        );
        let n = self.states[component];
        assert!(from < n, "component {component} has no state {from} (0..{n})");
        assert!(to < n, "component {component} has no state {to} (0..{n})");
        let mut src = vec![crate::minterm::Src::Any; k];
        let mut dst = vec![crate::minterm::Dst::Same; k];
        src[component] = crate::minterm::Src::Val(from);
        dst[component] = crate::minterm::Dst::Val(to);
        let node = self.mgr.mxd_singleton(&src, &dst);
        self.rel(node)
    }

    /// The union over every component of the steps `pattern` admits for a component
    /// with that many states.
    fn steps(&mut self, pattern: &dyn Fn(usize) -> Vec<(usize, usize)>) -> Transitions {
        let mut rel = self.mgr.zero();
        for i in 0..self.states.len() {
            for (from, to) in pattern(self.states[i]) {
                let one = self.single(i, from, to);
                let one = self.r(one);
                rel = self.mgr.or_rel(rel, one);
            }
        }
        self.rel(rel)
    }

    /// Nothing moves.
    pub fn stay(&mut self) -> Transitions {
        let k = self.states.len();
        let node = self.mgr.mxd_singleton(
            &vec![crate::minterm::Src::Any; k],
            &vec![crate::minterm::Dst::Same; k],
        );
        self.rel(node)
    }

    /// No transitions at all.
    pub fn no_transitions(&self) -> Transitions {
        self.rel(self.mgr.zero())
    }

    /// Every state.
    pub fn all_states(&self) -> StateSet {
        self.set(self.mgr.one())
    }

    /// No states.
    pub fn no_states(&self) -> StateSet {
        self.set(self.mgr.zero())
    }

    // ---------------------------------------------------------------- algebra

    pub fn union(&mut self, a: Transitions, b: Transitions) -> Transitions {
        let (a, b) = (self.r(a), self.r(b));
        let node = self.mgr.or_rel(a, b);
        self.rel(node)
    }

    pub fn intersect(&mut self, a: Transitions, b: Transitions) -> Transitions {
        let (a, b) = (self.r(a), self.r(b));
        let node = self.mgr.and_rel(a, b);
        self.rel(node)
    }

    pub fn difference(&mut self, a: Transitions, b: Transitions) -> Transitions {
        let (a, b) = (self.r(a), self.r(b));
        let node = self.mgr.setdiff_rel(a, b);
        self.rel(node)
    }

    /// The converse relation: every transition reversed.
    pub fn converse(&mut self, a: Transitions) -> Transitions {
        let a = self.r(a);
        let node = self.mgr.transpose(a);
        self.rel(node)
    }

    pub fn union_states(&mut self, a: StateSet, b: StateSet) -> StateSet {
        let (a, b) = (self.s(a), self.s(b));
        let node = self.mgr.or_set(a, b);
        self.set(node)
    }

    pub fn intersect_states(&mut self, a: StateSet, b: StateSet) -> StateSet {
        let (a, b) = (self.s(a), self.s(b));
        let node = self.mgr.and_set(a, b);
        self.set(node)
    }

    pub fn difference_states(&mut self, a: StateSet, b: StateSet) -> StateSet {
        let (a, b) = (self.s(a), self.s(b));
        let node = self.mgr.setdiff_set(a, b);
        self.set(node)
    }

    // --------------------------------------------------------------- boundary

    /// The boundary operator `B = R ∩ (from × to)`: the transitions of `rel` that
    /// start in `from` and land in `to`.
    ///
    /// Nothing here assumes φ is monotone. That is the point — it is what lets
    /// repair, restart and recovery be analysed the same way as failure, and it is
    /// why this is defined for systems where minimal path and cut vectors are not.
    pub fn boundary(&mut self, from: StateSet, to: StateSet, rel: Transitions) -> Transitions {
        let (from, to, rel) = (self.s(from), self.s(to), self.r(rel));
        let node = self.mgr.boundary(from, to, rel);
        self.rel(node)
    }

    /// Transitions of `rel` that carry the system **into** `{φ ≥ j}`.
    pub fn boundary_up(&mut self, levels: &Levels, j: usize, rel: Transitions) -> Transitions {
        self.boundary(levels.lower(j), levels.upper(j), rel)
    }

    /// Transitions of `rel` that carry the system **out of** `{φ ≥ j}`.
    pub fn boundary_down(&mut self, levels: &Levels, j: usize, rel: Transitions) -> Transitions {
        self.boundary(levels.upper(j), levels.lower(j), rel)
    }

    /// The states a transition set can start from: `{x : ∃y, (x,y) ∈ rel}`.
    ///
    /// Note this is the **existential** projection. "Every admissible step from `x`
    /// crosses" is a different, universal condition, and the two come apart exactly
    /// where minimal cut vectors do.
    pub fn sources(&mut self, rel: Transitions) -> StateSet {
        let (all, rel) = (self.mgr.one(), self.r(rel));
        let node = self.mgr.pre_image(all, rel);
        self.set(node)
    }

    /// The states a transition set can land on: `{y : ∃x, (x,y) ∈ rel}`.
    pub fn targets(&mut self, rel: Transitions) -> StateSet {
        let (all, rel) = (self.mgr.one(), self.r(rel));
        let node = self.mgr.post_image(all, rel);
        self.set(node)
    }

    /// Where `rel` can take the system starting from `set`.
    pub fn step_forward(&mut self, set: StateSet, rel: Transitions) -> StateSet {
        let (set, rel) = (self.s(set), self.r(rel));
        let node = self.mgr.post_image(set, rel);
        self.set(node)
    }

    /// Where the system must have been to reach `set` under `rel`.
    pub fn step_backward(&mut self, set: StateSet, rel: Transitions) -> StateSet {
        let (set, rel) = (self.s(set), self.r(rel));
        let node = self.mgr.pre_image(set, rel);
        self.set(node)
    }

    // ---------------------------------------------------------------- reading

    /// How many transitions. Does not enumerate them.
    pub fn count(&self, rel: Transitions) -> u128 {
        self.mgr.cardinality_relation(self.r(rel))
    }

    /// How many states. Does not enumerate them.
    pub fn count_states(&self, set: StateSet) -> u128 {
        self.mgr.cardinality_set(self.s(set))
    }

    /// Every transition, listed. Exponential — for inspecting small results, not
    /// for measuring large ones; use [`count`](Self::count) for that.
    pub fn transitions(&self, rel: Transitions) -> Vec<Transition> {
        self.mgr.enumerate_relation(self.r(rel))
    }

    /// Every state, listed. Same caveat as [`transitions`](Self::transitions).
    pub fn state_vectors(&self, set: StateSet) -> Vec<StateVec> {
        self.mgr.enumerate_set(self.s(set))
    }

    /// Diagram size, for reporting. Not comparable with MEDDLY's on the relation
    /// side; see the crate docs.
    pub fn node_count(&self, rel: Transitions) -> usize {
        self.mgr.node_count(self.r(rel))
    }

    pub fn node_count_states(&self, set: StateSet) -> usize {
        self.mgr.node_count(self.s(set))
    }

    /// Graphviz source.
    pub fn dot(&self, rel: Transitions) -> String {
        use common::prelude::Dot;
        self.mgr.dot_string(&self.r(rel))
    }

    pub fn dot_states(&self, set: StateSet) -> String {
        use common::prelude::Dot;
        self.mgr.dot_string(&self.s(set))
    }
}
