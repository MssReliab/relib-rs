use crate::nodes::*;
use common::prelude::*;

/// A node in the MxD forest.
///
/// Only two terminals, unlike `mddcore::mdd::Node`: that forest carries an extra
/// `Undet` for partial multi-state structure functions, which has no meaning for a
/// set or a relation. `Zero` is the empty set / empty relation, `One` the full one.
#[derive(Debug)]
pub enum Node {
    NonTerminal(NonTerminalMxD),
    Zero,
    One,
}

impl Node {
    pub fn id(&self) -> NodeId {
        match self {
            Self::NonTerminal(x) => x.id(),
            Self::Zero => 0,
            Self::One => 1,
        }
    }

    pub fn headerid(&self) -> Option<HeaderId> {
        match self {
            Self::NonTerminal(x) => Some(x.headerid()),
            _ => None,
        }
    }
}

/// What a header's child vector means. Both kinds carry the level so a node can be
/// classified from its header alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderKind {
    /// `edge_num == n`; child `a` is the sub-diagram for `x_i = a`.
    Set(Level),
    /// `edge_num == n * n`; child `a * n + b` is the sub-diagram for `(from_i = a, to_i = b)`.
    Rel(Level),
}

/// One declared variable. Set and relation nodes for the same variable sit at the
/// same level but under **different** `HeaderId`s, so the unique table can never
/// confuse one for the other.
#[derive(Debug, Clone, Copy)]
pub struct VarInfo {
    pub level: Level,
    pub domain: usize,
    pub set_header: HeaderId,
    pub rel_header: HeaderId,
}

/// A forest holding both sets (over state vectors) and relations (over pairs of
/// state vectors), in one arena and one level space.
///
/// # Level convention
///
/// Inherited from the rest of the workspace: `level == declaration index`, and a
/// **higher level is closer to the root**. So the first variable declared sits at
/// the bottom and the last one is the root. Enumeration walks from `k-1` down to 0.
///
/// # Fused relation nodes
///
/// MEDDLY spends two interleaved levels per variable (unprimed then primed). This
/// forest instead fuses them into a single node with `n * n` edges. That costs the
/// sharing of primed sub-graphs between different `from` values, but `n` is a
/// component's state count (2–5 in practice), and it buys a *local* diagonal test:
/// identity reduction becomes a plain predicate on one node's child block rather
/// than a rewrite that has to happen in the parent because only the parent knows
/// which `from` edge arrived. See the crate docs.
#[derive(Debug)]
pub struct MxdManager {
    headers: Vec<NodeHeader>,
    kinds: Vec<HeaderKind>,
    vars: Vec<VarInfo>,
    nodes: Vec<Node>,
    zero: NodeId,
    one: NodeId,
    // Keys/values stored as u32 (node/header counts fit in 32 bits), as in
    // `mddcore::mdd::MddManager`: halves the bytes of the (header, children) key
    // copied and hashed per create_node.
    utable: BddHashMap<(u32, Box<[u32]>), u32>,
    // Set operations are level-independent (full reduction makes a skipped level a
    // plain don't-care), so they share one op-keyed table: (op code, f, g).
    set_cache: ComputeCache,
    // Relation operations are NOT level-independent: a skipped level means the
    // identity, so the same pair of operands combines differently depending on how
    // many levels are still to come. The level therefore has to be in the key,
    // which leaves no room for an op code in the three available words — hence one
    // dedicated table per operation, keyed (f, g, level).
    and_rel_cache: ComputeCache,
    or_rel_cache: ComputeCache,
    not_rel_cache: ComputeCache,
    cross_cache: ComputeCache,
    transpose_cache: ComputeCache,
    post_cache: ComputeCache,
    pre_cache: ComputeCache,
    // The full relation Ω × Ω restricted to levels 0..=i, indexed by i. Built
    // lazily; needed because complementing a relation cannot bottom out at the One
    // terminal (which denotes the *identity*, not the full relation).
    full_rel: Vec<NodeId>,
    // Slots in `nodes` reclaimed by gc(), available for reuse.
    freelist: Vec<u32>,
}

