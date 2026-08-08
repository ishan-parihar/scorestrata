//! §7.3 Incremental compilation + §7.2 pass observability.
//!
//! Agents iterate constantly, so the compiler must recompute only what
//! changed — pattern, segment, layer, track, automation, mixer state — not
//! the whole project every time. Fast loops are a core architectural
//! feature, not a performance nicety (architecture.md §7.3).
//!
//! This module provides:
//! - [`CompilationCache`]: a per-project, persistable cache keyed by
//!   content hashes of each segment's dependencies (the segment's own
//!   fields, its layers including automation/FX, the referenced patterns,
//!   and the time-signature config that fixes tick math).
//! - [`incremental_compile`]: recompiles only invalidated segments and
//!   reuses cached event lists for everything else, producing a timeline
//!   **byte-identical** to [`crate::compile::compile_project`] while
//!   reporting exactly what changed and why (pass observability §7.2).
//!
//! Dependency granularity mirrors the invalidation units the spec lists:
//! patterns, layers (which carry automation + mixer inserts), segment
//! structure, and config. Tracks are derived from layer ids (§
//! `track_id_for_layer`), so a layer change covers its track.

use std::collections::BTreeMap;
use std::path::Path;

use scorestrata_core::domain::Project;
use scorestrata_core::error::ScorestrataError;
use scorestrata_core::ids::{LayerId, PatternId, SegmentId};

use crate::arrangement::{ExpandedSegment, expand_arrangement};
use crate::compile::compile_segment_events;
use crate::timeline::{TimelineEvent, TimelineIR};

/// Why a segment was invalidated (§7.3 change units).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InvalidationReason {
    /// A referenced pattern's notes/steps changed.
    PatternChanged(PatternId),
    /// A layer's content changed (instances, register, automation, FX).
    LayerChanged(LayerId),
    /// The segment's structure changed (bars, role, name, start bar).
    SegmentStructureChanged,
    /// The time-signature config that fixes tick math changed.
    ConfigChanged,
    /// First compile of this segment (no prior cache entry).
    FirstCompile,
}

/// A single invalidation record for pass observability (§7.2).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Invalidation {
    /// The segment whose cached events were discarded.
    pub segment_id: SegmentId,
    /// Why it was invalidated.
    pub reason: InvalidationReason,
}

/// §7.2 pass observability — what one incremental compile actually did.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompileObservability {
    /// Input hash: combined dependency hash of all compiled segments.
    pub input_hash: String,
    /// Output hash: the produced timeline's content hash.
    pub output_hash: String,
    /// Wall-clock duration of the compile.
    pub duration_ms: u128,
    /// Total segments considered.
    pub segments_total: usize,
    /// Segments reused from cache (zero recomputation).
    pub segments_reused: usize,
    /// Segments recomputed (invalidated).
    pub segments_recomputed: usize,
    /// Events reused from cache.
    pub events_reused: usize,
    /// Events recomputed.
    pub events_recomputed: usize,
    /// Invalidations, in arrangement order, with reasons.
    pub invalidations: Vec<Invalidation>,
}

/// Granular dependency hashes for one segment, used both to detect change
/// and to attribute the *reason* for it.
#[derive(Debug, Clone)]
struct SegmentDependencies {
    /// Segment fields that affect events (start bar, bars, role, name).
    segment_hash: String,
    /// Time-signature numerator (fixes ticks-per-bar math).
    config_hash: String,
    /// Content hash per referenced pattern.
    pattern_hashes: BTreeMap<PatternId, String>,
    /// Content hash per layer (includes instances, register, automation, FX).
    layer_hashes: BTreeMap<LayerId, String>,
    /// Combined hash — the cache key.
    combined: String,
}

/// A cached entry for one segment.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SegmentCacheEntry {
    /// Combined dependency hash that produced `events`.
    dependency_hash: String,
    /// Granular hashes for invalidation attribution on the next compile.
    segment_hash: String,
    config_hash: String,
    pattern_hashes: BTreeMap<PatternId, String>,
    layer_hashes: BTreeMap<LayerId, String>,
    /// Cached events (local ids; renumbered at merge).
    events: Vec<TimelineEvent>,
}

/// Per-project compilation cache for §7.3 incremental compilation.
///
/// Serializes to/from JSON so a project bundle can persist it (e.g. at
/// `<bundle>/cache/compile_cache.json`) and agent loops get fast
/// recompiles across sessions.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CompilationCache {
    segments: BTreeMap<SegmentId, SegmentCacheEntry>,
}

