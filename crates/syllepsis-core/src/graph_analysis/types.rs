use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphMode {
    Categories,
    Pillars,
    Communities,
    Density,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphAnalysisRequest {
    pub mode: GraphMode,
    pub umap_neighbors: usize,
    pub kmeans_k: usize,
    pub louvain_resolution: f64,
    pub hdbscan_min_cluster_size: usize,
}

impl Default for GraphAnalysisRequest {
    fn default() -> Self {
        Self {
            mode: GraphMode::Categories,
            umap_neighbors: 15,
            kmeans_k: 5,
            louvain_resolution: 1.0,
            hdbscan_min_cluster_size: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphAnalysisNode {
    pub id: String,
    pub title: String,
    pub categories: Vec<String>,
    pub x: f32,
    pub y: f32,
    pub cluster_id: Option<usize>,
    pub outlier: bool,
    pub no_semantic_signal: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphCluster {
    pub id: usize,
    pub label: String,
    pub node_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphSemanticEdge {
    pub source: String,
    pub target: String,
    pub similarity: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphPriorEdge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphProviderMetadata {
    pub id: String,
    pub semantic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphAnalysisSummary {
    pub note_count: usize,
    pub embedded_note_count: usize,
    pub cluster_count: usize,
    pub outlier_count: usize,
    pub no_signal_count: usize,
    pub semantic_edge_candidate_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphAnalysisResult {
    pub mode: GraphMode,
    pub nodes: Vec<GraphAnalysisNode>,
    pub clusters: Vec<GraphCluster>,
    pub semantic_edges: Vec<GraphSemanticEdge>,
    pub prior_edges: Vec<GraphPriorEdge>,
    pub provider: GraphProviderMetadata,
    pub summary: GraphAnalysisSummary,
}
