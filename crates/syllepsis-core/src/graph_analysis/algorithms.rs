use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use graphops::{louvain_weighted_seeded, Graph, WeightedGraph};
use hdbscan::{DistanceMetric, Hdbscan, HdbscanHyperParams, NnAlgorithm};
use ndarray::Array2;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use umap_rs::{GraphParams, OptimizationParams, Umap, UmapConfig};

use crate::embeddings::Embedding;
use crate::error::{CoreError, CoreResult};

pub(super) fn cosine_distance_matrix(vectors: &[Embedding]) -> Vec<Vec<f32>> {
    vectors
        .iter()
        .map(|a| {
            vectors
                .iter()
                .map(|b| (1.0 - a.cosine_similarity(b)).clamp(0.0, 2.0))
                .collect()
        })
        .collect()
}

pub(super) fn exact_knn(
    distances: &[Vec<f32>],
    requested_neighbors: usize,
) -> (Vec<Vec<usize>>, Vec<Vec<f32>>) {
    let count = distances.len();
    if count <= 1 {
        return (vec![Vec::new(); count], vec![Vec::new(); count]);
    }
    let neighbor_count = requested_neighbors.clamp(2, count - 1);
    let mut indices = Vec::with_capacity(count);
    let mut neighbor_distances = Vec::with_capacity(count);
    for (row_index, row) in distances.iter().enumerate() {
        let mut ranked: Vec<(usize, f32)> = row
            .iter()
            .copied()
            .enumerate()
            .filter(|(index, _)| *index != row_index)
            .collect();
        ranked.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        ranked.truncate(neighbor_count);
        indices.push(ranked.iter().map(|(index, _)| *index).collect());
        neighbor_distances.push(ranked.iter().map(|(_, distance)| *distance).collect());
    }
    (indices, neighbor_distances)
}

pub(super) fn umap_layout(
    vectors: &[Embedding],
    requested_neighbors: usize,
) -> CoreResult<Vec<(f32, f32)>> {
    if vectors.len() < 3 {
        return Ok(circle_layout(vectors.len()));
    }
    let dimensions = vectors[0].len();
    if dimensions == 0 || vectors.iter().any(|vector| vector.len() != dimensions) {
        return Err(CoreError::GraphAnalysis(
            "embedding dimensions are empty or inconsistent".into(),
        ));
    }

    let distances = cosine_distance_matrix(vectors);
    let (knn_indices, knn_distances) = exact_knn(&distances, requested_neighbors);
    let neighbor_count = knn_indices[0].len();
    let flat_data: Vec<f32> = vectors
        .iter()
        .flat_map(|vector| vector.0.iter().copied())
        .collect();
    let flat_indices: Vec<u32> = knn_indices
        .iter()
        .flat_map(|row| row.iter().map(|index| *index as u32))
        .collect();
    let flat_distances: Vec<f32> = knn_distances.into_iter().flatten().collect();
    let data = Array2::from_shape_vec((vectors.len(), dimensions), flat_data)
        .map_err(|error| CoreError::GraphAnalysis(error.to_string()))?;
    let indices = Array2::from_shape_vec((vectors.len(), neighbor_count), flat_indices)
        .map_err(|error| CoreError::GraphAnalysis(error.to_string()))?;
    let distances = Array2::from_shape_vec((vectors.len(), neighbor_count), flat_distances)
        .map_err(|error| CoreError::GraphAnalysis(error.to_string()))?;
    let init = deterministic_initialization(vectors.len());
    let config = UmapConfig {
        n_components: 2,
        graph: GraphParams {
            n_neighbors: neighbor_count,
            ..Default::default()
        },
        optimization: OptimizationParams {
            n_epochs: Some(250),
            ..Default::default()
        },
        ..Default::default()
    };

    let fitted = catch_unwind(AssertUnwindSafe(|| {
        Umap::new(config).fit(data.view(), indices.view(), distances.view(), init.view())
    }))
    .map_err(|_| CoreError::GraphAnalysis("UMAP rejected the graph inputs".into()))?;
    let positions: Vec<(f32, f32)> = fitted
        .embedding()
        .rows()
        .into_iter()
        .map(|row| (row[0], row[1]))
        .collect();
    if positions
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(CoreError::GraphAnalysis(
            "UMAP produced non-finite coordinates".into(),
        ));
    }
    Ok(normalize_layout(&positions))
}

