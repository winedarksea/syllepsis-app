mod algorithms;
mod layout;
#[cfg(test)]
mod tests;
mod types;

use std::collections::{BTreeMap, HashMap, HashSet};

use sha2::{Digest, Sha256};

use crate::embeddings::{configured_model_fingerprint, load_embedding_corpus, Embedding};
use crate::error::CoreResult;
use crate::model::{Category, Note, ObjectType};
use crate::storage::{Book, NoteStore};

use algorithms::{
    cosine_distance_matrix, deterministic_kmeans, hdbscan_labels, louvain_labels, umap_layout,
};
use layout::{
    automatic_minimum_cluster_size, automatic_neighbor_count, automatic_theme_count,
    category_layout_for_notes, ensure_useful_cluster_layout,
};
pub use types::{
    GraphAnalysisNode, GraphAnalysisRequest, GraphAnalysisResult, GraphAnalysisSummary,
    GraphCluster, GraphMode, GraphPriorEdge, GraphProviderMetadata, GraphSemanticEdge,
    GraphTimelineMeta, GraphTimelineNodeDate, GraphTimelineNodeRange, GraphTimelineTick,
    TimelineColorBy, TimelineDateField, TimelineGranularity,
};

const MAX_SEMANTIC_NEIGHBORS: usize = 30;

#[derive(Debug, Clone)]
pub struct SemanticGraphCorpus {
    fingerprint: String,
    notes: Vec<Note>,
    centroids: Vec<Embedding>,
    embedded_note_indices: Vec<usize>,
    category_display_names: HashMap<String, String>,
    provider_id: String,
}

