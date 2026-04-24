use crate::callgraph::CallGraph;
use crate::{Error, Result};
use dk_core::SymbolId;
use dkod_worktree::SymbolRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Warnings produced alongside a partition result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Warning {
    /// Partitioner produced fewer groups than the target — usually because
    /// coupling is too dense or the scope is small.
    FewerGroupsThanTarget { target: usize, got: usize },
    /// More connected components than the target.  V0 does not subdivide;
    /// balancing is a future optimisation.
    MoreGroupsThanTarget { target: usize, got: usize },
    /// An `in_scope` qualified name did not resolve to a known symbol.
    ScopeSymbolUnknown { name: String },
}

/// A single group of symbols that belong together (one connected component).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    /// Sorted by `qualified_name` for stable golden output.
    pub symbols: Vec<SymbolRef>,
}

/// The output of `partition`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partition {
    /// Sorted by group id (`g1`, `g2`, …).
    pub groups: Vec<Group>,
    pub warnings: Vec<Warning>,
}

// ── Union-Find ────────────────────────────────────────────────────────────────

struct UnionFind {
    parent: HashMap<SymbolId, SymbolId>,
}

impl UnionFind {
    fn new<'a>(ids: impl IntoIterator<Item = &'a SymbolId>) -> Self {
        let parent = ids.into_iter().map(|id| (*id, *id)).collect();
        Self { parent }
    }

    /// Find with two-pass path compression.
    fn find(&mut self, x: &SymbolId) -> SymbolId {
        // First pass: walk to root.
        let mut cur = *x;
        loop {
            let p = self.parent.get(&cur).copied().unwrap_or(cur);
            if p == cur {
                break;
            }
            cur = p;
        }
        let root = cur;
        // Second pass: point all traversed nodes directly at root.
        let mut cur = *x;
        while cur != root {
            let p = self.parent.get(&cur).copied().unwrap_or(root);
            self.parent.insert(cur, root);
            cur = p;
        }
        root
    }

    fn union(&mut self, a: &SymbolId, b: &SymbolId) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Partition `in_scope` symbols into connected components using the undirected
/// call graph.
///
/// * `in_scope` — qualified names to consider.  Names that do not resolve
///   produce a [`Warning::ScopeSymbolUnknown`] and are silently skipped.
/// * `graph` — the resolved call graph.
/// * `target_groups` — desired number of groups (≥ 1). Mismatches between the
///   actual number of connected components and this hint produce
///   `FewerGroupsThanTarget` / `MoreGroupsThanTarget` warnings; the actual
///   partition is never artificially inflated or deflated.
///
/// Returns an empty `Partition` (no error) when `in_scope` is empty or all
/// names are unknown.
pub fn partition(
    in_scope: &[String],
    graph: &CallGraph,
    target_groups: usize,
) -> Result<Partition> {
    if target_groups == 0 {
        return Err(Error::InvalidPartition(
            "target_groups must be >= 1".into(),
        ));
    }

    // ── Resolve names → SymbolIds ─────────────────────────────────────────
    let mut resolved: Vec<SymbolId> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();
    let mut in_scope_set: HashSet<SymbolId> = HashSet::new();

    for name in in_scope {
        match graph.symbol_id_by_name(name) {
            Some(id) => {
                if in_scope_set.insert(id) {
                    resolved.push(id);
                }
            }
            None => warnings.push(Warning::ScopeSymbolUnknown { name: name.clone() }),
        }
    }

    if resolved.is_empty() {
        return Ok(Partition {
            groups: Vec::new(),
            warnings,
        });
    }

    // ── Union coupled symbols (undirected, restricted to in-scope) ────────
    let mut uf = UnionFind::new(resolved.iter());
    for id in &resolved {
        for n in graph.undirected_neighbours(id) {
            if in_scope_set.contains(n) {
                uf.union(id, n);
            }
        }
    }

    // ── Group by representative ───────────────────────────────────────────
    // BTreeMap for deterministic iteration order across runs.
    let mut buckets: BTreeMap<SymbolId, Vec<SymbolId>> = BTreeMap::new();
    for id in &resolved {
        let root = uf.find(id);
        buckets.entry(root).or_default().push(*id);
    }

    let groups: Vec<Group> = buckets
        .into_values()
        .enumerate()
        .map(|(i, members)| {
            let gid = format!("g{}", i + 1);
            let mut symbols: Vec<SymbolRef> = members
                .iter()
                .filter_map(|sid| graph.symbol(sid))
                .map(|s| SymbolRef {
                    qualified_name: s.qualified_name.clone(),
                    file_path: s.file_path.clone(),
                    // Use Display (not Debug) so the kind is "function" not "Function"
                    kind: s.kind.to_string(),
                })
                .collect();
            // Stable ordering within a group.
            symbols.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
            Group { id: gid, symbols }
        })
        .collect();

    // ── Target-count warnings ─────────────────────────────────────────────
    match groups.len().cmp(&target_groups) {
        std::cmp::Ordering::Less => warnings.push(Warning::FewerGroupsThanTarget {
            target: target_groups,
            got: groups.len(),
        }),
        std::cmp::Ordering::Greater => warnings.push(Warning::MoreGroupsThanTarget {
            target: target_groups,
            got: groups.len(),
        }),
        std::cmp::Ordering::Equal => {}
    }

    Ok(Partition { groups, warnings })
}
