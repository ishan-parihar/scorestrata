//! Snapshot graph — parent chain traversal, diff, and rollback.
//!
//! The architecture spec requires: "Every project state should be
//! reconstructible from initial snapshot, event ledger, command history,
//! asset hashes, engine versions, seeds, policy profile."

use std::collections::BTreeMap;
use std::path::Path;

use scorestrata_core::error::ScorestrataError;
use scorestrata_core::ids::SnapshotId;
use scorestrata_core::snapshot::Snapshot;

use crate::StorageResult;

/// A node in the snapshot graph — tracks parent relationships.
#[derive(Debug, Clone)]
pub struct SnapshotNode {
    /// The snapshot data.
    pub snapshot: Snapshot,
    /// Parent snapshot ID (None for root/initial snapshot).
    pub parent_id: Option<SnapshotId>,
}

/// Snapshot graph — manages parent chains, diffs, and rollback.
#[derive(Debug, Clone, Default)]
pub struct SnapshotGraph {
    /// All snapshot nodes, keyed by snapshot ID hex.
    nodes: BTreeMap<String, SnapshotNode>,
    /// Root snapshot ID (the initial snapshot).
    root_id: Option<SnapshotId>,
    /// Head snapshot ID (the latest snapshot).
    head_id: Option<SnapshotId>,
}

impl SnapshotGraph {
    /// Create a new empty snapshot graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a graph from a list of snapshots with parent information.
    ///
    /// Parent relationships are inferred from `command_id`: each snapshot
    /// records the command that produced it, and the previous snapshot
    /// in chronological order is its parent.
    #[must_use]
    pub fn from_snapshots(snapshots: Vec<Snapshot>) -> Self {
        let mut graph = Self::new();
        let mut sorted = snapshots;
        sorted.sort_by_key(|a| a.created_at);

        let mut prev_id: Option<SnapshotId> = None;
        for snap in sorted {
            let node = SnapshotNode {
                snapshot: snap.clone(),
                parent_id: prev_id,
            };
            graph.nodes.insert(snap.id.as_hex(), node);
            if graph.root_id.is_none() {
                graph.root_id = Some(snap.id);
            }
            graph.head_id = Some(snap.id);
            prev_id = Some(snap.id);
        }
        graph
    }

    /// Get a snapshot by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Snapshot> {
        self.nodes.get(id).map(|n| &n.snapshot)
    }

    /// Get the root (initial) snapshot.
    #[must_use]
    pub fn root(&self) -> Option<&Snapshot> {
        self.root_id.and_then(|id| self.get(&id.as_hex()))
    }

    /// Get the head (latest) snapshot.
    #[must_use]
    pub fn head(&self) -> Option<&Snapshot> {
        self.head_id.and_then(|id| self.get(&id.as_hex()))
    }

    /// Get the parent chain from a snapshot back to the root.
    #[must_use]
    pub fn parent_chain(&self, snapshot_id: &str) -> Vec<&Snapshot> {
        let mut chain = Vec::new();
        let mut current = self.nodes.get(snapshot_id);
        while let Some(node) = current {
            chain.push(&node.snapshot);
            current = node
                .parent_id
                .as_ref()
                .and_then(|pid| self.nodes.get(&pid.as_hex()));
        }
        chain
    }

    /// Get the path (list of snapshot IDs) from root to a given snapshot.
    #[must_use]
    pub fn path_to(&self, snapshot_id: &str) -> Vec<String> {
        let chain = self.parent_chain(snapshot_id);
        chain.iter().rev().map(|s| s.id.as_hex()).collect()
    }

    /// Compute a diff between two snapshots.
    ///
    /// Returns a [`SnapshotDiff`] with field-level changes.
    #[must_use]
    pub fn diff(&self, from_id: &str, to_id: &str) -> Option<SnapshotDiff> {
        let from = self.get(from_id)?;
        let to = self.get(to_id)?;

        let mut changes = Vec::new();
        if from.branch_id != to.branch_id {
            changes.push(DiffChange {
                field: "branch_id".to_string(),
                old_value: from.branch_id.as_hex(),
                new_value: to.branch_id.as_hex(),
            });
        }
        if from.label != to.label {
            changes.push(DiffChange {
                field: "label".to_string(),
                old_value: from.label.clone(),
                new_value: to.label.clone(),
            });
        }
        if from.state_hash != to.state_hash {
            changes.push(DiffChange {
                field: "state_hash".to_string(),
                old_value: from.state_hash.clone(),
                new_value: to.state_hash.clone(),
            });
        }
        if from.scorecard_hash != to.scorecard_hash {
            changes.push(DiffChange {
                field: "scorecard_hash".to_string(),
                old_value: format!("{:?}", from.scorecard_hash),
                new_value: format!("{:?}", to.scorecard_hash),
            });
        }

        Some(SnapshotDiff {
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
            changes,
        })
    }

    /// Check if a snapshot is an ancestor of another.
    #[must_use]
    pub fn is_ancestor(&self, ancestor_id: &str, descendant_id: &str) -> bool {
        let chain = self.parent_chain(descendant_id);
        chain.iter().any(|s| s.id.as_hex() == ancestor_id)
    }

    /// Count total snapshots in the graph.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// A diff between two snapshots.