impl SemanticGraphCorpus {
    pub fn build(book: &Book) -> CoreResult<Self> {
        let mut notes: Vec<Note> = book
            .store
            .read_all_notes()?
            .into_iter()
            .filter(|note| {
                note.object_type != ObjectType::Commentary
                    && note.metadata.is_visible_in_default_views()
            })
            .collect();
        notes.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        let provider_id = configured_model_fingerprint(&book.config.embedding)?.model_id;
        let fingerprint = corpus_fingerprint(book, &notes, &embedding_source_cache_key(book))?;
        let loaded = load_embedding_corpus(book, &notes)?;
        let centroids: Vec<Embedding> = loaded
            .vectors
            .into_iter()
            .map(|vectors| vectors.centroid)
            .collect();
        let embedded_note_indices = centroids
            .iter()
            .enumerate()
            .filter_map(|(index, vector)| (vector.magnitude() > f32::EPSILON).then_some(index))
            .collect();
        let category_display_names = book
            .store
            .categories()?
            .into_iter()
            .map(|category| {
                let display = if category.long_name.trim().is_empty() {
                    category.name.clone()
                } else {
                    category.long_name
                };
                (category.name, display)
            })
            .collect();
        Ok(Self {
            fingerprint,
            notes,
            centroids,
            embedded_note_indices,
            category_display_names,
            provider_id,
        })
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn analyze(&self, request: &GraphAnalysisRequest) -> CoreResult<GraphAnalysisResult> {
        if self.notes.is_empty() {
            return Ok(empty_result(request.mode, &self.provider_id));
        }
        if request.mode == GraphMode::Timeline {
            return Ok(self.timeline_analysis(request));
        }
        if request.mode == GraphMode::Kanban {
            return Ok(self.kanban_analysis());
        }
        if request.mode == GraphMode::Categories {
            return Ok(self.embedding_coverage_fallback(GraphMode::Categories));
        }
        let embedded_vectors: Vec<Embedding> = self
            .embedded_note_indices
            .iter()
            .map(|index| self.centroids[*index].clone())
            .collect();
        let embedded_count = embedded_vectors.len();
        let embeddable_count = self
            .notes
            .iter()
            .filter(|note| !note.title.trim().is_empty() || !note.body.trim().is_empty())
            .count();
        if request.mode != GraphMode::Categories && embedded_count < embeddable_count {
            return Ok(self.embedding_coverage_fallback(request.mode));
        }
        let default_neighbors = match request.mode {
            GraphMode::Pillars => 50,
            GraphMode::Communities => 8,
            GraphMode::Density => 15,
            GraphMode::Categories | GraphMode::Timeline | GraphMode::Kanban => {
                request.umap_neighbors
            }
        };
        let requested_neighbors = if request.automatic_cluster_defaults {
            automatic_neighbor_count(request.mode, embedded_count)
        } else if request.umap_neighbors == 0 {
            default_neighbors
        } else {
            request.umap_neighbors
        }
        .clamp(2, 100);

        let (positions, embedded_cluster_labels, embedded_outliers) = match request.mode {
            GraphMode::Categories => unreachable!("category mode is handled before this match"),
            GraphMode::Pillars => {
                let positions = umap_layout(&embedded_vectors, requested_neighbors)?;
                let requested_k = if request.automatic_cluster_defaults {
                    automatic_theme_count(embedded_count)
                } else {
                    request.kmeans_k
                };
                let labels = deterministic_kmeans(
                    &embedded_vectors,
                    requested_k.clamp(2, 12).min(embedded_count.max(1)),
                );
                let optional_labels: Vec<Option<usize>> =
                    labels.iter().copied().map(Some).collect();
                (
                    ensure_useful_cluster_layout(positions, &optional_labels),
                    optional_labels,
                    vec![false; embedded_count],
                )
            }
            GraphMode::Communities => {
                let positions = umap_layout(&embedded_vectors, requested_neighbors)?;
                let distances = cosine_distance_matrix(&embedded_vectors);
                let mut labels =
                    louvain_labels(&distances, requested_neighbors, request.louvain_resolution);
                if request.automatic_cluster_defaults
                    && embedded_count >= 4
                    && labels.iter().copied().collect::<HashSet<_>>().len() < 2
                {
                    // A fully connected small corpus can make Louvain's default resolution return
                    // one community. Preserve the embeddings-based result, but use deterministic
                    // k-means as the automatic presentation fallback so "Communities" remains
                    // informative. Manual mode always exposes the raw Louvain parameters.
                    labels = deterministic_kmeans(
                        &embedded_vectors,
                        automatic_theme_count(embedded_count),
                    );
                }
                let optional_labels: Vec<Option<usize>> =
                    labels.iter().copied().map(Some).collect();
                (
                    ensure_useful_cluster_layout(positions, &optional_labels),
                    optional_labels,
                    vec![false; embedded_count],
                )
            }
            GraphMode::Density => {
                let positions = umap_layout(&embedded_vectors, requested_neighbors)?;
                let minimum_cluster_size = if request.automatic_cluster_defaults {
                    automatic_minimum_cluster_size(embedded_count)
                } else {
                    request.hdbscan_min_cluster_size
                };
                let labels = hdbscan_labels(&embedded_vectors, minimum_cluster_size.clamp(2, 50))?;
                let outliers = labels.iter().map(Option::is_none).collect();
                (
                    ensure_useful_cluster_layout(positions, &labels),
                    labels,
                    outliers,
                )
            }
            GraphMode::Timeline => unreachable!("timeline mode is handled before this match"),
            GraphMode::Kanban => unreachable!("kanban mode is handled before this match"),
        };

        let mut full_positions = vec![(0.0, 0.0); self.notes.len()];
        let mut full_labels = vec![None; self.notes.len()];
        let mut full_outliers = vec![false; self.notes.len()];
        for (embedded_index, note_index) in self.embedded_note_indices.iter().enumerate() {
            full_positions[*note_index] = positions[embedded_index];
            full_labels[*note_index] = embedded_cluster_labels[embedded_index];
            full_outliers[*note_index] = embedded_outliers[embedded_index];
        }
        place_no_signal_notes(
            &mut full_positions,
            &self.embedded_note_indices,
            self.notes.len(),
        );

        let clusters = cluster_descriptions(
            request.mode,
            &self.notes,
            &full_labels,
            &self.category_display_names,
        );
        let semantic_edges =
            semantic_edges(&self.notes, &self.centroids, &self.embedded_note_indices);
        let prior_edges = prior_edges(&self.notes);
        let nodes = self
            .notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                graph_analysis_node(
                    note,
                    full_positions[index].0,
                    full_positions[index].1,
                    full_labels[index],
                    full_outliers[index],
                    self.centroids[index].magnitude() <= f32::EPSILON,
                    None,
                    None,
                )
            })
            .collect();
        let outlier_count = full_outliers.iter().filter(|outlier| **outlier).count();
        let no_signal_count = self.notes.len() - embedded_count;
        Ok(GraphAnalysisResult {
            mode: request.mode,
            nodes,
            clusters: clusters.clone(),
            semantic_edges: semantic_edges.clone(),
            prior_edges,
            provider: GraphProviderMetadata {
                id: self.provider_id.clone(),
                semantic: self.provider_id != "hashing-bow",
            },
            summary: GraphAnalysisSummary {
                note_count: self.notes.len(),
                embedded_note_count: embedded_count,
                cluster_count: clusters.len(),
                outlier_count,
                no_signal_count,
                semantic_edge_candidate_count: semantic_edges.len(),
            },
            timeline: None,
        })
    }

    fn embedding_coverage_fallback(&self, mode: GraphMode) -> GraphAnalysisResult {
        let (positions, labels, outliers) = category_layout_for_notes(&self.notes);
        let clusters =
            cluster_descriptions(mode, &self.notes, &labels, &self.category_display_names);
        let nodes = self
            .notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                graph_analysis_node(
                    note,
                    positions[index].0,
                    positions[index].1,
                    labels[index],
                    outliers[index],
                    self.centroids[index].magnitude() <= f32::EPSILON,
                    None,
                    None,
                )
            })
            .collect();
        let semantic_edges =
            semantic_edges(&self.notes, &self.centroids, &self.embedded_note_indices);
        GraphAnalysisResult {
            mode,
            nodes,
            clusters: clusters.clone(),
            semantic_edges: semantic_edges.clone(),
            prior_edges: prior_edges(&self.notes),
            provider: GraphProviderMetadata {
                id: self.provider_id.clone(),
                semantic: false,
            },
            summary: GraphAnalysisSummary {
                note_count: self.notes.len(),
                embedded_note_count: self.embedded_note_indices.len(),
                cluster_count: clusters.len(),
                outlier_count: 0,
                no_signal_count: self.notes.len() - self.embedded_note_indices.len(),
                semantic_edge_candidate_count: semantic_edges.len(),
            },
            timeline: None,
        }
    }

    fn kanban_analysis(&self) -> GraphAnalysisResult {
        let nodes = self
            .notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                graph_analysis_node(
                    note,
                    0.0,
                    index as f32,
                    None,
                    false,
                    self.centroids[index].magnitude() <= f32::EPSILON,
                    None,
                    None,
                )
            })
            .collect();

        GraphAnalysisResult {
            mode: GraphMode::Kanban,
            nodes,
            clusters: Vec::new(),
            semantic_edges: Vec::new(),
            prior_edges: Vec::new(),
            provider: GraphProviderMetadata {
                id: self.provider_id.clone(),
                semantic: self.provider_id != "hashing-bow",
            },
            summary: GraphAnalysisSummary {
                note_count: self.notes.len(),
                embedded_note_count: self.embedded_note_indices.len(),
                cluster_count: 0,
                outlier_count: 0,
                no_signal_count: self.notes.len() - self.embedded_note_indices.len(),
                semantic_edge_candidate_count: 0,
            },
            timeline: None,
        }
    }

    /// Lay notes along a horizontal time axis. Needs only dates + categories (no embeddings),
    /// so it is dispatched before the embedding pipeline in `analyze`.
    fn timeline_analysis(&self, request: &GraphAnalysisRequest) -> GraphAnalysisResult {
        let note_count = self.notes.len();
        let resolved_dates: Vec<Option<GraphTimelineNodeDate>> = self
            .notes
            .iter()
            .map(|note| resolve_timeline_date(note, request))
            .collect();
        let resolved_ms: Vec<Option<i64>> = resolved_dates
            .iter()
            .map(|resolved| resolved.as_ref().map(|date| date.at_ms))
            .collect();
        let range_end_dates: Vec<Option<GraphTimelineNodeDate>> =
            match request.timeline_range_end_date {
                Some(end_field) => self
                    .notes
                    .iter()
                    .map(|note| resolve_timeline_range_end_date(note, end_field))
                    .collect(),
                None => vec![None; note_count],
            };
        let undated_count = resolved_ms.iter().filter(|ms| ms.is_none()).count();
        let dated: Vec<i64> = resolved_ms
            .iter()
            .enumerate()
            .filter_map(|(index, ms)| {
                let start_ms = (*ms)?;
                Some(
                    std::iter::once(start_ms)
                        .chain(range_end_dates[index].as_ref().map(|end| end.at_ms)),
                )
            })
            .flatten()
            .collect();

        let (labels, clusters) = self.timeline_cluster_labels(request);

        let mut xs = vec![0.0_f32; note_count];
        let mut ys = vec![0.0_f32; note_count];
        let mut range_end_xs = vec![None; note_count];

        // Reserve a left lane for undated notes only when some exist.
        let time_x_start: f32 = if undated_count > 0 { 0.10 } else { 0.0 };
        const TIME_X_END: f32 = 1.0;
        const LANE_X: f32 = 0.025;

        let timeline_meta = if dated.is_empty() {
            // Degenerate corpus: nothing has a resolvable date. Stack everyone in the lane.
            place_stack(&mut xs, &mut ys, LANE_X, (0..note_count).collect());
            GraphTimelineMeta {
                start_ms: 0,
                end_ms: 0,
                focus_start_x: 0.0,
                focus_end_x: 1.0,
                granularity: TimelineGranularity::Day,
                ticks: Vec::new(),
                undated_count,
                bucket_count: 1,
            }
        } else {
            let min_ms = *dated.iter().min().unwrap();
            let max_ms = *dated.iter().max().unwrap();
            let granularity = resolve_granularity(request.timeline_granularity, min_ms, max_ms);
            let first_dated_bucket = floor_to_bucket(min_ms, granularity);
            let max_bucket = floor_to_bucket(max_ms, granularity);
            let start_ms = step_bucket_back(first_dated_bucket, granularity, 2);
            let end_ms = step_bucket_forward(max_bucket, granularity, 3);
            let span = (end_ms - start_ms).max(1) as f32;
            let map_x = |ms: i64| -> f32 {
                (time_x_start + (ms - start_ms) as f32 / span * (TIME_X_END - time_x_start))
                    .clamp(0.0, 1.0)
            };

            for (index, end_date) in range_end_dates.iter().enumerate() {
                if resolved_ms[index].is_some() {
                    range_end_xs[index] = end_date.as_ref().map(|end| map_x(end.at_ms));
                }
            }

            if request.timeline_range_end_date.is_some() {
                place_timeline_collision_lanes(TimelineCollisionLaneInputs {
                    xs: &mut xs,
                    ys: &mut ys,
                    resolved_ms: &resolved_ms,
                    range_end_dates: &range_end_dates,
                    notes: &self.notes,
                    axis_start_ms: start_ms,
                    axis_end_ms: end_ms,
                    map_x,
                });
            } else {
                // Group dated notes by bucket so same-bucket notes share an x and stack vertically.
                let mut buckets: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
                for (index, ms) in resolved_ms.iter().enumerate() {
                    if let Some(ms) = ms {
                        buckets
                            .entry(floor_to_bucket(*ms, granularity))
                            .or_default()
                            .push(index);
                    }
                }
                for (bucket_start, mut members) in buckets {
                    members.sort_by(|a, b| {
                        resolved_ms[*a].cmp(&resolved_ms[*b]).then_with(|| {
                            self.notes[*a]
                                .id
                                .to_string()
                                .cmp(&self.notes[*b].id.to_string())
                        })
                    });
                    let bucket_center =
                        bucket_start + (next_bucket(bucket_start, granularity) - bucket_start) / 2;
                    place_stack(&mut xs, &mut ys, map_x(bucket_center), members);
                }
            }

            // Undated lane stacked at the far left.
            let undated: Vec<usize> = resolved_ms
                .iter()
                .enumerate()
                .filter_map(|(index, ms)| ms.is_none().then_some(index))
                .collect();
            if !undated.is_empty() {
                place_stack(&mut xs, &mut ys, LANE_X, undated);
            }

            // Axis ticks at bucket centers, thinned so labels never crowd.
            let mut boundaries = Vec::new();
            let mut cursor = start_ms;
            let last_axis_bucket = step_bucket_back(end_ms, granularity, 1);
            while cursor <= last_axis_bucket {
                boundaries.push(cursor);
                cursor = next_bucket(cursor, granularity);
            }
            let stride = (boundaries.len() / 24).max(1);
            let ticks = boundaries
                .iter()
                .enumerate()
                .filter(|(index, _)| index % stride == 0)
                .map(|(_, bucket_start)| {
                    let center =
                        bucket_start + (next_bucket(*bucket_start, granularity) - bucket_start) / 2;
                    GraphTimelineTick {
                        at_ms: *bucket_start,
                        label: format_tick(*bucket_start, granularity),
                        x: map_x(center),
                    }
                })
                .collect();

            // Initial camera frames the 1st-99th percentile, expanded by real bucket steps.
            let mut sorted = dated.clone();
            sorted.sort_unstable();
            let percentile = |q: f32| -> i64 {
                let idx = ((sorted.len() - 1) as f32 * q).round() as usize;
                sorted[idx]
            };
            let focus_start_bucket = step_bucket_back(
                floor_to_bucket(percentile(0.01), granularity),
                granularity,
                2,
            );
            let focus_end_boundary = step_bucket_forward(
                floor_to_bucket(percentile(0.99), granularity),
                granularity,
                3,
            );
            let mut focus_start_x = map_x(focus_start_bucket);
            let mut focus_end_x = map_x(focus_end_boundary);
            if focus_end_x - focus_start_x < 0.05 {
                focus_start_x = 0.0;
                focus_end_x = 1.0;
            }
            if undated_count > 0 {
                focus_start_x = 0.0; // keep the undated lane in view
            }

            GraphTimelineMeta {
                start_ms,
                end_ms,
                focus_start_x,
                focus_end_x,
                granularity,
                ticks,
                undated_count,
                bucket_count: boundaries.len() as u32,
            }
        };

        let nodes = self
            .notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                let timeline_range = match (
                    &resolved_dates[index],
                    &range_end_dates[index],
                    range_end_xs[index],
                ) {
                    (Some(start), Some(end_date), Some(end_x)) => Some(GraphTimelineNodeRange {
                        end_date: end_date.clone(),
                        end_x,
                        end_before_start: end_date.at_ms < start.at_ms,
                    }),
                    _ => None,
                };
                graph_analysis_node(
                    note,
                    xs[index],
                    ys[index],
                    labels[index],
                    false,
                    // Reuse the "no signal" flag/styling to mark notes parked in the undated lane.
                    resolved_ms[index].is_none(),
                    resolved_dates[index].clone(),
                    timeline_range,
                )
            })
            .collect();

        GraphAnalysisResult {
            mode: GraphMode::Timeline,
            nodes,
            clusters: clusters.clone(),
            semantic_edges: Vec::new(),
            prior_edges: prior_edges(&self.notes),
            provider: GraphProviderMetadata {
                id: self.provider_id.clone(),
                semantic: self.provider_id != "hashing-bow",
            },
            summary: GraphAnalysisSummary {
                note_count,
                embedded_note_count: self.embedded_note_indices.len(),
                cluster_count: clusters.len(),
                outlier_count: 0,
                no_signal_count: undated_count,
                semantic_edge_candidate_count: 0,
            },
            timeline: Some(timeline_meta),
        }
    }

    /// Cluster- id + descriptions for timeline coloring. `Category` indexes the first category;
    /// `Cluster` reuses k-means over the existing embeddings (falling back to categories when no
    /// embeddings exist).
    fn timeline_cluster_labels(
        &self,
        request: &GraphAnalysisRequest,
    ) -> (Vec<Option<usize>>, Vec<GraphCluster>) {
        let category_labels = || {
            let mut category_ids: BTreeMap<String, usize> = BTreeMap::new();
            let labels: Vec<Option<usize>> = self
                .notes
                .iter()
                .map(|note| {
                    let key = note
                        .categories
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "uncategorized".into());
                    let next = category_ids.len();
                    Some(*category_ids.entry(key).or_insert(next))
                })
                .collect();
            labels
        };

        let labels = match request.timeline_color_by {
            TimelineColorBy::Category => category_labels(),
            TimelineColorBy::Cluster if !self.embedded_note_indices.is_empty() => {
                let embedded_vectors: Vec<Embedding> = self
                    .embedded_note_indices
                    .iter()
                    .map(|index| self.centroids[*index].clone())
                    .collect();
                let k = request
                    .kmeans_k
                    .clamp(2, 12)
                    .min(embedded_vectors.len().max(1));
                let embedded_labels = deterministic_kmeans(&embedded_vectors, k);
                let mut labels = vec![None; self.notes.len()];
                for (embedded_index, note_index) in self.embedded_note_indices.iter().enumerate() {
                    labels[*note_index] = Some(embedded_labels[embedded_index]);
                }
                labels
            }
            TimelineColorBy::Cluster => category_labels(),
        };
        let clusters = cluster_descriptions(
            GraphMode::Timeline,
            &self.notes,
            &labels,
            &self.category_display_names,
        );
        (labels, clusters)
    }
}

