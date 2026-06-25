mod algorithms;
#[cfg(test)]
mod tests;
mod types;

use std::collections::{BTreeMap, HashMap, HashSet};

use sha2::{Digest, Sha256};

use crate::embeddings::{configured_model_fingerprint, load_embedding_corpus, Embedding};
use crate::error::CoreResult;
use crate::model::{Category, Note};
use crate::storage::{Book, NoteStore};

use algorithms::{
    circle_layout, cosine_distance_matrix, deterministic_kmeans, hdbscan_labels, louvain_labels,
    normalize_layout, umap_layout,
};
pub use types::{
    GraphAnalysisNode, GraphAnalysisRequest, GraphAnalysisResult, GraphAnalysisSummary,
    GraphCluster, GraphMode, GraphPriorEdge, GraphProviderMetadata, GraphSemanticEdge,
};

const MAX_SEMANTIC_NEIGHBORS: usize = 30;
type CategoryLayoutAnalysis = (Vec<(f32, f32)>, Vec<Option<usize>>, Vec<bool>);

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
            .filter(|note| note.metadata.is_visible_in_default_views())
            .collect();
        notes.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
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
        let embedded_vectors: Vec<Embedding> = self
            .embedded_note_indices
            .iter()
            .map(|index| self.centroids[*index].clone())
            .collect();
        let embedded_count = embedded_vectors.len();
        let default_neighbors = match request.mode {
            GraphMode::Pillars => 50,
            GraphMode::Communities => 8,
            GraphMode::Density => 15,
            GraphMode::Categories => request.umap_neighbors,
        };
        let requested_neighbors = if request.umap_neighbors == 0 {
            default_neighbors
        } else {
            request.umap_neighbors
        }
        .clamp(2, 100);

        let (positions, embedded_cluster_labels, embedded_outliers) = match request.mode {
            GraphMode::Categories => self.category_analysis(),
            GraphMode::Pillars => {
                let positions = umap_layout(&embedded_vectors, requested_neighbors)?;
                let labels = deterministic_kmeans(
                    &embedded_vectors,
                    request.kmeans_k.clamp(2, 12).min(embedded_count.max(1)),
                );
                (
                    positions,
                    labels.into_iter().map(Some).collect(),
                    vec![false; embedded_count],
                )
            }
            GraphMode::Communities => {
                let positions = umap_layout(&embedded_vectors, requested_neighbors)?;
                let distances = cosine_distance_matrix(&embedded_vectors);
                let labels =
                    louvain_labels(&distances, requested_neighbors, request.louvain_resolution);
                (
                    positions,
                    labels.into_iter().map(Some).collect(),
                    vec![false; embedded_count],
                )
            }
            GraphMode::Density => {
                let positions = umap_layout(&embedded_vectors, requested_neighbors)?;
                let labels = hdbscan_labels(
                    &embedded_vectors,
                    request.hdbscan_min_cluster_size.clamp(2, 50),
                )?;
                let outliers = labels.iter().map(Option::is_none).collect();
                (positions, labels, outliers)
            }
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
            .map(|(index, note)| GraphAnalysisNode {
                id: note.id.to_string(),
                title: if note.title.trim().is_empty() {
                    "(untitled)".into()
                } else {
                    note.title.clone()
                },
                categories: note.categories.clone(),
                x: full_positions[index].0,
                y: full_positions[index].1,
                cluster_id: full_labels[index],
                outlier: full_outliers[index],
                no_semantic_signal: self.centroids[index].magnitude() <= f32::EPSILON,
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
        })
    }

    fn category_analysis(&self) -> CategoryLayoutAnalysis {
        let mut category_ids = BTreeMap::new();
        for note_index in &self.embedded_note_indices {
            let key = self.notes[*note_index]
                .categories
                .first()
                .cloned()
                .unwrap_or_else(|| "uncategorized".into());
            let next = category_ids.len();
            category_ids.entry(key).or_insert(next);
        }
        let mut labels = Vec::with_capacity(self.embedded_note_indices.len());
        let mut positions = Vec::with_capacity(self.embedded_note_indices.len());
        let group_count = category_ids.len().max(1);
        let mut members_by_group: HashMap<usize, Vec<usize>> = HashMap::new();
        for (embedded_index, note_index) in self.embedded_note_indices.iter().enumerate() {
            let key = self.notes[*note_index]
                .categories
                .first()
                .cloned()
                .unwrap_or_else(|| "uncategorized".into());
            let group = category_ids[&key];
            labels.push(Some(group));
            members_by_group
                .entry(group)
                .or_default()
                .push(embedded_index);
            positions.push((0.0, 0.0));
        }
        for (group, members) in members_by_group {
            let group_angle = group as f32 / group_count as f32 * std::f32::consts::TAU;
            let center = (
                0.5 + group_angle.cos() * 0.30,
                0.5 + group_angle.sin() * 0.30,
            );
            let local = circle_layout(members.len());
            for (local_index, embedded_index) in members.iter().enumerate() {
                positions[*embedded_index] = (
                    center.0 + (local[local_index].0 - 0.5) * 0.22,
                    center.1 + (local[local_index].1 - 0.5) * 0.22,
                );
            }
        }
        (
            normalize_layout(&positions),
            labels,
            vec![false; self.embedded_note_indices.len()],
        )
    }
}

pub fn current_corpus_fingerprint(book: &Book) -> CoreResult<String> {
    let mut notes: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|note| note.metadata.is_visible_in_default_views())
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
                GraphMode::Categories => "Category",
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
