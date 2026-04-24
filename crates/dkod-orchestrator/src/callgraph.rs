use dk_core::{RawCallEdge, Symbol, SymbolId};
use std::collections::{HashMap, HashSet};

pub struct CallGraph {
    /// Qualified name → id (qualified_name wins; short name is fallback)
    symbol_index: HashMap<String, SymbolId>,
    by_id: HashMap<SymbolId, Symbol>,
    /// Directed: caller → {callees}
    adj: HashMap<SymbolId, HashSet<SymbolId>>,
    /// Undirected: used by the partitioner for connected-component grouping
    undirected: HashMap<SymbolId, HashSet<SymbolId>>,
    unresolved: usize,
}

impl CallGraph {
    /// Build a resolved call graph from a symbol list and raw edges.
    ///
    /// Edges whose caller or callee does not resolve to a known symbol are
    /// counted in `unresolved_count` and silently dropped from the adjacency
    /// maps — they represent calls to external dependencies and are normal.
    /// Self-loops are filtered so that a recursive function does not end up
    /// coupled to itself in the partitioner.
    pub fn build(symbols: &[Symbol], edges: &[RawCallEdge]) -> Self {
        let mut symbol_index = HashMap::new();
        let mut by_id = HashMap::new();
        for s in symbols {
            symbol_index.insert(s.qualified_name.clone(), s.id);
            // Short name is a fallback; qualified name wins if both exist.
            symbol_index.entry(s.name.clone()).or_insert(s.id);
            by_id.insert(s.id, s.clone());
        }

        let mut adj: HashMap<SymbolId, HashSet<SymbolId>> = HashMap::new();
        let mut undirected: HashMap<SymbolId, HashSet<SymbolId>> = HashMap::new();
        let mut unresolved = 0usize;

        for e in edges {
            let (Some(&caller), Some(&callee)) = (
                symbol_index.get(&e.caller_name),
                symbol_index.get(&e.callee_name),
            ) else {
                unresolved += 1;
                continue;
            };

            // Filter self-loops — a recursive call does not mean the symbol
            // should be coupled to anything outside itself.
            if caller == callee {
                continue;
            }

            adj.entry(caller).or_default().insert(callee);
            undirected.entry(caller).or_default().insert(callee);
            undirected.entry(callee).or_default().insert(caller);
        }

        Self {
            symbol_index,
            by_id,
            adj,
            undirected,
            unresolved,
        }
    }

    /// Look up a symbol id by qualified name or short name.
    pub fn symbol_id_by_name(&self, name: &str) -> Option<SymbolId> {
        self.symbol_index.get(name).copied()
    }

    /// Directed successors (symbols called by `id`).
    pub fn successors(&self, id: SymbolId) -> Vec<SymbolId> {
        self.adj
            .get(&id)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Undirected neighbours of `id` — used by the partitioner.
    pub fn undirected_neighbours(&self, id: &SymbolId) -> impl Iterator<Item = &SymbolId> {
        self.undirected.get(id).into_iter().flatten()
    }

    /// Number of edges that could not be resolved to known symbols.
    pub fn unresolved_count(&self) -> usize {
        self.unresolved
    }

    /// Look up a symbol by its id.
    pub fn symbol(&self, id: &SymbolId) -> Option<&Symbol> {
        self.by_id.get(id)
    }

    /// Iterate over all symbols in the graph.
    pub fn all_symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.by_id.values()
    }
}
