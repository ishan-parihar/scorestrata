//! Knowledge graph / ontology layer (§9.6).
//!
//! The architecture spec models relationships such as:
//!
//! ```text
//! Diagnostic → affects → Metric
//! Metric → evidence in → Segment
//! Patch → fixes → FailureClass
//! Lesson → derived from → Experiment
//! Task → implements → Spec
//! Spec → verified by → GoldenTest
//! Candidate → child of → Snapshot
//! PreferenceSignal → calibrates → GenreProfile
//! ```
//!
//! A graph layer makes the system more queryable and more learnable. Agents
//! can ask "which patches most often fix weak transitions in melodic techno?",
//! "which metric regresses most when density increases?", or "which tasks
//! block the validator milestone?" — this module provides the typed graph and
//! the query helpers that answer them.

use std::collections::{BTreeMap, BTreeSet};

use scorestrata_core::diagnostic::Diagnostic;
use serde::{Deserialize, Serialize};

/// Node kinds in the ontology (§9.6 entity vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// A diagnostic packet.
    Diagnostic,
    /// A metric sample.
    Metric,
    /// A musical segment.
    Segment,
    /// A musical layer.
    Layer,
    /// A patch operation.
    Patch,
    /// A failure class (e.g. `weak_transition`).
    FailureClass,
    /// A learned lesson.
    Lesson,
    /// A recorded experiment.
    Experiment,
    /// An engineering task.
    Task,
    /// An executable spec.
    Spec,
    /// A golden test.
    GoldenTest,
    /// A candidate artifact.
    Candidate,
    /// A project snapshot.
    Snapshot,
    /// A preference signal.
    PreferenceSignal,
    /// A genre calibration profile.
    GenreProfile,
    /// A project.
    Project,
}

impl NodeKind {
    /// Stable string tag for the kind (used in node IDs and serialization).
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Diagnostic => "diagnostic",
            Self::Metric => "metric",
            Self::Segment => "segment",
            Self::Layer => "layer",
            Self::Patch => "patch",
            Self::FailureClass => "failure_class",
            Self::Lesson => "lesson",
            Self::Experiment => "experiment",
            Self::Task => "task",
            Self::Spec => "spec",
            Self::GoldenTest => "golden_test",
            Self::Candidate => "candidate",
            Self::Snapshot => "snapshot",
            Self::PreferenceSignal => "preference_signal",
            Self::GenreProfile => "genre_profile",
            Self::Project => "project",
        }
    }
}

/// Edge kinds — the typed relationship vocabulary of §9.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    /// Diagnostic affects a metric.
    Affects,
    /// Metric has evidence in a segment.
    EvidenceIn,
    /// Patch fixes a failure class.
    Fixes,
    /// Lesson is derived from an experiment.
    DerivedFrom,
    /// Task implements a spec.
    Implements,
    /// Spec is verified by a golden test.
    VerifiedBy,
    /// Candidate is a child of a snapshot.
    ChildOf,
    /// Preference signal calibrates a genre profile.
    Calibrates,
    /// Generic dependency (e.g. task blocks task).
    DependsOn,
}

impl EdgeKind {
    /// Stable string tag for the edge kind.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Affects => "affects",
            Self::EvidenceIn => "evidence_in",
            Self::Fixes => "fixes",
            Self::DerivedFrom => "derived_from",
            Self::Implements => "implements",
            Self::VerifiedBy => "verified_by",
            Self::ChildOf => "child_of",
            Self::Calibrates => "calibrates",
            Self::DependsOn => "depends_on",
        }
    }
}

/// A node in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    /// Stable node ID (`<kind>:<id>`).
    pub id: String,
    /// Node kind.
    pub kind: NodeKind,
    /// Human-readable label.
    pub label: String,
}

/// A directed typed edge in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Source node ID.
    pub from: String,
    /// Target node ID.
    pub to: String,
    /// Relationship kind.
    pub kind: EdgeKind,
}

/// Knowledge graph — the queryable ontology of entities and relationships.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    /// Nodes by ID.
    pub nodes: BTreeMap<String, GraphNode>,
    /// Directed edges.
    pub edges: Vec<GraphEdge>,
}

/// Compose a stable, fully-qualified node ID from a kind and a raw ID.
///
/// Always use this when building node IDs for queries, so callers never
/// depend on the internal `kind:id` formatting.
#[must_use]
pub fn node_id(kind: NodeKind, raw: &str) -> String {
    format!("{}:{raw}", kind.tag())
}