fn graph_analysis_node(
    note: &Note,
    x: f32,
    y: f32,
    cluster_id: Option<usize>,
    outlier: bool,
    no_semantic_signal: bool,
    timeline_date: Option<GraphTimelineNodeDate>,
    timeline_range: Option<GraphTimelineNodeRange>,
) -> GraphAnalysisNode {
    GraphAnalysisNode {
        id: note.id.to_string(),
        object_type: note.object_type,
        title: if note.title.trim().is_empty() {
            "(untitled)".into()
        } else {
            note.title.clone()
        },
        summary: note.summary.clone(),
        categories: note.categories.clone(),
        status: note.metadata.status,
        classification: note.metadata.classification.kind,
        priority: note.metadata.classification.priority,
        starred: note.metadata.classification.starred,
        created: note.metadata.dates.created,
        updated: note.metadata.dates.updated,
        started: note
            .metadata
            .dates
            .started
            .as_ref()
            .and_then(|date| date.date),
        completed: note
            .metadata
            .dates
            .completed
            .as_ref()
            .and_then(|date| date.date),
        x,
        y,
        cluster_id,
        outlier,
        no_semantic_signal,
        timeline_date,
        timeline_range,
    }
}

pub fn current_corpus_fingerprint(book: &Book) -> CoreResult<String> {
    let mut notes: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|note| {
            note.object_type != ObjectType::Commentary
                && note.metadata.is_visible_in_default_views()
        })
        .collect();
    notes.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
    corpus_fingerprint(book, &notes, &embedding_source_cache_key(book))
}

