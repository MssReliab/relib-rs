use common::prelude::*;

/// Non-terminal MxD node.
///
/// Structurally identical to `mddcore::NonTerminalMDD` (same u32-narrowed storage,
/// same flat child vector); it is duplicated here rather than imported so that
/// `mxdcore` depends only on `common` and not on the whole MDD engine.
///
/// The child vector is interpreted by the node's header:
///
/// - **set node** (`edge_num == n`): child `a` is the sub-diagram for `x_i = a`.
/// - **relation node** (`edge_num == n * n`): child `a * n + b` is the sub-diagram
///   for the transition `(from_i = a, to_i = b)`. The two levels MEDDLY spends on
///   an unprimed/primed pair are fused into this one node — see the crate docs.
///
/// Like `NonTerminalMDD`, this intentionally does NOT implement `common::NonTerminal`
/// (whose `Index`/`iter` hand out references to `NodeId`, which u32 storage cannot
/// provide); it exposes value-returning inherent accessors instead.
#[derive(Debug)]
pub struct NonTerminalMxD {
    id: u32,
    header: u32,
    nodes: Box<[u32]>,
}

impl NonTerminalMxD {
    pub fn new(id: NodeId, header: HeaderId, nodes: &[NodeId]) -> Self {
        Self {
            id: id as u32,
            header: header as u32,
            nodes: nodes.iter().map(|&x| x as u32).collect(),
        }
    }

    #[inline]
    pub fn id(&self) -> NodeId {
        self.id as NodeId
    }

    #[inline]
    pub fn headerid(&self) -> HeaderId {
        self.header as HeaderId
    }

    /// Returns the `i`-th child as a `NodeId`.
    #[inline]
    pub fn edge(&self, i: usize) -> NodeId {
        self.nodes[i] as NodeId
    }

    /// Number of children (edge count).
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Iterates the children as `NodeId` values.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().map(|&x| x as NodeId)
    }
}