#[derive(Debug, Clone)]
pub struct SnapshotDiff {
    /// Source snapshot ID.
    pub from_id: String,
    /// Target snapshot ID.
    pub to_id: String,
    /// List of field-level changes.
    pub changes: Vec<DiffChange>,
}

/// A single field-level change in a snapshot diff.
#[derive(Debug, Clone)]
pub struct DiffChange {
    /// Field name.
    pub field: String,
    /// Old value (serialized).
    pub old_value: String,
    /// New value (serialized).
    pub new_value: String,
}

impl SnapshotDiff {
    /// Whether the diff has any changes.
    #[must_use]
    pub const fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    /// Summary string of the diff.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.changes.is_empty() {
            format!(
                "Snapshots {} and {} are identical",
                self.from_id, self.to_id
            )
        } else {
            let fields: Vec<&str> = self.changes.iter().map(|c| c.field.as_str()).collect();
            format!(
                "{} field(s) changed between {} and {}: {}",
                self.changes.len(),
                self.from_id,
                self.to_id,
                fields.join(", ")
            )
        }
    }
}

/// Persist the snapshot graph as individual JSON files in the snapshots/ directory.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if files cannot be written.
pub fn save_snapshot_graph(project_path: &Path, graph: &SnapshotGraph) -> StorageResult<()> {
    let snapshots_dir = project_path.join("snapshots");
    std::fs::create_dir_all(&snapshots_dir).map_err(|e| ScorestrataError::Io {
        path: snapshots_dir.display().to_string(),
        source: e,
    })?;

    for node in graph.nodes.values() {
        let file = snapshots_dir.join(format!("{}.json", node.snapshot.id.as_hex()));
        let data = serde_json::to_string_pretty(&node.snapshot).map_err(|e| {
            ScorestrataError::Serialization {
                message: format!("Failed to serialize snapshot: {e}"),
            }
        })?;
        std::fs::write(&file, data).map_err(|e| ScorestrataError::Io {
            path: file.display().to_string(),
            source: e,
        })?;
    }

    // Save graph metadata (root and head)
    let meta = serde_json::json!({
        "root_id": graph.root_id.map(|id| id.as_hex()),
        "head_id": graph.head_id.map(|id| id.as_hex()),
    });
    let meta_file = snapshots_dir.join("_graph.json");
    std::fs::write(&meta_file, meta.to_string()).map_err(|e| ScorestrataError::Io {
        path: meta_file.display().to_string(),
        source: e,
    })?;

    Ok(())
}