pub fn corpus_fingerprint(book: &Book, notes: &[Note], provider_id: &str) -> CoreResult<String> {
    let categories: Vec<Category> = book.store.categories()?;
    let serialized = serde_json::to_vec(&(
        &book.metadata.book_id,
        &book.config.embedding,
        provider_id,
        categories,
        notes
            .iter()
            .map(|note| {
                (
                    note.id.to_string(),
                    &note.title,
                    &note.summary,
                    &note.body,
                    &note.categories,
                    &note.prior,
                    &note.metadata,
                )
            })
            .collect::<Vec<_>>(),
    ))?;
    Ok(format!("{:x}", Sha256::digest(serialized)))
}

fn embedding_source_cache_key(_book: &Book) -> String {
    let mut hasher = Sha256::new();
    hasher.update(_book.config.embedding.model_id.as_bytes());
    let directory = crate::storage::layout::embeddings_dir(&_book.root);
    if let Ok(entries) = std::fs::read_dir(directory) {
        let mut paths = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            if let Ok(bytes) = std::fs::read(path) {
                hasher.update((bytes.len() as u64).to_le_bytes());
                hasher.update(bytes);
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

fn semantic_edges(
    notes: &[Note],
    centroids: &[Embedding],
    embedded_note_indices: &[usize],
) -> Vec<GraphSemanticEdge> {
    let mut strongest_by_pair: HashMap<(usize, usize), f32> = HashMap::new();
    for source in embedded_note_indices {
        let mut ranked: Vec<(usize, f32)> = embedded_note_indices
            .iter()
            .copied()
            .filter(|target| target != source)
            .map(|target| {
                (
                    target,
                    centroids[*source].cosine_similarity(&centroids[target]),
                )
            })
            .filter(|(_, similarity)| *similarity > 0.0)
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        ranked.truncate(MAX_SEMANTIC_NEIGHBORS);
        for (target, similarity) in ranked {
            let pair = if *source < target {
                (*source, target)
            } else {
                (target, *source)
            };
            strongest_by_pair
                .entry(pair)
                .and_modify(|existing| *existing = existing.max(similarity))
                .or_insert(similarity);
        }
    }
    let mut edges: Vec<GraphSemanticEdge> = strongest_by_pair
        .into_iter()
        .map(|((source, target), similarity)| GraphSemanticEdge {
            source: notes[source].id.to_string(),
            target: notes[target].id.to_string(),
            similarity,
        })
        .collect();
    edges.sort_by(|left, right| {
        right
            .similarity
            .total_cmp(&left.similarity)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.target.cmp(&right.target))
    });
    edges
}

fn prior_edges(notes: &[Note]) -> Vec<GraphPriorEdge> {
    let present: HashSet<String> = notes.iter().map(|note| note.id.to_string()).collect();
    notes
        .iter()
        .filter_map(|note| {
            let target = note.prior.as_ref()?.target.note_id()?;
            let target = target.to_string();
            present.contains(&target).then(|| GraphPriorEdge {
                source: note.id.to_string(),
                target,
            })
        })
        .collect()
}

fn cluster_descriptions(
    mode: GraphMode,
    notes: &[Note],
    labels: &[Option<usize>],
    category_display_names: &HashMap<String, String>,
) -> Vec<GraphCluster> {
    let mut members: BTreeMap<usize, Vec<&Note>> = BTreeMap::new();
    for (note, label) in notes.iter().zip(labels) {
        if let Some(label) = label {
            members.entry(*label).or_default().push(note);
        }
    }
    members
        .into_iter()
        .map(|(id, notes)| {
            let mut category_counts: HashMap<&str, usize> = HashMap::new();
            for note in &notes {
                for category in &note.categories {
                    *category_counts.entry(category).or_default() += 1;
                }
            }
            let mut ranked: Vec<(&str, usize)> = category_counts.into_iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            let dominant: Vec<String> = ranked
                .into_iter()
                .take(2)
                .map(|(category, _)| {
                    category_display_names
                        .get(category)
                        .cloned()
                        .unwrap_or_else(|| category.to_string())
                })
                .collect();
            let fallback = match mode {
                GraphMode::Categories | GraphMode::Timeline | GraphMode::Kanban => "Category",
                GraphMode::Pillars => "Pillar",
                GraphMode::Communities => "Community",
                GraphMode::Density => "Dense region",
            };
            GraphCluster {
                id,
                label: if dominant.is_empty() {
                    format!("{fallback} {}", id + 1)
                } else {
                    dominant.join(" · ")
                },
                node_count: notes.len(),
            }
        })
        .collect()
}

fn place_no_signal_notes(
    positions: &mut [(f32, f32)],
    embedded_indices: &[usize],
    note_count: usize,
) {
    let embedded: HashSet<usize> = embedded_indices.iter().copied().collect();
    let no_signal: Vec<usize> = (0..note_count)
        .filter(|index| !embedded.contains(index))
        .collect();
    for (offset, note_index) in no_signal.iter().enumerate() {
        positions[*note_index] = (0.06, 0.14 + offset as f32 * 0.055);
    }
}

const MS_PER_HOUR: i64 = 3_600_000;
const MS_PER_DAY: i64 = 86_400_000;

/// Resolve a note's timestamp for the chosen date field. `created`/`updated` are always present;
/// `scheduled`/`completed` resolve from the absolute `FlexDate.date` only (relative dates are a
/// follow-up — they need anchor-note lookup — and count as undated for now).
fn resolve_note_ms(note: &Note, field: TimelineDateField) -> Option<i64> {
    let dates = &note.metadata.dates;
    match field {
        TimelineDateField::Created => Some(dates.created.timestamp_millis()),
        TimelineDateField::Updated => Some(dates.updated.timestamp_millis()),
        TimelineDateField::Scheduled => flex_date_ms(dates.scheduled.as_ref()),
        TimelineDateField::Started => flex_date_ms(dates.started.as_ref()),
        TimelineDateField::Due => flex_date_ms(dates.due.as_ref()),
        TimelineDateField::Completed => flex_date_ms(dates.completed.as_ref()),
    }
}

fn resolve_timeline_date(
    note: &Note,
    request: &GraphAnalysisRequest,
) -> Option<GraphTimelineNodeDate> {
    if let Some(at_ms) = resolve_note_ms(note, request.timeline_primary_date) {
        return Some(GraphTimelineNodeDate {
            at_ms,
            source_field: request.timeline_primary_date,
            used_fallback: false,
            date_only: is_date_only_field(request.timeline_primary_date),
        });
    }
    let source_field = request.timeline_fallback_date?;
    let at_ms = resolve_note_ms(note, source_field)?;
    Some(GraphTimelineNodeDate {
        at_ms,
        source_field,
        used_fallback: true,
        date_only: is_date_only_field(source_field),
    })
}

fn resolve_timeline_range_end_date(
    note: &Note,
    source_field: TimelineDateField,
) -> Option<GraphTimelineNodeDate> {
    let at_ms = resolve_note_ms(note, source_field)?;
    Some(GraphTimelineNodeDate {
        at_ms,
        source_field,
        used_fallback: false,
        date_only: is_date_only_field(source_field),
    })
}

fn is_date_only_field(field: TimelineDateField) -> bool {
    matches!(
        field,
        TimelineDateField::Scheduled
            | TimelineDateField::Started
            | TimelineDateField::Due
            | TimelineDateField::Completed
    )
}

fn flex_date_ms(flex: Option<&crate::model::FlexDate>) -> Option<i64> {
    use chrono::TimeZone;
    let date = flex?.date?;
    let naive = date.and_hms_opt(0, 0, 0)?;
    Some(chrono::Utc.from_utc_datetime(&naive).timestamp_millis())
}

/// Stack a set of notes in a single vertical column at `x`, compressing the row step so even a
/// tall column stays within the canvas.
fn place_stack(xs: &mut [f32], ys: &mut [f32], x: f32, members: Vec<usize>) {
    const Y_TOP: f32 = 0.08;
    const Y_BOTTOM: f32 = 0.95;
    const Y_STEP_MAX: f32 = 0.045;
    let step = if members.len() <= 1 {
        0.0
    } else {
        ((Y_BOTTOM - Y_TOP) / (members.len() as f32 - 1.0)).min(Y_STEP_MAX)
    };
    for (offset, index) in members.into_iter().enumerate() {
        xs[index] = x;
        ys[index] = Y_TOP + offset as f32 * step;
    }
}

struct TimelineCollisionLaneInputs<'a, F: Fn(i64) -> f32> {
    xs: &'a mut [f32],
    ys: &'a mut [f32],
    resolved_ms: &'a [Option<i64>],
    range_end_dates: &'a [Option<GraphTimelineNodeDate>],
    notes: &'a [Note],
    axis_start_ms: i64,
    axis_end_ms: i64,
    map_x: F,
}

fn place_timeline_collision_lanes<F: Fn(i64) -> f32>(inputs: TimelineCollisionLaneInputs<'_, F>) {
    let TimelineCollisionLaneInputs {
        xs,
        ys,
        resolved_ms,
        range_end_dates,
        notes,
        axis_start_ms,
        axis_end_ms,
        map_x,
    } = inputs;
    const Y_TOP: f32 = 0.08;
    const Y_BOTTOM: f32 = 0.95;
    const MINIMUM_LANE_GAP_DIVISOR: i64 = 250;

    let axis_span = (axis_end_ms - axis_start_ms).max(1);
    let minimum_lane_gap_ms = (axis_span / MINIMUM_LANE_GAP_DIVISOR).max(MS_PER_HOUR);
    let mut intervals: Vec<(usize, i64, i64)> = resolved_ms
        .iter()
        .enumerate()
        .filter_map(|(index, start)| {
            let start = (*start)?;
            let end = range_end_dates[index]
                .as_ref()
                .map(|range| range.at_ms)
                .unwrap_or(start);
            Some((index, start.min(end), start.max(end)))
        })
        .collect();
    intervals.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| {
                notes[left.0]
                    .id
                    .to_string()
                    .cmp(&notes[right.0].id.to_string())
            })
    });

    let mut lane_available_after: Vec<i64> = Vec::new();
    let mut lane_by_index = vec![0_usize; resolved_ms.len()];
    for (index, visual_start, visual_end) in intervals {
        let lane = lane_available_after
            .iter()
            .position(|available_after| *available_after <= visual_start)
            .unwrap_or_else(|| {
                lane_available_after.push(i64::MIN);
                lane_available_after.len() - 1
            });
        lane_available_after[lane] = visual_end + minimum_lane_gap_ms;
        lane_by_index[index] = lane;
        xs[index] = map_x(resolved_ms[index].unwrap());
    }

    let lane_count = lane_available_after.len().max(1);
    let lane_step = if lane_count <= 1 {
        0.0
    } else {
        (Y_BOTTOM - Y_TOP) / (lane_count as f32 - 1.0)
    };
    for (index, start) in resolved_ms.iter().enumerate() {
        if start.is_some() {
            ys[index] = Y_TOP + lane_by_index[index] as f32 * lane_step;
        }
    }
}