/// Result of an incremental compile: the timeline plus pass observability.
#[derive(Debug, Clone)]
pub struct IncrementalCompileResult {
    /// The merged timeline — identical to a full [`compile_project`](crate::compile::compile_project).
    pub timeline: TimelineIR,
    /// What the compile actually did (§7.2).
    pub observability: CompileObservability,
}

fn hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}

/// Serialize a domain entity deterministically for hashing.
///
/// Rust struct serialization is field-order stable, so the same entity
/// always hashes identically (§17 determinism rules). `unwrap_or_default`
/// mirrors `TimelineIR::content_hash` — all domain entities are plain
/// `Serialize` data types with no fallible custom serializers, so the
/// empty-hash fallback is unreachable in practice (and a collision would
/// only over-recompute, never reuse stale events).
fn entity_hash<T: serde::Serialize>(entity: &T) -> String {
    hash(&serde_json::to_vec(entity).unwrap_or_default())
}

impl CompilationCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of cached segments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Whether the cache holds no segments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.segments.clear();
    }

    /// Which segments changed since the cache was last compiled (§22.2
    /// `diff` scope).
    ///
    /// A segment is "changed" if it has no cache entry yet, or its current
    /// dependency hash differs from the cached one. This lets the validator
    /// run the `diff` scope — validate only what changed — which pairs with
    /// [`incremental_compile`] to make agent edit loops fast.
    ///
    /// # Errors
    ///
    /// Returns an error if the arrangement is invalid or a referenced
    /// pattern is missing (same failure modes as a full compile).
    pub fn changed_segments(&self, project: &Project) -> Result<Vec<SegmentId>, ScorestrataError> {
        let expanded = expand_arrangement(&project.arrangement)?;
        let mut changed = Vec::new();
        for expanded_segment in &expanded {
            let deps = segment_dependencies(project, expanded_segment)?;
            let stale = self
                .segments
                .get(&expanded_segment.segment.id)
                .is_none_or(|entry| entry.dependency_hash != deps.combined);
            if stale {
                changed.push(expanded_segment.segment.id);
            }
        }
        Ok(changed)
    }

    /// Persist the cache to a JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`ScorestrataError::Io`] if the file cannot be written, or
    /// [`ScorestrataError::Serialization`] if serialization fails.
    pub fn save_to(&self, path: &Path) -> Result<(), ScorestrataError> {
        let json =
            serde_json::to_string_pretty(self).map_err(|e| ScorestrataError::Serialization {
                message: format!("Failed to serialize compile cache: {e}"),
            })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ScorestrataError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        std::fs::write(path, json).map_err(|e| ScorestrataError::Io {
            path: path.display().to_string(),
            source: e,
        })
    }

    /// Load a cache from a JSON file. A missing or corrupt file yields an
    /// empty cache rather than an error, so incremental compilation is
    /// always safe to start cold.
    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
}

/// Compute the dependency hashes for one expanded segment.
///
/// The combined hash covers every input that can change the segment's
/// events: the segment's own fields (start bar → absolute tick math),
/// each layer's full content (instances, register, automation, FX inserts,
/// mix), every referenced pattern's notes/steps, and the ticks-per-bar
/// derived from the time signature.
fn segment_dependencies(
    project: &Project,
    expanded: &ExpandedSegment,
) -> Result<SegmentDependencies, ScorestrataError> {
    let segment = &expanded.segment;

    let mut pattern_hashes = BTreeMap::new();
    let mut layer_hashes = BTreeMap::new();
    let mut combined = Vec::new();

    // Segment structure (start bar is what makes ticks absolute).
    let mut segment_for_hash = segment.clone();
    segment_for_hash.layers = Vec::new();
    let segment_hash = entity_hash(&segment_for_hash);
    combined.extend_from_slice(segment_hash.as_bytes());

    // Config: ticks-per-bar = PPQ * numerator.
    let ticks_per_bar = u64::from(scorestrata_core::domain::PPQ)
        * u64::from(project.config.time_signature.numerator);
    let config_hash = hash(&ticks_per_bar.to_le_bytes());
    combined.extend_from_slice(config_hash.as_bytes());

    // Layers (full content: instances, register, automation, FX, mix).
    for layer in &segment.layers {
        let layer_hash = entity_hash(layer);
        layer_hashes.insert(layer.id, layer_hash.clone());
        combined.extend_from_slice(layer_hash.as_bytes());
    }

    // Referenced patterns.
    let mut referenced: Vec<PatternId> = segment
        .layers
        .iter()
        .flat_map(|l| l.pattern_instances.iter().map(|i| i.pattern_id))
        .collect();
    referenced.sort_unstable();
    referenced.dedup();

    for pattern_id in referenced {
        let pattern = project
            .patterns
            .iter()
            .find(|p| p.id == pattern_id)
            .ok_or_else(|| ScorestrataError::MissingEntity {
                entity_type: "pattern".to_string(),
                entity_id: pattern_id.to_string(),
            })?;
        let pattern_hash = entity_hash(pattern);
        pattern_hashes.insert(pattern_id, pattern_hash.clone());
        combined.extend_from_slice(pattern_hash.as_bytes());
    }

    Ok(SegmentDependencies {
        segment_hash,
        config_hash,
        pattern_hashes,
        layer_hashes,
        combined: hash(&combined),
    })
}

