//! Presentation safeguards for semantic graph layouts.
//!
//! UMAP is allowed to express the embedding geometry, but a technically valid near-collinear
//! result is not useful in an interactive graph. These helpers retain healthy layouts and replace
//! degenerate output with deterministic cluster islands.

use std::collections::{BTreeMap, HashMap};

use super::algorithms::{circle_layout, normalize_layout};
use super::GraphMode;
use crate::model::Note;

const MIN_USEFUL_SECONDARY_AXIS_RATIO: f32 = 0.08;
const CLUSTER_CENTER_RADIUS: f32 = 0.31;
const CLUSTER_MEMBER_RADIUS: f32 = 0.10;

pub(super) type CategoryLayout = (Vec<(f32, f32)>, Vec<Option<usize>>, Vec<bool>);

pub(super) fn automatic_neighbor_count(mode: GraphMode, note_count: usize) -> usize {
    if note_count <= 2 {
        return note_count.saturating_sub(1).max(1);
    }
    let scale = (note_count as f32).sqrt().round() as usize;
    match mode {
        GraphMode::Pillars => (scale * 3).clamp(5, 30),
        GraphMode::Communities => (scale * 2).clamp(4, 20),
        GraphMode::Density => (scale * 2).clamp(5, 25),
        GraphMode::Categories | GraphMode::Timeline | GraphMode::Kanban => 15,
    }
    .min(note_count - 1)
}

pub(super) fn automatic_theme_count(note_count: usize) -> usize {
    if note_count <= 2 {
        return note_count.max(1);
    }
    ((note_count as f32 / 2.0).sqrt().round() as usize).clamp(2, 8)
}

pub(super) fn automatic_minimum_cluster_size(note_count: usize) -> usize {
    if note_count <= 3 {
        return 2;
    }
    ((note_count as f32).sqrt().round() as usize).clamp(3, 12)
}

pub(super) fn category_layout_for_notes(notes: &[Note]) -> CategoryLayout {
    let mut category_ids = BTreeMap::new();
    for note in notes {
        let key = note
            .categories
            .first()
            .cloned()
            .unwrap_or_else(|| "uncategorized".into());
        let next = category_ids.len();
        category_ids.entry(key).or_insert(next);
    }
    let mut labels = Vec::with_capacity(notes.len());
    let mut positions = vec![(0.0, 0.0); notes.len()];
    let group_count = category_ids.len().max(1);
    let mut members_by_group: HashMap<usize, Vec<usize>> = HashMap::new();
    for (note_index, note) in notes.iter().enumerate() {
        let key = note
            .categories
            .first()
            .cloned()
            .unwrap_or_else(|| "uncategorized".into());
        let group = category_ids[&key];
        labels.push(Some(group));
        members_by_group.entry(group).or_default().push(note_index);
    }
    for (group, members) in members_by_group {
        let group_angle = group as f32 / group_count as f32 * std::f32::consts::TAU;
        let center = (
            0.5 + group_angle.cos() * 0.30,
            0.5 + group_angle.sin() * 0.30,
        );
        let local = circle_layout(members.len());
        for (local_index, note_index) in members.iter().enumerate() {
            positions[*note_index] = (
                center.0 + (local[local_index].0 - 0.5) * 0.22,
                center.1 + (local[local_index].1 - 0.5) * 0.22,
            );
        }
    }
    (
        normalize_layout(&positions),
        labels,
        vec![false; notes.len()],
    )
}

pub(super) fn ensure_useful_cluster_layout(
    positions: Vec<(f32, f32)>,
    labels: &[Option<usize>],
) -> Vec<(f32, f32)> {
    if positions.len() < 3 || has_useful_two_dimensional_spread(&positions) {
        return positions;
    }
    deterministic_cluster_islands(labels)
}

fn has_useful_two_dimensional_spread(points: &[(f32, f32)]) -> bool {
    let (min_x, max_x, min_y, max_y) = points.iter().fold(
        (
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ),
        |bounds, point| {
            (
                bounds.0.min(point.0),
                bounds.1.max(point.0),
                bounds.2.min(point.1),
                bounds.3.max(point.1),
            )
        },
    );
    let x_span = max_x - min_x;
    let y_span = max_y - min_y;
    let primary_span = x_span.max(y_span);
    let secondary_span = x_span.min(y_span);
    primary_span > f32::EPSILON && secondary_span / primary_span >= MIN_USEFUL_SECONDARY_AXIS_RATIO
}

fn deterministic_cluster_islands(labels: &[Option<usize>]) -> Vec<(f32, f32)> {
    let mut members_by_cluster: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
    for (index, label) in labels.iter().enumerate() {
        members_by_cluster.entry(*label).or_default().push(index);
    }
    let cluster_count = members_by_cluster.len().max(1);
    let mut positions = vec![(0.5, 0.5); labels.len()];
    for (cluster_offset, members) in members_by_cluster.values().enumerate() {
        let cluster_angle = std::f32::consts::FRAC_PI_4
            + cluster_offset as f32 / cluster_count as f32 * std::f32::consts::TAU;
        let center = if cluster_count == 1 {
            (0.5, 0.5)
        } else {
            (
                0.5 + cluster_angle.cos() * CLUSTER_CENTER_RADIUS,
                0.5 + cluster_angle.sin() * CLUSTER_CENTER_RADIUS,
            )
        };
        let local_positions = circle_layout(members.len());
        for (local_offset, note_index) in members.iter().enumerate() {
            positions[*note_index] = (
                center.0 + (local_positions[local_offset].0 - 0.5) * CLUSTER_MEMBER_RADIUS * 2.0,
                center.1 + (local_positions[local_offset].1 - 0.5) * CLUSTER_MEMBER_RADIUS * 2.0,
            );
        }
    }
    normalize_layout(&positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_defaults_scale_with_corpus_size() {
        assert_eq!(automatic_neighbor_count(GraphMode::Communities, 2), 1);
        assert_eq!(automatic_theme_count(2), 2);
        assert!(automatic_neighbor_count(GraphMode::Communities, 100) <= 20);
        assert!(automatic_theme_count(100) <= 8);
        assert!(automatic_minimum_cluster_size(100) <= 12);
    }

    #[test]
    fn line_layout_is_replaced_with_two_dimensional_cluster_islands() {
        let line = vec![(0.1, 0.5), (0.3, 0.5), (0.7, 0.5), (0.9, 0.5)];
        let labels = vec![Some(0), Some(0), Some(1), Some(1)];
        let repaired = ensure_useful_cluster_layout(line, &labels);
        assert!(has_useful_two_dimensional_spread(&repaired));
    }

    #[test]
    fn healthy_layout_is_preserved() {
        let layout = vec![(0.1, 0.1), (0.9, 0.2), (0.4, 0.9)];
        assert_eq!(
            ensure_useful_cluster_layout(layout.clone(), &[Some(0), Some(0), Some(1)]),
            layout
        );
    }
}