fn resolve_granularity(
    requested: TimelineGranularity,
    min_ms: i64,
    max_ms: i64,
) -> TimelineGranularity {
    match requested {
        TimelineGranularity::Auto => {
            let span = max_ms - min_ms;
            if span < 2 * MS_PER_DAY {
                TimelineGranularity::Hour
            } else if span < 90 * MS_PER_DAY {
                TimelineGranularity::Day
            } else if span < 5 * 365 * MS_PER_DAY {
                TimelineGranularity::Month
            } else {
                TimelineGranularity::Year
            }
        }
        concrete => concrete,
    }
}

/// Floor a timestamp to the start of its bucket (UTC).
fn floor_to_bucket(ms: i64, granularity: TimelineGranularity) -> i64 {
    use chrono::{Datelike, TimeZone};
    match granularity {
        TimelineGranularity::Hour => ms - ms.rem_euclid(MS_PER_HOUR),
        TimelineGranularity::Day => ms - ms.rem_euclid(MS_PER_DAY),
        TimelineGranularity::Month => {
            let dt = chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default();
            chrono::Utc
                .with_ymd_and_hms(dt.year(), dt.month(), 1, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        }
        TimelineGranularity::Year => {
            let dt = chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default();
            chrono::Utc
                .with_ymd_and_hms(dt.year(), 1, 1, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        }
        TimelineGranularity::Auto => ms,
    }
}

/// The start of the bucket immediately after `bucket_start`.
fn next_bucket(bucket_start: i64, granularity: TimelineGranularity) -> i64 {
    use chrono::{Datelike, TimeZone};
    match granularity {
        TimelineGranularity::Hour => bucket_start + MS_PER_HOUR,
        TimelineGranularity::Day => bucket_start + MS_PER_DAY,
        TimelineGranularity::Month => {
            let dt = chrono::DateTime::from_timestamp_millis(bucket_start).unwrap_or_default();
            let (year, month) = if dt.month() == 12 {
                (dt.year() + 1, 1)
            } else {
                (dt.year(), dt.month() + 1)
            };
            chrono::Utc
                .with_ymd_and_hms(year, month, 1, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        }
        TimelineGranularity::Year => {
            let dt = chrono::DateTime::from_timestamp_millis(bucket_start).unwrap_or_default();
            chrono::Utc
                .with_ymd_and_hms(dt.year() + 1, 1, 1, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        }
        TimelineGranularity::Auto => bucket_start + MS_PER_DAY,
    }
}

fn previous_bucket(bucket_start: i64, granularity: TimelineGranularity) -> i64 {
    use chrono::{Datelike, TimeZone};
    match granularity {
        TimelineGranularity::Hour => bucket_start - MS_PER_HOUR,
        TimelineGranularity::Day => bucket_start - MS_PER_DAY,
        TimelineGranularity::Month => {
            let dt = chrono::DateTime::from_timestamp_millis(bucket_start).unwrap_or_default();
            let (year, month) = if dt.month() == 1 {
                (dt.year() - 1, 12)
            } else {
                (dt.year(), dt.month() - 1)
            };
            chrono::Utc
                .with_ymd_and_hms(year, month, 1, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        }
        TimelineGranularity::Year => {
            let dt = chrono::DateTime::from_timestamp_millis(bucket_start).unwrap_or_default();
            chrono::Utc
                .with_ymd_and_hms(dt.year() - 1, 1, 1, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        }
        TimelineGranularity::Auto => bucket_start - MS_PER_DAY,
    }
}

fn step_bucket_forward(
    mut bucket_start: i64,
    granularity: TimelineGranularity,
    steps: usize,
) -> i64 {
    for _ in 0..steps {
        bucket_start = next_bucket(bucket_start, granularity);
    }
    bucket_start
}

fn step_bucket_back(mut bucket_start: i64, granularity: TimelineGranularity, steps: usize) -> i64 {
    for _ in 0..steps {
        bucket_start = previous_bucket(bucket_start, granularity);
    }
    bucket_start
}

fn format_tick(bucket_start: i64, granularity: TimelineGranularity) -> String {
    let Some(dt) = chrono::DateTime::from_timestamp_millis(bucket_start) else {
        return String::new();
    };
    match granularity {
        TimelineGranularity::Hour => dt.format("%-d %b %H:%M").to_string(),
        TimelineGranularity::Day => dt.format("%-d %b").to_string(),
        TimelineGranularity::Month => dt.format("%b %Y").to_string(),
        TimelineGranularity::Year => dt.format("%Y").to_string(),
        TimelineGranularity::Auto => dt.format("%-d %b").to_string(),
    }
}

fn empty_result(mode: GraphMode, provider_id: &str) -> GraphAnalysisResult {
    GraphAnalysisResult {
        mode,
        nodes: Vec::new(),
        clusters: Vec::new(),
        semantic_edges: Vec::new(),
        prior_edges: Vec::new(),
        provider: GraphProviderMetadata {
            id: provider_id.into(),
            semantic: provider_id != "hashing-bow",
        },
        summary: GraphAnalysisSummary {
            note_count: 0,
            embedded_note_count: 0,
            cluster_count: 0,
            outlier_count: 0,
            no_signal_count: 0,
            semantic_edge_candidate_count: 0,
        },
        timeline: None,
    }
}

trait PriorTargetNoteId {
    fn note_id(&self) -> Option<&crate::id::NoteId>;
}

impl PriorTargetNoteId for crate::model::PriorRef {
    fn note_id(&self) -> Option<&crate::id::NoteId> {
        match self {
            crate::model::PriorRef::Note(note_id) => Some(note_id),
            crate::model::PriorRef::Category(_) => None,
        }
    }
}
