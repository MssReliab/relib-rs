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
    pub fn state_space_size(&self) -> u64 {
        self.vars.iter().map(|v| v.domain as u64).product()
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

    /// Creates a **relation** node at `level`, quasi-reduced.
    ///
    /// `block[a * n + b]` is the sub-diagram for the transition `(from = a, to = b)`.
    ///
    /// The only rule applied is "the empty block is the empty relation". In
    /// particular the fully-reduced rule (*all entries equal ⇒ elide*) must **never**
    /// be applied to a relation node: eliding a level here has to mean `to == from`
    /// (identity), whereas "all entries equal" means "`to` is anything" — the
    /// opposite. Identity reduction, which elides exactly the diagonal blocks, is a
    /// later stage; until then no relation level is ever skipped, so the forest is
    /// quasi-reduced and trivially sound.
    pub fn create_rel_node(&mut self, level: Level, block: &[NodeId]) -> NodeId {
        let v = self.vars[level];
        assert_eq!(
            block.len(),
            v.domain * v.domain,
            "relation node at level {level} needs {} children",
            v.domain * v.domain
        );
        if block.iter().all(|&x| x == self.zero) {
            return self.zero;
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

        self.freelist.clear();
        for (id, &alive) in live.iter().enumerate() {
            if !alive {
                self.freelist.push(id as u32);
            }
        }
        self.freelist.len()
    }

    /// Number of live (non-reclaimed) node slots, including terminals.
    #[inline]
    pub fn live_node_count(&self) -> usize {
        self.nodes.len() - self.freelist.len()
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
