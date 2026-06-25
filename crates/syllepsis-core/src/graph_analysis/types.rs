use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphMode {
    Categories,
    Pillars,
    Communities,
    Density,
    Timeline,
}

/// Which metadata date drives a note's position on the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineDateField {
    Created,
    Updated,
    Scheduled,
    Completed,
}

/// Time-bucket granularity for the timeline. `Auto` is resolved from the date span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineGranularity {
    Auto,
    Hour,
    Day,
    Month,
    Year,
}

/// How timeline nodes are colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineColorBy {
    Category,
    Cluster,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphAnalysisRequest {
    pub mode: GraphMode,
    pub automatic_cluster_defaults: bool,
    pub umap_neighbors: usize,
    pub kmeans_k: usize,
    pub louvain_resolution: f64,
    pub hdbscan_min_cluster_size: usize,
    pub timeline_primary_date: TimelineDateField,
    pub timeline_fallback_date: Option<TimelineDateField>,
    pub timeline_granularity: TimelineGranularity,
    pub timeline_color_by: TimelineColorBy,
}

impl Default for GraphAnalysisRequest {
    fn default() -> Self {
        Self {
            mode: GraphMode::Categories,
            automatic_cluster_defaults: true,
            umap_neighbors: 15,
            kmeans_k: 5,
            louvain_resolution: 1.0,
            hdbscan_min_cluster_size: 5,
            timeline_primary_date: TimelineDateField::Created,
            timeline_fallback_date: Some(TimelineDateField::Created),
            timeline_granularity: TimelineGranularity::Auto,
            timeline_color_by: TimelineColorBy::Category,
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

/// Axis metadata for the timeline mode. `None` for every other mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTimelineMeta {
    /// Epoch millis of the true axis start (earliest dated note's bucket).
    pub start_ms: i64,
    /// Epoch millis of the true axis end (latest dated note's bucket).
    pub end_ms: i64,
    /// Normalized x (same scale as `node.x`) of ~the 1st percentile — initial camera left.
    pub focus_start_x: f32,
    /// Normalized x of ~the 99th percentile — initial camera right.
    pub focus_end_x: f32,
    /// Granularity actually used (`Auto` resolved to a concrete bucket size).
    pub granularity: TimelineGranularity,
    pub ticks: Vec<GraphTimelineTick>,
    /// Notes with no resolvable date, parked in the left "undated" lane.
    pub undated_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTimelineTick {
    pub at_ms: i64,
    pub label: String,
    /// Normalized x on the same scale as `node.x`.
    pub x: f32,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<GraphTimelineMeta>,
}
