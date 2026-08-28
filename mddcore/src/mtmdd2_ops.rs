use common::prelude::*;
use crate::nodes::*;
use crate::mdd;
use crate::mtmdd;
use crate::mtmdd2::*;

type VNode<V> = mtmdd::Node<V>;
type BNode = mdd::Node;

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum MtMdd2Operation {
    Eq,
    Lt,
    // LtE,
    // Gt,
    // GtE,
    If,
}

impl MtMdd2Operation {
    /// Compact code for use as a computed-table key word.
    #[inline]
    pub(crate) fn code(&self) -> u32 {
        match self {
            MtMdd2Operation::Eq => 0,
            MtMdd2Operation::Lt => 1,
            MtMdd2Operation::If => 2,
        }
    }
}

impl<V> MtMdd2Manager<V>
where
    V: MddValue,
{
    pub fn and(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Bool(fnode), Node::Bool(gnode)) => Node::Bool(self.mdd_mut().and(fnode, gnode)),
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn or(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Bool(fnode), Node::Bool(gnode)) => Node::Bool(self.mdd_mut().or(fnode, gnode)),
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn xor(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Bool(fnode), Node::Bool(gnode)) => Node::Bool(self.mdd_mut().xor(fnode, gnode)),
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn not(&mut self, f: Node) -> Node {
        match f {
            Node::Bool(fnode) => Node::Bool(self.mdd_mut().not(fnode)),
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn add(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => Node::Value(self.mtmdd_mut().add(fnode, gnode)),
            _ => Node::Value(self.mtmdd().undet()),
        }
    }

    pub fn sub(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => Node::Value(self.mtmdd_mut().sub(fnode, gnode)),
            _ => Node::Value(self.mtmdd().undet()),
        }
    }

    pub fn mul(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => Node::Value(self.mtmdd_mut().mul(fnode, gnode)),
            _ => Node::Value(self.mtmdd().undet()),
        }
    }

    pub fn div(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => Node::Value(self.mtmdd_mut().div(fnode, gnode)),
            _ => Node::Value(self.mtmdd().undet()),
        }
    }

    pub fn max(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => Node::Value(self.mtmdd_mut().max(fnode, gnode)),
            _ => Node::Value(self.mtmdd().undet()),
        }
    }

    pub fn min(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => Node::Value(self.mtmdd_mut().min(fnode, gnode)),
            _ => Node::Value(self.mtmdd().undet()),
        }
    }
}

