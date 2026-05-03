use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use petgraph::visit::{Bfs, Reversed, Walker};

use crate::sgraph::graph::{NodeId, StateGraph};

/// Tracks which cache keys currently hold valid data.
pub struct CacheIndex {
    valid: HashSet<u64>,
}

impl CacheIndex {
    pub fn new() -> Self {
        Self {
            valid: HashSet::new(),
        }
    }

    pub fn is_valid(&self, key: u64) -> bool {
        self.valid.contains(&key)
    }

    pub fn mark_valid(&mut self, key: u64) {
        self.valid.insert(key);
    }

    pub fn invalidate(&mut self, key: u64) {
        self.valid.remove(&key);
    }
}

/// Compute a content-addressable key for `node`'s output.
///
/// The key hashes the node's params **and** every transitive predecessor —
/// any change upstream of the node invalidates the cache entry. Hashing only
/// the predecessor closure (rather than the whole graph) keeps unrelated
/// edits, e.g. adding a downstream sink, from busting the cache.
pub fn cache_key(graph: &StateGraph, node: NodeId) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    // BFS over the reversed graph collects every ancestor, including `node`
    // itself. Sort the result so the hash doesn't depend on visit order.
    let reversed = Reversed(&graph.graph);
    let mut closure: Vec<NodeId> = Bfs::new(&reversed, node).iter(&reversed).collect();
    closure.sort_unstable_by_key(|n| n.index());

    for id in closure {
        graph.graph[id]
            .serialize_params()
            .to_string()
            .hash(&mut hasher);
    }
    hasher.finish()
}