impl DDForest for MxdManager {
    type Node = Node;
    type NodeHeader = NodeHeader;

    #[inline]
    fn get_node(&self, id: &NodeId) -> Option<&Self::Node> {
        self.nodes.get(*id)
    }

    #[inline]
    fn get_header(&self, id: &HeaderId) -> Option<&Self::NodeHeader> {
        self.headers.get(*id)
    }

    fn level(&self, id: &NodeId) -> Option<Level> {
        self.get_node(id).and_then(|node| match node {
            Node::NonTerminal(fnode) => self.get_header(&fnode.headerid()).map(|x| x.level()),
            Node::Zero | Node::One => None,
        })
    }

    fn label(&self, id: &NodeId) -> Option<&str> {
        self.get_node(id).and_then(|node| match node {
            Node::NonTerminal(fnode) => self.get_header(&fnode.headerid()).map(|x| x.label()),
            Node::Zero | Node::One => None,
        })
    }
}

impl Default for MxdManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MxdManager {
    pub fn new() -> Self {
        let mut nodes = Vec::default();
        let zero = {
            let tmp = Node::Zero;
            let id = tmp.id();
            nodes.push(tmp);
            debug_assert!(id == nodes[id].id());
            id
        };
        let one = {
            let tmp = Node::One;
            let id = tmp.id();
            nodes.push(tmp);
            debug_assert!(id == nodes[id].id());
            id
        };
        Self {
            headers: Vec::default(),
            kinds: Vec::default(),
            vars: Vec::default(),
            nodes,
            zero,
            one,
            utable: BddHashMap::default(),
            set_cache: ComputeCache::new(),
            and_rel_cache: ComputeCache::new(),
            or_rel_cache: ComputeCache::new(),
            not_rel_cache: ComputeCache::new(),
            cross_cache: ComputeCache::new(),
            transpose_cache: ComputeCache::new(),
            post_cache: ComputeCache::new(),
            pre_cache: ComputeCache::new(),
            full_rel: Vec::new(),
            freelist: Vec::new(),
        }
    }

    /// Declares a variable with `domain` states, returning its level.
    ///
    /// Creates **two** headers at that level: a set header (`edge_num = domain`)
    /// and a relation header (`edge_num = domain * domain`).
    pub fn defvar(&mut self, label: &str, domain: usize) -> Level {
        assert!(domain > 0, "variable {label} must have at least one state");
        let level = self.vars.len();
        let set_header = self.create_header(HeaderKind::Set(level), label, domain);
        let rel_header =
            self.create_header(HeaderKind::Rel(level), &format!("{label}'"), domain * domain);
        self.vars.push(VarInfo {
            level,
            domain,
            set_header,
            rel_header,
        });
        level
    }

    fn create_header(&mut self, kind: HeaderKind, label: &str, edge_num: usize) -> HeaderId {
        let id = self.headers.len();
        self.headers.push(NodeHeader::new(id, kind.level(), label, edge_num));
        self.kinds.push(kind);
        debug_assert!(id == self.headers[id].id());
        id
    }

    #[inline]
    pub fn num_vars(&self) -> usize {
        self.vars.len()
    }

    #[inline]
    pub fn var(&self, level: Level) -> &VarInfo {
        &self.vars[level]
    }

    #[inline]
    pub fn vars(&self) -> &[VarInfo] {
        &self.vars
    }

    #[inline]
    pub fn kind(&self, header: HeaderId) -> HeaderKind {
        self.kinds[header]
    }

    /// The size of the state space, `|Ω| = Π n_i`.
    pub fn state_space_size(&self) -> u128 {
        self.vars.iter().map(|v| v.domain as u128).product()
    }

    fn new_nonterminal(&mut self, header: HeaderId, nodes: &[NodeId]) -> NodeId {
        let id = if let Some(slot) = self.freelist.pop() {
            let id = slot as usize;
            self.nodes[id] = Node::NonTerminal(NonTerminalMxD::new(id, header, nodes));
            id
        } else {
            let id = self.nodes.len();
            self.nodes
                .push(Node::NonTerminal(NonTerminalMxD::new(id, header, nodes)));
            id
        };
        debug_assert!(id == self.nodes[id].id());
        id
    }