fn deterministic_initialization(count: usize) -> Array2<f32> {
    let mut values = Vec::with_capacity(count * 2);
    for index in 0..count {
        let angle = index as f32 / count.max(1) as f32 * std::f32::consts::TAU;
        let radius = 0.9 + (index % 7) as f32 * 0.01;
        values.push(angle.cos() * radius);
        values.push(angle.sin() * radius);
    }
    Array2::from_shape_vec((count, 2), values).expect("deterministic UMAP init has valid shape")
}

pub(super) fn normalize_layout(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if points.is_empty() {
        return Vec::new();
    }
    let min_x = points
        .iter()
        .map(|point| point.0)
        .fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|point| point.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let span = (max_x - min_x).max(max_y - min_y).max(f32::EPSILON);
    points
        .iter()
        .map(|(x, y)| {
            (
                0.1 + ((x - min_x) / span) * 0.8,
                0.1 + ((y - min_y) / span) * 0.8,
            )
        })
        .collect()
}

pub(super) fn circle_layout(count: usize) -> Vec<(f32, f32)> {
    (0..count)
        .map(|index| {
            let angle = index as f32 / count.max(1) as f32 * std::f32::consts::TAU;
            (0.5 + angle.cos() * 0.32, 0.5 + angle.sin() * 0.32)
        })
        .collect()
}

pub(super) fn deterministic_kmeans(vectors: &[Embedding], requested_k: usize) -> Vec<usize> {
    if vectors.is_empty() {
        return Vec::new();
    }
    let k = requested_k.clamp(1, vectors.len());
    let mut rng = StdRng::seed_from_u64(0x53594c4c45505349);
    let mut centroids = vec![vectors[rng.random_range(0..vectors.len())].0.clone()];
    while centroids.len() < k {
        let squared_distances: Vec<f32> = vectors
            .iter()
            .map(|vector| {
                centroids
                    .iter()
                    .map(|centroid| squared_euclidean(&vector.0, centroid))
                    .fold(f32::INFINITY, f32::min)
            })
            .collect();
        let total: f32 = squared_distances.iter().sum();
        let next = if total <= f32::EPSILON {
            centroids.len() % vectors.len()
        } else {
            let mut target = rng.random::<f32>() * total;
            squared_distances
                .iter()
                .position(|distance| {
                    target -= *distance;
                    target <= 0.0
                })
                .unwrap_or(vectors.len() - 1)
        };
        centroids.push(vectors[next].0.clone());
    }

    let mut labels = vec![0; vectors.len()];
    for _ in 0..100 {
        let next_labels: Vec<usize> = vectors
            .iter()
            .map(|vector| {
                centroids
                    .iter()
                    .enumerate()
                    .min_by(|a, b| {
                        squared_euclidean(&vector.0, a.1)
                            .total_cmp(&squared_euclidean(&vector.0, b.1))
                    })
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            })
            .collect();
        if next_labels == labels {
            break;
        }
        labels = next_labels;
        for (cluster, centroid) in centroids.iter_mut().enumerate().take(k) {
            let members: Vec<&Embedding> = vectors
                .iter()
                .zip(&labels)
                .filter_map(|(vector, label)| (*label == cluster).then_some(vector))
                .collect();
            if let Some(next_centroid) = Embedding::average(members) {
                *centroid = next_centroid.0;
            }
        }
    }
    renumber_labels(labels)
}

fn squared_euclidean(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(left, right)| (left - right).powi(2))
        .sum()
}

struct SimilarityGraph {
    adjacency: Vec<Vec<(usize, f64)>>,
}

impl Graph for SimilarityGraph {
    fn node_count(&self) -> usize {
        self.adjacency.len()
    }

    fn neighbors(&self, node: usize) -> Vec<usize> {
        self.adjacency[node]
            .iter()
            .map(|(neighbor, _)| *neighbor)
            .collect()
    }
}

impl WeightedGraph for SimilarityGraph {
    fn edge_weight(&self, source: usize, target: usize) -> f64 {
        self.adjacency[source]
            .iter()
            .find_map(|(neighbor, weight)| (*neighbor == target).then_some(*weight))
            .unwrap_or(0.0)
    }
}