/// Attribute why a cached segment is invalid, by diffing granular hashes.
fn attribute_reason(entry: &SegmentCacheEntry, deps: &SegmentDependencies) -> InvalidationReason {
    // Pattern changes are the most common agent edit — check first.
    for (pattern_id, new_hash) in &deps.pattern_hashes {
        if entry.pattern_hashes.get(pattern_id) != Some(new_hash) {
            return InvalidationReason::PatternChanged(*pattern_id);
        }
    }
    for (layer_id, new_hash) in &deps.layer_hashes {
        if entry.layer_hashes.get(layer_id) != Some(new_hash) {
            return InvalidationReason::LayerChanged(*layer_id);
        }
    }
    if entry.config_hash != deps.config_hash {
        return InvalidationReason::ConfigChanged;
    }
    if entry.segment_hash != deps.segment_hash {
        return InvalidationReason::SegmentStructureChanged;
    }
    // Dependencies changed at a granularity we did not track — treat as
    // a structural change so the caller always knows it was invalidated.
    InvalidationReason::SegmentStructureChanged
}

/// Incrementally compile a project against a cache.
///
/// Recomputes only segments whose dependency hash changed; reuses cached
/// event lists for everything else. The merged timeline is byte-identical
/// to [`crate::compile::compile_project`] for the same project — the cache
/// is a pure speed optimization, never a behavior change.
///
/// The returned [`CompileObservability`] satisfies §7.2 pass observability:
/// input hash, output hash, duration, and the full invalidation list with
/// reasons.
///
/// # Errors
///
/// Returns an error if the arrangement is invalid or a referenced pattern
/// is missing (same failure modes as a full compile).
pub fn incremental_compile(
    project: &Project,
    cache: &mut CompilationCache,
) -> Result<IncrementalCompileResult, ScorestrataError> {
    let started = std::time::Instant::now();
    let expanded = expand_arrangement(&project.arrangement)?;

    let mut all_segments: Vec<Vec<TimelineEvent>> = Vec::with_capacity(expanded.len());
    let mut invalidations = Vec::new();
    let mut segments_reused = 0usize;
    let mut segments_recomputed = 0usize;
    let mut events_reused = 0usize;
    let mut events_recomputed = 0usize;

    for expanded_segment in &expanded {
        let deps = segment_dependencies(project, expanded_segment)?;

        let entry = cache.segments.get(&expanded_segment.segment.id);
        let segment_events = match entry {
            Some(cached) if cached.dependency_hash == deps.combined => {
                segments_reused += 1;
                events_reused += cached.events.len();
                cached.events.clone()
            }
            Some(cached) => {
                // Cache miss: attribute the reason, recompute, store.
                segments_recomputed += 1;
                invalidations.push(Invalidation {
                    segment_id: expanded_segment.segment.id,
                    reason: attribute_reason(cached, &deps),
                });
                let fresh = compile_segment_events(project, expanded_segment)?;
                events_recomputed += fresh.len();
                cache.segments.insert(
                    expanded_segment.segment.id,
                    SegmentCacheEntry {
                        dependency_hash: deps.combined,
                        segment_hash: deps.segment_hash,
                        config_hash: deps.config_hash,
                        pattern_hashes: deps.pattern_hashes,
                        layer_hashes: deps.layer_hashes,
                        events: fresh.clone(),
                    },
                );
                fresh
            }
            None => {
                // First compile of this segment: record it as an invalidation
                // so the observability report accounts for every new unit.
                segments_recomputed += 1;
                invalidations.push(Invalidation {
                    segment_id: expanded_segment.segment.id,
                    reason: InvalidationReason::FirstCompile,
                });
                let fresh = compile_segment_events(project, expanded_segment)?;
                events_recomputed += fresh.len();
                cache.segments.insert(
                    expanded_segment.segment.id,
                    SegmentCacheEntry {
                        dependency_hash: deps.combined,
                        segment_hash: deps.segment_hash,
                        config_hash: deps.config_hash,
                        pattern_hashes: deps.pattern_hashes,
                        layer_hashes: deps.layer_hashes,
                        events: fresh.clone(),
                    },
                );
                fresh
            }
        };
        all_segments.push(segment_events);
    }

    // Merge with global ids in traversal order — identical to compile_project.
    let mut events: Vec<TimelineEvent> = Vec::new();
    let mut next_event_id: u64 = 0;
    for mut segment_events in all_segments {
        for event in &mut segment_events {
            event.id = next_event_id;
            next_event_id += 1;
        }
        events.extend(segment_events);
    }
    events.sort_by_key(|e| (e.tick, e.id));

    let total_ticks = events.iter().map(|e| e.tick.0).max().unwrap_or(0);

    let mut timeline = TimelineIR {
        project_id: project.id,
        events,
        total_ticks,
        bpm: project.config.bpm,
        time_signature_num: project.config.time_signature.numerator,
        input_hash: String::new(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    timeline.input_hash = timeline.content_hash();

    let observability = CompileObservability {
        input_hash: timeline.input_hash.clone(),
        output_hash: timeline.content_hash(),
        duration_ms: started.elapsed().as_millis(),
        segments_total: expanded.len(),
        segments_reused,
        segments_recomputed,
        events_reused,
        events_recomputed,
        invalidations,
    };

    Ok(IncrementalCompileResult {
        timeline,
        observability,
    })
}

/// Convenience: load a cache from disk, compile incrementally, save it back.
///
/// # Errors
///
/// Returns an error if the compile fails (see [`incremental_compile`]).
pub fn compile_with_cache(
    project: &Project,
    cache_path: &Path,
) -> Result<IncrementalCompileResult, ScorestrataError> {
    let mut cache = CompilationCache::load_from(cache_path);
    let result = incremental_compile(project, &mut cache)?;
    cache.save_to(cache_path)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_project;
    use scorestrata_core::domain::{
        Arrangement, Brief, DeliveryTarget, Duration, Layer, MidiNote, Note, Pattern,
        ProjectConfig, Segment, SegmentRole, Tick, TimeSignature, Velocity,
    };
    use scorestrata_core::ids::{NoteId, PatternInstanceId, ProjectId};

    fn build_project() -> Project {
        let pattern = Pattern {
            id: PatternId::new(),
            name: "lead".to_string(),
            bars: 1,
            time_signature: TimeSignature::default(),
            notes: vec![
                Note {
                    id: NoteId::new(),
                    pitch: MidiNote::new(60),
                    start: Tick::new(0),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(100),
                    tied: false,
                },
                Note {
                    id: NoteId::new(),
                    pitch: MidiNote::new(63),
                    start: Tick::new(480),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(90),
                    tied: false,
                },
            ],
            steps: Vec::new(),
        };

        let layer = Layer {
            id: LayerId::new(),
            name: "Lead".to_string(),
            instrument: "synth".to_string(),
            pattern_instances: vec![scorestrata_core::domain::PatternInstance {
                id: PatternInstanceId::new(),
                pattern_id: pattern.id,
                start_tick: Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            }],
            register: None,
            automation: vec![],
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
        };

        Project {
            id: ProjectId::new(),
            name: "Test".to_string(),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            brief: Brief {
                raw_text: "test".to_string(),
                genre: None,
                mood: vec![],
                references: vec![],
                delivery_target: None,
                constraints: vec![],
                confidence: 0.5,
            },
            config: ProjectConfig {
                name: "Test".to_string(),
                genre: "techno".to_string(),
                bpm: 124,
                key: "C minor".to_string(),
                time_signature: TimeSignature::default(),
                duration_bars: 8,
                delivery_target: DeliveryTarget::Streaming,
                mood: vec![],
                references: vec![],
            },
            arrangement: Arrangement {
                total_bars: 8,
                time_signature: TimeSignature::default(),
                segments: vec![Segment {
                    id: SegmentId::new(),
                    name: "Loop".to_string(),
                    role: SegmentRole::Chorus,
                    start_bar: 0,
                    bars: 8,
                    layers: vec![layer],
                }],
                energy_curve: Vec::new(),
            },
            tracks: vec![],
            patterns: vec![pattern],
            spectral_reference: None,
            status: scorestrata_core::domain::ProjectStatus::ConfigApproved,
        }
    }

    #[test]
    fn cold_incremental_compile_matches_full_compile() {
        let project = build_project();
        let mut cache = CompilationCache::new();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let incremental = incremental_compile(&project, &mut cache).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let full = compile_project(&project).unwrap();
        assert_eq!(incremental.timeline.content_hash(), full.content_hash());
        assert_eq!(incremental.timeline.event_count(), full.event_count());
        assert_eq!(incremental.observability.segments_recomputed, 1);
        assert_eq!(incremental.observability.segments_reused, 0);
    }

    #[test]
    fn warm_cache_reuses_everything() {
        let project = build_project();
        let mut cache = CompilationCache::new();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let first = incremental_compile(&project, &mut cache).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let second = incremental_compile(&project, &mut cache).unwrap();
        assert_eq!(
            first.timeline.content_hash(),
            second.timeline.content_hash()
        );
        assert_eq!(second.observability.segments_reused, 1);
        assert_eq!(second.observability.segments_recomputed, 0);
        assert_eq!(
            second.observability.events_reused,
            first.timeline.event_count()
        );
        assert!(second.observability.invalidations.is_empty());
    }

    #[test]
    fn pattern_change_invalidates_only_that_segment() {
        let mut project = build_project();
        let mut cache = CompilationCache::new();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        incremental_compile(&project, &mut cache).unwrap();

        // Edit the pattern (add a third note).
        let pattern = &mut project.patterns[0];
        pattern.notes.push(Note {
            id: NoteId::new(),
            pitch: MidiNote::new(67),
            start: Tick::new(960),
            duration: Duration::quarter(),
            velocity: Velocity::new(80),
            tied: false,
        });

        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let result = incremental_compile(&project, &mut cache).unwrap();
        assert_eq!(result.observability.segments_recomputed, 1);
        assert_eq!(result.observability.segments_reused, 0);
        assert_eq!(result.observability.invalidations.len(), 1);
        assert_eq!(
            result.observability.invalidations[0].reason,
            InvalidationReason::PatternChanged(project.patterns[0].id)
        );
        // Byte-identical to a full compile of the edited project.
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let full = compile_project(&project).unwrap();
        assert_eq!(result.timeline.content_hash(), full.content_hash());
    }

    #[test]
    fn changed_segments_detects_edits_for_diff_scope() {
        let mut project = build_project();
        let mut cache = CompilationCache::new();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        incremental_compile(&project, &mut cache).unwrap();

        // Nothing changed: diff scope has nothing to validate.
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let before = cache.changed_segments(&project).unwrap();
        assert!(before.is_empty(), "no edits means no changed segments");

        // Edit the pattern: the referencing segment becomes changed.
        let segment_id = project.arrangement.segments[0].id;
        project.patterns[0].notes.push(Note {
            id: NoteId::new(),
            pitch: MidiNote::new(70),
            start: Tick::new(1440),
            duration: Duration::quarter(),
            velocity: Velocity::new(85),
            tied: false,
        });
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let after = cache.changed_segments(&project).unwrap();
        assert_eq!(after, vec![segment_id]);
    }

    #[test]
    fn cache_round_trips_through_disk() {
        let project = build_project();
        let dir = std::env::temp_dir().join(format!("ss-compile-cache-{}", uuid::Uuid::new_v4()));
        let path = dir.join("compile_cache.json");
        let mut cache = CompilationCache::new();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        incremental_compile(&project, &mut cache).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        cache.save_to(&path).unwrap();

        let mut loaded = CompilationCache::load_from(&path);
        assert_eq!(loaded.len(), 1);
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let result = incremental_compile(&project, &mut loaded).unwrap();
        assert_eq!(result.observability.segments_reused, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_change_invalidates_with_config_reason() {
        let mut project = build_project();
        let mut cache = CompilationCache::new();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        incremental_compile(&project, &mut cache).unwrap();

        project.config.time_signature = TimeSignature {
            numerator: 3,
            denominator: 4,
        };
        project.arrangement.time_signature = project.config.time_signature;

        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let result = incremental_compile(&project, &mut cache).unwrap();
        assert_eq!(result.observability.segments_recomputed, 1);
        assert_eq!(
            result.observability.invalidations[0].reason,
            InvalidationReason::ConfigChanged
        );
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let full = compile_project(&project).unwrap();
        assert_eq!(result.timeline.content_hash(), full.content_hash());
    }
}