impl<V> MtMdd2Manager<V>
where
    V: MddValue,
{
    pub fn eq(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Bool(fnode), Node::Bool(gnode)) => {
                let tmp = self.mdd_mut().xor(fnode, gnode);
                Node::Bool(self.mdd_mut().not(tmp))
            }
            (Node::Value(fnode), Node::Value(gnode)) => Node::Bool(self.veq(fnode, gnode)),
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn veq(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let key = (MtMdd2Operation::Eq, f, g);
        if let Some(x) = self.bcache_get(&key) {
            return x;
        }
        let node = match (
            self.mtmdd().get_node(&f).unwrap(),
            self.mtmdd().get_node(&g).unwrap(),
        ) {
            (VNode::Undet, _) => self.mdd().zero(),
            (_, VNode::Undet) => self.mdd().zero(),
            (VNode::Terminal(fnode), VNode::Terminal(gnode)) if fnode.value() == gnode.value() => {
                self.mdd().one()
            }
            (VNode::Terminal(_), VNode::Terminal(_)) => self.mdd().zero(),
            (VNode::Terminal(_fnode), VNode::NonTerminal(gnode)) => {
                let headerid = gnode.headerid();
                let gnodeid: Vec<_> = gnode.iter().collect();
                let nodes: Vec<_> = gnodeid.into_iter().map(|g| self.veq(f, g)).collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
            (VNode::NonTerminal(fnode), VNode::Terminal(_gnode)) => {
                let headerid = fnode.headerid();
                let fnodeid: Vec<_> = fnode.iter().collect();
                let nodes: Vec<_> = fnodeid.into_iter().map(|f| self.veq(f, g)).collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
            (VNode::NonTerminal(fnode), VNode::NonTerminal(_gnode))
                if self.mtmdd().level(&f) > self.mtmdd().level(&g) =>
            {
                let headerid = fnode.headerid();
                let fnodeid: Vec<_> = fnode.iter().collect();
                let nodes: Vec<_> = fnodeid.into_iter().map(|f| self.veq(f, g)).collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
            (VNode::NonTerminal(_fnode), VNode::NonTerminal(gnode))
                if self.mtmdd().level(&f) < self.mtmdd().level(&g) =>
            {
                let headerid = gnode.headerid();
                let gnodeid: Vec<_> = gnode.iter().collect();
                let nodes: Vec<_> = gnodeid.into_iter().map(|g| self.veq(f, g)).collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
            (VNode::NonTerminal(fnode), VNode::NonTerminal(gnode)) => {
                let headerid = fnode.headerid();
                let fnodeid: Vec<_> = fnode.iter().collect();
                let gnodeid: Vec<_> = gnode.iter().collect();
                let nodes: Vec<_> = fnodeid
                    .into_iter()
                    .zip(gnodeid.into_iter())
                    .map(|(f, g)| self.veq(f, g))
                    .collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
        };
        self.bcache_put(key, node);
        node
    }

    pub fn neq(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Bool(fnode), Node::Bool(gnode)) => Node::Bool(self.mdd_mut().xor(fnode, gnode)),
            (Node::Value(fnode), Node::Value(gnode)) => {
                let tmp = self.veq(fnode, gnode);
                Node::Bool(self.mdd_mut().not(tmp))
            }
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn lt(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => Node::Bool(self.vlt(fnode, gnode)),
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn vlt(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let key = (MtMdd2Operation::Lt, f, g);
        if let Some(x) = self.bcache_get(&key) {
            return x;
        }
        let node = match (
            self.mtmdd().get_node(&f).unwrap(),
            self.mtmdd().get_node(&g).unwrap(),
        ) {
            (VNode::Undet, _) => self.mdd().zero(),
            (_, VNode::Undet) => self.mdd().zero(),
            (VNode::Terminal(fnode), VNode::Terminal(gnode)) if fnode.value() < gnode.value() => {
                self.mdd().one()
            }
            (VNode::Terminal(_fnode), VNode::Terminal(_gnode)) => self.mdd().zero(),
            (VNode::Terminal(_fnode), VNode::NonTerminal(gnode)) => {
                let headerid = gnode.headerid();
                let gnodeid: Vec<_> = gnode.iter().collect();
                let nodes: Vec<_> = gnodeid.into_iter().map(|g| self.vlt(f, g)).collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
            (VNode::NonTerminal(fnode), VNode::Terminal(_gnode)) => {
                let headerid = fnode.headerid();
                let fnodeid: Vec<_> = fnode.iter().collect();
                let nodes: Vec<_> = fnodeid.into_iter().map(|f| self.vlt(f, g)).collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
            (VNode::NonTerminal(fnode), VNode::NonTerminal(_gnode))
                if self.mtmdd().level(&f) > self.mtmdd().level(&g) =>
            {
                let headerid = fnode.headerid();
                let fnodeid: Vec<_> = fnode.iter().collect();
                let nodes: Vec<_> = fnodeid.into_iter().map(|f| self.vlt(f, g)).collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
            (VNode::NonTerminal(_fnode), VNode::NonTerminal(gnode))
                if self.mtmdd().level(&f) < self.mtmdd().level(&g) =>
            {
                let headerid = gnode.headerid();
                let gnodeid: Vec<_> = gnode.iter().collect();
                let nodes: Vec<_> = gnodeid.into_iter().map(|g| self.vlt(f, g)).collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
            (VNode::NonTerminal(fnode), VNode::NonTerminal(gnode)) => {
                let headerid = fnode.headerid();
                let fnodeid: Vec<_> = fnode.iter().collect();
                let gnodeid: Vec<_> = gnode.iter().collect();
                let nodes: Vec<_> = fnodeid
                    .into_iter()
                    .zip(gnodeid.into_iter())
                    .map(|(f, g)| self.vlt(f, g))
                    .collect();
                self.mdd_mut().create_node(headerid, &nodes)
            }
        };
        self.bcache_put(key, node);
        node
    }

    pub fn lte(&mut self, f: Node, g: Node) -> Node {
        // f <= g is the complement of g < f, mirroring `gte` (which is the complement
        // of f < g). Taking `veq` and only short-circuiting when it is the constant-one
        // node is wrong: `veq` is a diagram, not a boolean, so `x == y` being true on
        // *some* assignments never trips that check, and the equal case was dropped for
        // every non-degenerate pair -- `x <= y` computed `x < y`.
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => {
                let resultgt = self.vlt(gnode, fnode);
                let result = self.mdd_mut().not(resultgt);
                Node::Bool(result)
            }
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn gt(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => {
                let tmp = self.vlt(gnode, fnode);
                Node::Bool(tmp)
            }
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn gte(&mut self, f: Node, g: Node) -> Node {
        match (f, g) {
            (Node::Value(fnode), Node::Value(gnode)) => {
                let resultlt = self.vlt(fnode, gnode);
                let result = self.mdd_mut().not(resultlt);
                Node::Bool(result)
            }
            _ => Node::Bool(self.mdd().undet()),
        }
    }

    pub fn ite(&mut self, f: Node, g: Node, h: Node) -> Node {
        match (f, g, h) {
            (Node::Bool(fnode), Node::Value(gnode), Node::Value(hnode)) => {
                Node::Value(self.vite(fnode, gnode, hnode))
            }
            (Node::Bool(fnode), Node::Bool(gnode), Node::Bool(hnode)) => {
                let result = self.mdd_mut().ite(fnode, gnode, hnode);
                Node::Bool(result)
            }
            (_, Node::Value(_gnode), Node::Value(_hnode)) => {
                let result = self.mtmdd().undet();
                Node::Value(result)
            }
            (_, Node::Bool(_gnode), Node::Bool(_hnode)) => {
                let result = self.mdd().undet();
                Node::Bool(result)
            }
            _ => panic!("ite: unexpected pattern."),
        }
    }

    pub fn vif(&mut self, f: NodeId, g: NodeId) -> NodeId {
        let key = (MtMdd2Operation::If, f, g);
        if let Some(x) = self.vcache_get(&key) {
            return x;
        }
        let node = match (
            self.mdd().get_node(&f).unwrap(),
            self.mtmdd().get_node(&g).unwrap(),
        ) {
            (BNode::Undet, _) => self.mtmdd().undet(),
            (_, VNode::Undet) => self.mtmdd().undet(),
            (BNode::Zero, _) => self.mtmdd().undet(),
            (BNode::One, _) => g,
            (BNode::NonTerminal(fnode), VNode::Terminal(_gnode)) => {
                let headerid = fnode.headerid();
                let fnodeid: Vec<_> = fnode.iter().collect();
                let nodes: Vec<_> = fnodeid.into_iter().map(|f| self.vif(f, g)).collect();
                self.mtmdd_mut().create_node(headerid, &nodes)
            }
            (BNode::NonTerminal(fnode), VNode::NonTerminal(_gnode))
                if self.mdd().level(&f) > self.mtmdd().level(&g) =>
            {
                let headerid = fnode.headerid();
                let fnodeid: Vec<_> = fnode.iter().collect();
                let nodes: Vec<_> = fnodeid.into_iter().map(|f| self.vif(f, g)).collect();
                self.mtmdd_mut().create_node(headerid, &nodes)
            }
            (BNode::NonTerminal(_fnode), VNode::NonTerminal(gnode))
                if self.mdd().level(&f) < self.mtmdd().level(&g) =>
            {
                let headerid = gnode.headerid();
                let gnodeid: Vec<_> = gnode.iter().collect();
                let nodes: Vec<_> = gnodeid.into_iter().map(|g| self.vif(f, g)).collect();
                self.mtmdd_mut().create_node(headerid, &nodes)
            }
            (BNode::NonTerminal(fnode), VNode::NonTerminal(gnode)) => {
                let headerid = fnode.headerid();
                let fnodeid: Vec<_> = fnode.iter().collect();
                let gnodeid: Vec<_> = gnode.iter().collect();
                let nodes: Vec<_> = fnodeid
                    .into_iter()
                    .zip(gnodeid.into_iter())
                    .map(|(f, g)| self.vif(f, g))
                    .collect();
                self.mtmdd_mut().create_node(headerid, &nodes)
            }
        };
        self.vcache_put(key, node);
        node
    }

    /// Native value-side `ite`: "if boolean `f` then value `g` else value `h`".
    ///
    /// `f` lives in the boolean (mdd) forest, `g`/`h`/result in the value
    /// (mtmdd) forest; the two share the same headers/levels (see
    /// `create_header`). A single ternary Shannon recursion — the three-operand
    /// generalization of `vif` — replacing the old `replace(vif(f,g), vif(!f,h))`
    /// (not + 2×vif + replace = four cross-forest passes). Semantics match: from
    /// `vif`'s terminal arms, `f == One → g`, `f == Zero → h`, `f == Undet → undet`.
    pub(crate) fn vite(&mut self, f: NodeId, g: NodeId, h: NodeId) -> NodeId {
        match self.mdd().get_node(&f).unwrap() {
            BNode::One => return g,
            BNode::Zero => return h,
            BNode::Undet => return self.mtmdd().undet(),
            BNode::NonTerminal(_) => {}
        }
        if g == h {
            return g;
        }
        if let Some(x) = self.vite_cache_get(f, g, h) {
            return x;
        }

        // Top variable = highest real level among the operands (`f` is a
        // non-terminal bool node, so `mdd.level(f)` seeds it).
        let mut top = self.mdd().level(&f).unwrap();
        if let Some(lg) = self.mtmdd().level(&g) {
            if lg > top {
                top = lg;
            }
        }
        if let Some(lh) = self.mtmdd().level(&h) {
            if lh > top {
                top = lh;
            }
        }
        let (headerid, k) = self.vite_top_header(f, g, h, top);
        let fc = self.vite_cofactor_b(f, top, k);
        let gc = self.vite_cofactor_v(g, top, k);
        let hc = self.vite_cofactor_v(h, top, k);
        let nodes: Vec<NodeId> = (0..k).map(|i| self.vite(fc[i], gc[i], hc[i])).collect();
        let result = self.mtmdd_mut().create_node(headerid, &nodes);
        self.vite_cache_put(f, g, h, result);
        result
    }

    /// Cofactor a boolean (mdd) operand at level `top`: its `k` children if it
    /// sits there, else replicate it across all `k` slots.
    #[inline]
    fn vite_cofactor_b(&self, id: NodeId, top: Level, k: usize) -> Vec<NodeId> {
        if self.mdd().level(&id) == Some(top) {
            if let BNode::NonTerminal(n) = self.mdd().get_node(&id).unwrap() {
                return n.iter().collect();
            }
        }
        vec![id; k]
    }

    /// Cofactor a value (mtmdd) operand at level `top`: its `k` children if it
    /// sits there, else replicate it across all `k` slots.
    #[inline]
    fn vite_cofactor_v(&self, id: NodeId, top: Level, k: usize) -> Vec<NodeId> {
        if self.mtmdd().level(&id) == Some(top) {
            if let VNode::NonTerminal(n) = self.mtmdd().get_node(&id).unwrap() {
                return n.iter().collect();
            }
        }
        vec![id; k]
    }

    /// Header id and edge count at level `top`, from whichever of the bool `f`
    /// or the value `g`/`h` sits there (they share the header). At least one does.
    #[inline]
    fn vite_top_header(&self, f: NodeId, g: NodeId, h: NodeId, top: Level) -> (HeaderId, usize) {
        if self.mdd().level(&f) == Some(top) {
            if let BNode::NonTerminal(n) = self.mdd().get_node(&f).unwrap() {
                return (n.headerid(), n.iter().count());
            }
        }
        for x in [g, h] {
            if self.mtmdd().level(&x) == Some(top) {
                if let VNode::NonTerminal(n) = self.mtmdd().get_node(&x).unwrap() {
                    return (n.headerid(), n.iter().count());
                }
            }
        }
        unreachable!("vite: top level has no matching non-terminal operand")
    }
}