pub(super) fn louvain_labels(
    distances: &[Vec<f32>],
    requested_neighbors: usize,
    resolution: f64,
) -> Vec<usize> {
    let (neighbors, _) = exact_knn(distances, requested_neighbors);
    let mut weights: Vec<HashMap<usize, f64>> = vec![HashMap::new(); distances.len()];
    for (source, row) in neighbors.iter().enumerate() {
        for target in row {
            let weight = (1.0 - distances[source][*target]).max(0.0) as f64;
            if weight <= 0.0 {
                continue;
            }
            weights[source]
                .entry(*target)
                .and_modify(|existing| *existing = existing.max(weight))
                .or_insert(weight);
            weights[*target]
                .entry(source)
                .and_modify(|existing| *existing = existing.max(weight))
                .or_insert(weight);
        }
    }
    let graph = SimilarityGraph {
        adjacency: weights
            .into_iter()
            .map(|row| {
                let mut edges: Vec<(usize, f64)> = row.into_iter().collect();
                edges.sort_by_key(|(neighbor, _)| *neighbor);
                edges
            })
            .collect(),
    };
    renumber_labels(louvain_weighted_seeded(
        &graph,
        resolution.clamp(0.25, 2.0),
        0x53594c4c,
    ))
}

pub(super) fn hdbscan_labels(
    vectors: &[Embedding],
    requested_min_cluster_size: usize,
) -> CoreResult<Vec<Option<usize>>> {
    if vectors.len() < 2 {
        return Ok(vec![None; vectors.len()]);
    }
    let min_cluster_size = requested_min_cluster_size.clamp(2, vectors.len());
    let min_samples = (min_cluster_size / 2).max(2).min(vectors.len());
    let data: Vec<Vec<f32>> = vectors.iter().map(|vector| vector.0.clone()).collect();
    let parameters = HdbscanHyperParams::builder()
        .min_cluster_size(min_cluster_size)
        .min_samples(min_samples)
        .dist_metric(DistanceMetric::Euclidean)
        .nn_algorithm(NnAlgorithm::BruteForce)
        .build();
    let raw = Hdbscan::new(&data, parameters)
        .cluster()
        .map_err(|error| CoreError::GraphAnalysis(error.to_string()))?;
    let mut remap = HashMap::new();
    let mut next = 0usize;
    Ok(raw
        .into_iter()
        .map(|label| {
            if label < 0 {
                None
            } else {
                Some(*remap.entry(label).or_insert_with(|| {
                    let assigned = next;
                    next += 1;
                    assigned
                }))
            }
        })
        .collect())
}

fn renumber_labels(labels: Vec<usize>) -> Vec<usize> {
    let mut remap = HashMap::new();
    let mut next = 0usize;
    labels
        .into_iter()
        .map(|label| {
            *remap.entry(label).or_insert_with(|| {
                let assigned = next;
                next += 1;
                assigned
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(values: &[f32]) -> Embedding {
        let mut vector = Embedding::new(values.to_vec());
        vector.normalize();
        vector
    }

    #[test]
    fn exact_knn_excludes_self_and_orders_by_distance() {
        let distances = vec![
            vec![0.0, 0.2, 0.8],
            vec![0.2, 0.0, 0.4],
            vec![0.8, 0.4, 0.0],
        ];
        let (neighbors, _) = exact_knn(&distances, 2);
        assert_eq!(neighbors[0], vec![1, 2]);
    }

    #[test]
    fn kmeans_is_deterministic_and_separates_obvious_groups() {
        let vectors = vec![
            vector(&[1.0, 0.0]),
            vector(&[0.9, 0.1]),
            vector(&[0.0, 1.0]),
            vector(&[0.1, 0.9]),
        ];
        let first = deterministic_kmeans(&vectors, 2);
        assert_eq!(first, deterministic_kmeans(&vectors, 2));
        assert_eq!(first[0], first[1]);
        assert_eq!(first[2], first[3]);
        assert_ne!(first[0], first[2]);
    }

    #[test]
    fn normalized_layout_is_finite_and_padded() {
        let normalized = normalize_layout(&[(10.0, -5.0), (20.0, 5.0)]);
        assert!(normalized
            .iter()
            .all(|(x, y)| x.is_finite() && y.is_finite()));
        assert!(normalized.iter().all(|(x, y)| *x >= 0.1 && *y >= 0.1));
    }

    #[test]
    fn hdbscan_marks_a_lonely_point_as_noise() {
        let vectors = vec![
            vector(&[1.0, 0.0, 0.0]),
            vector(&[0.99, 0.01, 0.0]),
            vector(&[0.98, 0.02, 0.0]),
            vector(&[0.0, 1.0, 0.0]),
            vector(&[0.01, 0.99, 0.0]),
            vector(&[0.02, 0.98, 0.0]),
            vector(&[0.0, 0.0, 1.0]),
        ];
        let labels = hdbscan_labels(&vectors, 3).unwrap();
        assert!(labels.last().unwrap().is_none());
    }
}