    /// Creates a **set** node at `level`, fully reduced.
    ///
    /// Identical rule to `mddcore::mdd::MddManager::create_node`: if every child is
    /// the same node the variable is a don't-care and the node is elided. A level
    /// skipped on a path therefore means "this variable takes any value".
    pub fn create_set_node(&mut self, level: Level, children: &[NodeId]) -> NodeId {
        let v = self.vars[level];
        assert_eq!(
            children.len(),
            v.domain,
            "set node at level {level} needs {} children",
            v.domain
        );
        if let Some(&first) = children.first() {
            if children.iter().all(|&x| first == x) {
                return first;
            }
        }
        self.hash_cons(v.set_header, children)
    }

    /// Creates a **relation** node at `level`, identity reduced.
    ///
    /// `block[a * n + b]` is the sub-diagram for the transition `(from = a, to = b)`.
    ///
    /// Two rules:
    ///
    /// 1. The empty block is the empty relation.
    /// 2. **Identity reduction**: a block that is `c` down the diagonal and empty
    ///    everywhere else says "this component does not move, then continue with
    ///    `c`", so the level carries no information of its own and is elided in
    ///    favour of `c`.
    ///
    /// The fully-reduced rule (*all entries equal ⇒ elide*) must **never** be applied
    /// here. Eliding a relation level means `to == from`; "all entries equal" means
    /// `to` is anything. Those are opposites, and confusing them is the difference
    /// between the identity and `Ω × Ω`.
    ///
    /// Because a variable's source and target live in one node, the diagonal test is
    /// local. Under MEDDLY's interleaved layout it is not: a primed node cannot tell
    /// which unprimed edge reached it, so the rewrite has to happen in the parent.
    /// That difference is the main reason for the fused representation.
    pub fn create_rel_node(&mut self, level: Level, block: &[NodeId]) -> NodeId {
        let v = self.vars[level];
        let n = v.domain;
        assert_eq!(
            block.len(),
            n * n,
            "relation node at level {level} needs {} children",
            n * n
        );
        if block.iter().all(|&x| x == self.zero) {
            return self.zero;
        }
        // Identity reduction: `c` on the diagonal, empty off it.
        let diag = block[0];
        if diag != self.zero
            && (0..n).all(|a| block[a * n + a] == diag)
            && (0..n).all(|a| (0..n).all(|b| a == b || block[a * n + b] == self.zero))
        {
            return diag;
        }
        self.hash_cons(v.rel_header, block)
    }

    fn hash_cons(&mut self, header: HeaderId, children: &[NodeId]) -> NodeId {
        let key = (header as u32, children.iter().map(|&x| x as u32).collect());
        if let Some(&nodeid) = self.utable.get(&key) {
            return nodeid as NodeId;
        }
        let node = self.new_nonterminal(header, children);
        self.utable.insert(key, node as u32);
        node
    }

    /// Mark-and-sweep garbage collection, mirroring
    /// `mddcore::mdd::MddManager::gc`. Marks everything reachable from `roots` plus
    /// the two terminals, reclaims the rest onto the free list and drops dead
    /// unique-table entries. Does not compact, so surviving `NodeId`s stay valid.
    /// Returns the number of slots reclaimed.
    pub fn gc(&mut self, roots: &[NodeId]) -> usize {
        let n = self.nodes.len();
        let mut live = vec![false; n];
        live[self.zero] = true;
        live[self.one] = true;

        let mut stack: Vec<NodeId> = roots.iter().copied().filter(|&r| r < n).collect();
        while let Some(id) = stack.pop() {
            if live[id] {
                continue;
            }
            live[id] = true;
            if let Node::NonTerminal(fnode) = &self.nodes[id] {
                stack.extend(fnode.iter());
            }
        }

        self.utable.retain(|_, &mut v| live[v as usize]);

        // Op-keyed, so entries touching a reclaimed slot can be dropped selectively.
        self.set_cache.retain_live(&live);
        // These carry a *level* in the third key word, so `retain_live3` would test
        // it as a node id and keep or drop entries for the wrong reason. Same call
        // that `mddcore::mtmdd2::MtMdd2Manager::gc` makes for its cross-forest
        // tables: drop the lot. A miss only costs a recomputation.
        self.and_rel_cache.clear();
        self.or_rel_cache.clear();
        self.not_rel_cache.clear();
        self.cross_cache.clear();
        self.transpose_cache.clear();
        self.post_cache.clear();
        self.pre_cache.clear();
        // Memoized full relations may name reclaimed slots; they rebuild cheaply
        // and hash-cons back to the same nodes if those are still live.
        self.full_rel.clear();

        self.freelist.clear();
        for (id, &alive) in live.iter().enumerate() {
            if !alive {
                self.freelist.push(id as u32);
            }
        }
        self.freelist.len()
    }

