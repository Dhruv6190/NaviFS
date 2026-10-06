//! In-memory knowledge graph representation and traversal for NaviFS

use navifs_core::{EntityId, EntityNode, RelationEdge};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory graph engine managing entities and directional relationships
#[derive(Debug, Default, Clone)]
pub struct KnowledgeGraph {
    nodes: Arc<RwLock<HashMap<EntityId, EntityNode>>>,
    // source_id -> list of edges
    outgoing: Arc<RwLock<HashMap<EntityId, Vec<RelationEdge>>>>,
    // target_id -> list of edges
    incoming: Arc<RwLock<HashMap<EntityId, Vec<RelationEdge>>>>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            outgoing: Arc::new(RwLock::new(HashMap::new())),
            incoming: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_node(&self, node: EntityNode) {
        let mut nodes = self.nodes.write().await;
        nodes.insert(node.id, node);
    }

    pub async fn add_nodes(&self, new_nodes: &[EntityNode]) {
        let mut nodes = self.nodes.write().await;
        for n in new_nodes {
            nodes.insert(n.id, n.clone());
        }
    }

    pub async fn add_edge(&self, edge: RelationEdge) {
        let mut out = self.outgoing.write().await;
        let mut inc = self.incoming.write().await;

        out.entry(edge.source_id).or_default().push(edge.clone());
        inc.entry(edge.target_id).or_default().push(edge);
    }

    pub async fn add_edges(&self, edges: &[RelationEdge]) {
        let mut out = self.outgoing.write().await;
        let mut inc = self.incoming.write().await;

        for edge in edges {
            out.entry(edge.source_id).or_default().push(edge.clone());
            inc.entry(edge.target_id).or_default().push(edge.clone());
        }
    }

    pub async fn get_node(&self, id: &EntityId) -> Option<EntityNode> {
        let nodes = self.nodes.read().await;
        nodes.get(id).cloned()
    }

    pub async fn get_outgoing_edges(&self, id: &EntityId) -> Vec<RelationEdge> {
        let out = self.outgoing.read().await;
        out.get(id).cloned().unwrap_or_default()
    }

    pub async fn get_incoming_edges(&self, id: &EntityId) -> Vec<RelationEdge> {
        let inc = self.incoming.read().await;
        inc.get(id).cloned().unwrap_or_default()
    }

    /// Breadth-First-Search (BFS) to discover related neighborhood entities up to max_depth
    pub async fn traverse_neighborhood(
        &self,
        start_id: &EntityId,
        max_depth: usize,
    ) -> (Vec<EntityNode>, Vec<RelationEdge>) {
        let nodes_map = self.nodes.read().await;
        let out_map = self.outgoing.read().await;

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result_nodes = Vec::new();
        let mut result_edges = Vec::new();

        if let Some(start_node) = nodes_map.get(start_id) {
            visited.insert(*start_id);
            queue.push_back((*start_id, 0));
            result_nodes.push(start_node.clone());
        }

        while let Some((curr_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            if let Some(edges) = out_map.get(&curr_id) {
                for edge in edges {
                    result_edges.push(edge.clone());
                    if !visited.contains(&edge.target_id) {
                        visited.insert(edge.target_id);
                        if let Some(target_node) = nodes_map.get(&edge.target_id) {
                            result_nodes.push(target_node.clone());
                        }
                        queue.push_back((edge.target_id, depth + 1));
                    }
                }
            }
        }

        (result_nodes, result_edges)
    }

    /// Renders current graph as a GraphViz DOT script for visualization
    pub async fn to_dot(&self) -> String {
        let nodes = self.nodes.read().await;
        let out = self.outgoing.read().await;

        let mut dot = String::from(
            "digraph NaviFS_KnowledgeGraph {\n  rankdir=LR;\n  node [shape=box, style=rounded];\n",
        );

        for (id, node) in nodes.iter() {
            let label = format!(
                "{}\\n({:?})",
                node.name.replace('"', "\\\""),
                node.entity_type
            );
            dot.push_str(&format!("  \"{}\" [label=\"{}\"];\n", id, label));
        }

        for edges in out.values() {
            for edge in edges {
                dot.push_str(&format!(
                    "  \"{}\" -> \"{}\" [label=\"{:?}\"];\n",
                    edge.source_id, edge.target_id, edge.relation_type
                ));
            }
        }

        dot.push_str("}\n");
        dot
    }
}