impl KnowledgeGraph {
    /// Create a new empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace a node.
    pub fn add_node(&mut self, kind: NodeKind, id: &str, label: &str) {
        let full_id = format!("{}:{id}", kind.tag());
        self.nodes.insert(
            full_id.clone(),
            GraphNode {
                id: full_id,
                kind,
                label: label.to_string(),
            },
        );
    }

    /// Add a typed edge; returns `false` when either endpoint is missing.
    pub fn link(
        &mut self,
        from_kind: NodeKind,
        from_id: &str,
        to_kind: NodeKind,
        to_id: &str,
        kind: EdgeKind,
    ) -> bool {
        let from = format!("{}:{from_id}", from_kind.tag());
        let to = format!("{}:{to_id}", to_kind.tag());
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return false;
        }
        let edge = GraphEdge { from, to, kind };
        if !self.edges.contains(&edge) {
            self.edges.push(edge);
        }
        true
    }

    /// Get a node by ID (full `<kind>:<id>` form).
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    /// Outgoing edges from a node.
    #[must_use]
    pub fn outgoing(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.edges.iter().filter(|e| e.from == node_id).collect()
    }

    /// Incoming edges to a node.
    #[must_use]
    pub fn incoming(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.edges.iter().filter(|e| e.to == node_id).collect()
    }

    /// Neighbors reachable from a node via a specific edge kind.
    #[must_use]
    pub fn neighbors(&self, node_id: &str, kind: EdgeKind) -> Vec<&GraphNode> {
        self.outgoing(node_id)
            .into_iter()
            .filter(|e| e.kind == kind)
            .filter_map(|e| self.nodes.get(&e.to))
            .collect()
    }

    /// Which patches fix a given failure class or metric, ranked by how many
    /// distinct targets each patch fixes across the whole graph (§9.6).
    ///
    /// Answers: "Which patches most often fix weak transitions?" — a patch
    /// that also fixes several related failures is the most valuable fixer.
    /// Each patch→target pair is unique (edges are deduped), so ranking by
    /// edges to the single target would be flat; ranking by *global* Fixes
    /// coverage is the meaningful signal.
    #[must_use]
    pub fn frequent_fixers(&self, target_id: &str) -> Vec<(&GraphNode, usize)> {
        // Global coverage: how many distinct targets each patch fixes.
        let mut coverage: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for edge in self.edges.iter().filter(|e| e.kind == EdgeKind::Fixes) {
            coverage
                .entry(edge.from.clone())
                .or_default()
                .insert(edge.to.clone());
        }
        let mut ranked: Vec<(&GraphNode, usize)> = coverage
            .into_iter()
            .filter(|(from, _)| {
                self.edges
                    .iter()
                    .any(|e| e.kind == EdgeKind::Fixes && e.from == *from && e.to == target_id)
            })
            .filter_map(|(id, targets)| self.nodes.get(&id).map(|node| (node, targets.len())))
            .collect();
        ranked.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        ranked
    }

    /// Which tasks block a given task (transitive `DependsOn` closure, §9.6).
    ///
    /// Answers: "Which tasks block the validator milestone?" — every task
    /// that must complete (directly or transitively) before `task_id` can
    /// proceed, in dependency order. Requires the tasks to be wired with
    /// `EdgeKind::DependsOn` edges (`A → B` reads "A depends on B").
    #[must_use]
    pub fn blocking_tasks(&self, task_id: &str) -> Vec<&GraphNode> {
        // Walk outgoing DependsOn edges transitively: the targets this task
        // depends on are its blockers; their dependencies block too.
        let mut blockers = Vec::new();
        let mut seen = BTreeSet::new();
        let mut queue = vec![task_id.to_string()];
        while let Some(current) = queue.pop() {
            for edge in self.outgoing(&current) {
                if edge.kind != EdgeKind::DependsOn {
                    continue;
                }
                if seen.insert(edge.to.clone()) {
                    if let Some(node) = self.nodes.get(&edge.to) {
                        blockers.push(node);
                    }
                    queue.push(edge.to.clone());
                }
            }
        }
        blockers
    }

    /// Ingest a validation report's diagnostics into the ontology.
    ///
    /// Adds `Diagnostic → affects → Metric` edges and, for each suggested
    /// patch, `Patch → fixes → Metric` edges — the substrate for the
    /// "which patches fix which failures" query.
    pub fn ingest_diagnostics(&mut self, project_id: &str, diagnostics: &[Diagnostic]) {
        self.add_node(
            NodeKind::Project,
            project_id,
            &format!("project {project_id}"),
        );
        for diagnostic in diagnostics {
            let diag_id = format!("{}:{}", diagnostic.code, diagnostic.id.as_hex());
            self.add_node(
                NodeKind::Diagnostic,
                &diag_id,
                &format!("{} ({})", diagnostic.code, diagnostic.metric),
            );
            let metric_id = format!("{}:{}", diagnostic.metric, project_id);
            self.add_node(NodeKind::Metric, &metric_id, &diagnostic.metric);
            let _ = self.link(
                NodeKind::Diagnostic,
                &diag_id,
                NodeKind::Metric,
                &metric_id,
                EdgeKind::Affects,
            );
            // Scope: the metric has evidence in the affected segment.
            if let Some(segment_id) = diagnostic.scope.segment_id {
                let seg_ref = segment_id.as_hex();
                self.add_node(NodeKind::Segment, &seg_ref, &format!("segment {seg_ref}"));
                let _ = self.link(
                    NodeKind::Metric,
                    &metric_id,
                    NodeKind::Segment,
                    &seg_ref,
                    EdgeKind::EvidenceIn,
                );
            }
            // Suggested patches fix the metric's failure class.
            for patch in &diagnostic.suggested_patches {
                let patch_id = format!("{}:{}", patch.op, project_id);
                self.add_node(NodeKind::Patch, &patch_id, &patch.op);
                let _ = self.link(
                    NodeKind::Patch,
                    &patch_id,
                    NodeKind::Metric,
                    &metric_id,
                    EdgeKind::Fixes,
                );
            }
        }
    }

    /// Total node count.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Total edge count.
    #[must_use]
    pub const fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::diagnostic::{MetricScope, SuggestedPatch};
    use scorestrata_core::error::Severity;
    use scorestrata_core::ids::{DiagnosticId, SegmentId};

    fn sample_diagnostic(code: &str, metric: &str, op: &str) -> Diagnostic {
        Diagnostic {
            id: DiagnosticId::new(),
            code: code.to_string(),
            metric: metric.to_string(),
            severity: Severity::Warning,
            scope: MetricScope {
                segment_id: Some(SegmentId::new()),
                layer_id: None,
                bar_range: None,
            },
            evidence_uris: vec![],
            message: "test".to_string(),
            root_causes: vec![],
            suggested_patches: vec![SuggestedPatch {
                op: op.to_string(),
                params: serde_json::json!({}),
                expected_delta: None,
                confidence: 0.8,
            }],
            expected_delta: None,
            confidence: 0.8,
        }
    }

    #[test]
    fn graph_records_typed_relationships() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(NodeKind::Patch, "p1", "add_drum_fill");
        graph.add_node(NodeKind::FailureClass, "weak_transition", "weak transition");
        assert!(graph.link(
            NodeKind::Patch,
            "p1",
            NodeKind::FailureClass,
            "weak_transition",
            EdgeKind::Fixes
        ));
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
        assert_eq!(
            graph.neighbors("patch:p1", EdgeKind::Fixes)[0].id,
            "failure_class:weak_transition"
        );
    }

    #[test]
    fn link_requires_both_endpoints() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(NodeKind::Patch, "p1", "add_drum_fill");
        assert!(!graph.link(
            NodeKind::Patch,
            "p1",
            NodeKind::FailureClass,
            "missing",
            EdgeKind::Fixes
        ));
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn ingest_diagnostics_builds_ontology() {
        let mut graph = KnowledgeGraph::new();
        let diagnostics = vec![
            sample_diagnostic(
                "TRANSITION_ENERGY_DELTA_LOW",
                "arrangement.transition_energy",
                "add_transition_package",
            ),
            sample_diagnostic(
                "HARMONY_SCALE_VIOLATION",
                "harmony.scale_compliance",
                "quantize_layer_to_scale",
            ),
        ];
        graph.ingest_diagnostics("p1", &diagnostics);
        // Project + 2 diagnostics + 2 metrics + 2 segments + 2 patches.
        assert_eq!(graph.node_count(), 9);
        // 2 Affects + 2 EvidenceIn + 2 Fixes.
        assert_eq!(graph.edge_count(), 6);
    }

    #[test]
    fn frequent_fixers_ranks_patches_by_coverage() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(NodeKind::Patch, "quantize", "quantize_layer_to_scale");
        graph.add_node(NodeKind::Patch, "transition", "add_transition_package");
        graph.add_node(NodeKind::Metric, "m1", "harmony.scale_compliance");
        graph.add_node(NodeKind::Metric, "m2", "arrangement.transition_energy");
        graph.add_node(NodeKind::Metric, "m3", "mix.masking");
        // quantize fixes m1 and m3; transition fixes m2.
        graph.link(
            NodeKind::Patch,
            "quantize",
            NodeKind::Metric,
            "m1",
            EdgeKind::Fixes,
        );
        graph.link(
            NodeKind::Patch,
            "quantize",
            NodeKind::Metric,
            "m3",
            EdgeKind::Fixes,
        );
        graph.link(
            NodeKind::Patch,
            "transition",
            NodeKind::Metric,
            "m2",
            EdgeKind::Fixes,
        );
        // transition also fixes m1 — both patches fix it, so the ranker must
        // fall back to global coverage to order them.
        graph.link(
            NodeKind::Patch,
            "transition",
            NodeKind::Metric,
            "m1",
            EdgeKind::Fixes,
        );
        let fixers = graph.frequent_fixers("metric:m1");
        assert_eq!(fixers.len(), 2);
        assert_eq!(fixers[0].0.id, "patch:quantize");
        assert_eq!(fixers[0].1, 2); // fixes m1 + m3
        assert_eq!(fixers[1].0.id, "patch:transition");
        assert_eq!(fixers[1].1, 2); // fixes m2 + m1
    }

    #[test]
    fn blocking_tasks_returns_transitive_dependencies() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(NodeKind::Task, "core-001", "Define core IDs");
        graph.add_node(NodeKind::Task, "theory-001", "Implement scales");
        graph.add_node(
            NodeKind::Task,
            "validator-001",
            "Implement scale compliance",
        );
        graph.add_node(NodeKind::Task, "golden-001", "Golden harness");
        // golden → validator → theory → core.
        graph.link(
            NodeKind::Task,
            "golden-001",
            NodeKind::Task,
            "validator-001",
            EdgeKind::DependsOn,
        );
        graph.link(
            NodeKind::Task,
            "validator-001",
            NodeKind::Task,
            "theory-001",
            EdgeKind::DependsOn,
        );
        graph.link(
            NodeKind::Task,
            "theory-001",
            NodeKind::Task,
            "core-001",
            EdgeKind::DependsOn,
        );
        let blockers = graph.blocking_tasks("task:golden-001");
        assert_eq!(blockers.len(), 3);
        assert!(blockers.iter().any(|n| n.id == "task:core-001"));
        assert!(blockers.iter().any(|n| n.id == "task:validator-001"));
        assert!(!blockers.iter().any(|n| n.id == "task:golden-001"));
    }

    #[test]
    fn ingest_dedupes_shared_metrics() {
        // Two diagnostics on the SAME metric collapse to one metric node.
        let mut graph = KnowledgeGraph::new();
        let a = sample_diagnostic(
            "SCALE_COMPLIANCE_VIOLATION",
            "harmony.scale_compliance",
            "quantize_layer_to_scale",
        );
        let mut b = sample_diagnostic(
            "SCALE_COMPLIANCE_VIOLATION_2",
            "harmony.scale_compliance",
            "quantize_layer_to_scale",
        );
        b.scope.segment_id = a.scope.segment_id; // same segment too
        let diagnostics = vec![a, b];
        graph.ingest_diagnostics("p1", &diagnostics);
        let metric_nodes = graph
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Metric)
            .count();
        assert_eq!(metric_nodes, 1, "shared metric collapses to one node");
        // Project + 2 diagnostics + 1 metric + 1 segment + 1 patch.
        assert_eq!(graph.node_count(), 6);
    }

    #[test]
    fn graph_round_trips_through_json() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(NodeKind::Lesson, "l1", "transition fix lesson");
        graph.add_node(NodeKind::Experiment, "e1", "exp");
        graph.link(
            NodeKind::Lesson,
            "l1",
            NodeKind::Experiment,
            "e1",
            EdgeKind::DerivedFrom,
        );
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string(&graph).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let back: KnowledgeGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(back.node_count(), graph.node_count());
        assert_eq!(back.edge_count(), graph.edge_count());
    }
}