    /// The children of `node` if it sits at `level` and is of the requested kind,
    /// otherwise `None` — meaning the level is skipped on this path.
    ///
    /// This is the single place the skip rule is detected; what a skip *means* is
    /// the caller's business, and differs by kind: don't-care for a set, identity
    /// for a relation. See the `enumerate` module docs.
    pub(crate) fn children_at(
        &self,
        node: NodeId,
        level: Level,
        want_rel: bool,
    ) -> Option<Vec<NodeId>> {
        match self.get_node(&node) {
            Some(Node::NonTerminal(f)) => {
                let kind = self.kind(f.headerid());
                if kind.level() == level && kind.is_rel() == want_rel {
                    Some(f.iter().collect())
                } else {
                    // Either a lower level (skipped) or the other kind at this level.
                    debug_assert!(
                        kind.level() < level,
                        "node at level {} reached while reading level {level}",
                        kind.level()
                    );
                    None
                }
            }
            // A terminal: every remaining level is skipped.
            _ => None,
        }
    }

    /// Child `a` of a **set** node read at `level`, expanding a skipped level as
    /// the don't-care it means.
    ///
    /// The allocation-free counterpart of collecting the whole child vector: the
    /// hot recursions read one child, recurse (which needs `&mut self`, so no
    /// borrow may be held), then read the next. Costs a slot index and an enum
    /// match per call.
    #[inline]
    pub(crate) fn set_child(&self, node: NodeId, level: Level, a: usize) -> NodeId {
        match self.get_node(&node) {
            Some(Node::NonTerminal(f)) if self.is_at(f, level, false) => f.edge(a),
            _ => node,
        }
    }

    /// Cell `(a, b)` of a **relation** node read at `level`, expanding a skipped
    /// level as the identity it means.
    #[inline]
    pub(crate) fn rel_cell(&self, node: NodeId, level: Level, a: usize, b: usize) -> NodeId {
        let n = self.vars[level].domain;
        match self.get_node(&node) {
            Some(Node::NonTerminal(f)) if self.is_at(f, level, true) => f.edge(a * n + b),
            // Skipped: the component does not move, so only the diagonal exists.
            _ if a == b => node,
            _ => self.zero,
        }
    }

    #[inline]
    fn is_at(&self, f: &NonTerminalMxD, level: Level, want_rel: bool) -> bool {
        let kind = self.kinds[f.headerid()];
        kind.level() == level && kind.is_rel() == want_rel
    }

    /// Number of live (non-reclaimed) node slots, including terminals.
    #[inline]
    pub fn live_node_count(&self) -> usize {
        self.nodes.len() - self.freelist.len()
    }