/// Load a snapshot graph from the snapshots/ directory.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if files cannot be read.
pub fn load_snapshot_graph(project_path: &Path) -> StorageResult<SnapshotGraph> {
    let snapshots_dir = project_path.join("snapshots");
    if !snapshots_dir.exists() {
        return Ok(SnapshotGraph::new());
    }

    // Read metadata
    let meta_file = snapshots_dir.join("_graph.json");
    let (root_id_str, head_id_str) = if meta_file.exists() {
        let meta_content =
            std::fs::read_to_string(&meta_file).map_err(|e| ScorestrataError::Io {
                path: meta_file.display().to_string(),
                source: e,
            })?;
        let meta: serde_json::Value =
            serde_json::from_str(&meta_content).map_err(|e| ScorestrataError::Serialization {
                message: format!("Failed to parse graph metadata: {e}"),
            })?;
        (
            meta["root_id"].as_str().map(String::from),
            meta["head_id"].as_str().map(String::from),
        )
    } else {
        (None, None)
    };

    // Read all snapshot files
    let mut graph = SnapshotGraph::new();
    let entries: Vec<_> = std::fs::read_dir(&snapshots_dir)
        .map_err(|e| ScorestrataError::Io {
            path: snapshots_dir.display().to_string(),
            source: e,
        })?
        .filter_map(Result::ok)
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "json")
                && e.file_name().to_str() != Some("_graph.json")
        })
        .collect();

    for entry in entries {
        let content = std::fs::read_to_string(entry.path()).map_err(|e| ScorestrataError::Io {
            path: entry.path().display().to_string(),
            source: e,
        })?;
        if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&content) {
            graph.nodes.insert(
                snapshot.id.as_hex(),
                SnapshotNode {
                    snapshot,
                    parent_id: None, // Reconstructed below
                },
            );
        }
    }

    // Reconstruct parent chain from chronological order
    let mut sorted_ids: Vec<String> = graph.nodes.keys().cloned().collect();
    sorted_ids.sort_by(|a, b| {
        let a_time = graph.nodes.get(a).map(|n| n.snapshot.created_at);
        let b_time = graph.nodes.get(b).map(|n| n.snapshot.created_at);
        a_time.cmp(&b_time)
    });

    let mut prev_id: Option<SnapshotId> = None;
    for id_str in &sorted_ids {
        if let Some(node) = graph.nodes.get_mut(id_str) {
            node.parent_id = prev_id;
            prev_id = Some(node.snapshot.id);
        }
    }

    // Set root and head
    if let Some(root_str) = &root_id_str {
        graph.root_id = root_str.parse::<SnapshotId>().ok();
    } else if let Some(first) = sorted_ids.first() {
        graph.root_id = first.parse::<SnapshotId>().ok();
    }

    if let Some(head_str) = &head_id_str {
        graph.head_id = head_str.parse::<SnapshotId>().ok();
    } else if let Some(last) = sorted_ids.last() {
        graph.head_id = last.parse::<SnapshotId>().ok();
    }

    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use scorestrata_core::ids::*;

    fn make_snapshot(label: &str) -> Snapshot {
        Snapshot {
            id: SnapshotId::new(),
            project_id: ProjectId::new(),
            branch_id: BranchId::new(),
            label: label.to_string(),
            created_at: Utc::now(),
            command_id: None,
            state_hash: format!("hash_{label}"),
            scorecard_hash: None,
        }
    }

    #[test]
    fn graph_tracks_parent_chain() {
        let s1 = make_snapshot("initial");
        let s2 = make_snapshot("after_patch");
        let s3 = make_snapshot("after_validate");

        let mut graph = SnapshotGraph::new();
        let id1 = s1.id;
        let id2 = s2.id;
        let id3 = s3.id;

        graph.nodes.insert(
            id1.as_hex(),
            SnapshotNode {
                snapshot: s1,
                parent_id: None,
            },
        );
        graph.nodes.insert(
            id2.as_hex(),
            SnapshotNode {
                snapshot: s2,
                parent_id: Some(id1),
            },
        );
        graph.nodes.insert(
            id3.as_hex(),
            SnapshotNode {
                snapshot: s3,
                parent_id: Some(id2),
            },
        );
        graph.root_id = Some(id1);
        graph.head_id = Some(id3);

        let chain = graph.parent_chain(&id3.as_hex());
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].label, "after_validate");
        assert_eq!(chain[1].label, "after_patch");
        assert_eq!(chain[2].label, "initial");
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test should panic on unexpected error")]
    fn diff_detects_changes() {
        let s1 = Snapshot {
            id: SnapshotId::new(),
            project_id: ProjectId::new(),
            branch_id: BranchId::new(),
            label: "v1".to_string(),
            created_at: Utc::now(),
            command_id: None,
            state_hash: "hash_a".to_string(),
            scorecard_hash: None,
        };
        let s2 = Snapshot {
            id: SnapshotId::new(),
            project_id: ProjectId::new(),
            branch_id: s1.branch_id,
            label: "v2".to_string(),
            created_at: Utc::now(),
            command_id: None,
            state_hash: "hash_b".to_string(),
            scorecard_hash: None,
        };

        let mut graph = SnapshotGraph::new();
        let id1 = s1.id;
        let id2 = s2.id;
        graph.nodes.insert(
            id1.as_hex(),
            SnapshotNode {
                snapshot: s1,
                parent_id: None,
            },
        );
        graph.nodes.insert(
            id2.as_hex(),
            SnapshotNode {
                snapshot: s2,
                parent_id: Some(id1),
            },
        );

        let diff = graph
            .diff(&id1.as_hex(), &id2.as_hex())
            .expect("Both snapshot IDs exist in graph");
        assert!(diff.has_changes());
        assert_eq!(diff.changes.len(), 2); // label + state_hash
    }

    #[test]
    fn is_ancestor_works() {
        let s1 = make_snapshot("root");
        let s2 = make_snapshot("child");
        let s3 = make_snapshot("grandchild");

        let mut graph = SnapshotGraph::new();
        let id1 = s1.id;
        let id2 = s2.id;
        let id3 = s3.id;
        graph.nodes.insert(
            id1.as_hex(),
            SnapshotNode {
                snapshot: s1,
                parent_id: None,
            },
        );
        graph.nodes.insert(
            id2.as_hex(),
            SnapshotNode {
                snapshot: s2,
                parent_id: Some(id1),
            },
        );
        graph.nodes.insert(
            id3.as_hex(),
            SnapshotNode {
                snapshot: s3,
                parent_id: Some(id2),
            },
        );

        assert!(graph.is_ancestor(&id1.as_hex(), &id3.as_hex()));
        assert!(graph.is_ancestor(&id2.as_hex(), &id3.as_hex()));
        assert!(!graph.is_ancestor(&id3.as_hex(), &id1.as_hex()));
    }
}