    /// Number of distinct non-terminal nodes reachable from `root`.
    ///
    /// The size of one diagram, as opposed to [`live_node_count`](Self::live_node_count),
    /// which is the size of the whole arena. Nodes shared with other diagrams are
    /// counted here, once.
    pub fn node_count(&self, root: NodeId) -> usize {
        let mut seen = BddHashSet::default();
        let mut stack = vec![root];
        let mut n = 0;
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(Node::NonTerminal(f)) = self.get_node(&id) {
                n += 1;
                stack.extend(f.iter());
            }
        }
        n
    }

    /// `(headers, node slots)`.
    #[inline]
    pub fn size(&self) -> (usize, usize) {
        (self.headers.len(), self.nodes.len())
    }

    #[inline]
    pub fn zero(&self) -> NodeId {
        self.zero
    }

    #[inline]
    pub fn one(&self) -> NodeId {
        self.one
    }

    /// The full relation `Ω × Ω` restricted to levels `0..=level`.
    ///
    /// Cannot be the `One` terminal: a skipped relation level is the identity, so
    /// `One` denotes the diagonal. The full relation is a genuine spine of
    /// all-ones blocks.
    pub(crate) fn full_relation(&mut self, level: i64) -> NodeId {
        if level < 0 {
            return self.one;
        }
        let l = level as usize;
        if let Some(&cached) = self.full_rel.get(l) {
            if cached != usize::MAX {
                return cached;
            }
        }
        let below = self.full_relation(level - 1);
        let n = self.vars[l].domain;
        let node = self.create_rel_node(l, &vec![below; n * n]);
        if self.full_rel.len() <= l {
            self.full_rel.resize(l + 1, usize::MAX);
        }
        self.full_rel[l] = node;
        node
    }

    #[inline]
    pub(crate) fn set_cache_get(&self, op: u32, f: NodeId, g: NodeId) -> Option<NodeId> {
        self.set_cache.get(op, f as u32, g as u32).map(|v| v as NodeId)
    }

    #[inline]
    pub(crate) fn set_cache_put(&mut self, op: u32, f: NodeId, g: NodeId, val: NodeId) {
        self.set_cache.put(op, f as u32, g as u32, val as u32);
    }

    #[inline]
    pub(crate) fn rel_cache_get(
        &self,
        which: RelOp,
        f: NodeId,
        g: NodeId,
        level: i64,
    ) -> Option<NodeId> {
        self.rel_cache(which)
            .get(f as u32, g as u32, level as u32)
            .map(|v| v as NodeId)
    }

    #[inline]
    pub(crate) fn rel_cache_put(
        &mut self,
        which: RelOp,
        f: NodeId,
        g: NodeId,
        level: i64,
        val: NodeId,
    ) {
        self.rel_cache_mut(which)
            .put(f as u32, g as u32, level as u32, val as u32);
    }

    #[inline]
    fn rel_cache(&self, which: RelOp) -> &ComputeCache {
        match which {
            RelOp::And => &self.and_rel_cache,
            RelOp::Or => &self.or_rel_cache,
            RelOp::Not => &self.not_rel_cache,
            RelOp::Cross => &self.cross_cache,
            RelOp::Transpose => &self.transpose_cache,
            RelOp::PostImage => &self.post_cache,
            RelOp::PreImage => &self.pre_cache,
        }
    }

    #[inline]
    fn rel_cache_mut(&mut self, which: RelOp) -> &mut ComputeCache {
        match which {
            RelOp::And => &mut self.and_rel_cache,
            RelOp::Or => &mut self.or_rel_cache,
            RelOp::Not => &mut self.not_rel_cache,
            RelOp::Cross => &mut self.cross_cache,
            RelOp::Transpose => &mut self.transpose_cache,
            RelOp::PostImage => &mut self.post_cache,
            RelOp::PreImage => &mut self.pre_cache,
        }
    }

    #[inline]
    pub fn clear_cache(&mut self) {
        self.set_cache.clear();
        self.and_rel_cache.clear();
        self.or_rel_cache.clear();
        self.not_rel_cache.clear();
        self.cross_cache.clear();
        self.transpose_cache.clear();
        self.post_cache.clear();
        self.pre_cache.clear();
    }
}

/// Selects which of the per-operation relation tables to use. Relation operations
/// cannot share one op-keyed table because the level occupies a key word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelOp {
    And,
    Or,
    Not,
    Cross,
    Transpose,
    PostImage,
    PreImage,
}

impl HeaderKind {
    #[inline]
    pub fn level(&self) -> Level {
        match self {
            HeaderKind::Set(l) | HeaderKind::Rel(l) => *l,
        }
    }

    #[inline]
    pub fn is_set(&self) -> bool {
        matches!(self, HeaderKind::Set(_))
    }

    #[inline]
    pub fn is_rel(&self) -> bool {
        matches!(self, HeaderKind::Rel(_))
    }
}
