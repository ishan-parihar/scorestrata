//! §22.2 Scoped validation — validate only the plane the agent is editing.
//!
//! The spec lists eight scopes: `config, arrangement, segment, layer,
//! transition, mix, full, diff`. Scoped validation keeps loops fast: an
//! agent editing one layer should not re-run the whole validator.
//!
//! This module owns the scope vocabulary and the symbolic planes
//! (everything except `mix`, which needs the audio renderer, and `diff`,
//! which needs the compiler's incremental cache). Those two are completed
//! by the CLI/MCP orchestration layers, which have the audio engine and
//! the cache available — this module stays dependency-free of them so the
//! scope contract is the single source of truth.

use scorestrata_core::diagnostic::Diagnostic;
use scorestrata_core::domain::Project;
use scorestrata_core::ids::{LayerId, SegmentId};
use scorestrata_theory::key::Key;

/// A §22.2 validation scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationScope {
    /// Project config only (`CONFIG_*` checks).
    Config,
    /// Arrangement structure (segment layout, bar coverage).
    Arrangement,
    /// A single segment (by id).
    Segment(SegmentId),
    /// Per-layer declarations across the project (`LAYER_*` + registers).
    Layer,
    /// Transition energy between adjacent segments.
    Transition,
    /// Audio mix metrics (LUFS, true peak, clipping, spectral balance).
    Mix,
    /// Harmony: scale compliance + chord tones.
    Harmony,
    /// Rhythm: drum density.
    Rhythm,
    /// Everything the symbolic planes can see without a render.
    Full,
    /// Only segments whose compile-cache dependency hash changed (§7.3).
    Diff,
}

impl ValidationScope {
    /// Parse a scope string from the CLI or MCP params.
    ///
    /// Returns `None` for unknown scopes so callers can surface a clear
    /// "unknown scope" error rather than silently running `full`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "config" => Some(Self::Config),
            "arrangement" => Some(Self::Arrangement),
            "segment" => Some(Self::Segment(SegmentId::default())),
            "layer" => Some(Self::Layer),
            "transition" => Some(Self::Transition),
            "mix" => Some(Self::Mix),
            "harmony" => Some(Self::Harmony),
            "rhythm" => Some(Self::Rhythm),
            "full" => Some(Self::Full),
            "diff" => Some(Self::Diff),
            _ => None,
        }
    }

    /// All scope names, for capability discovery and docs.
    #[must_use]
    pub const fn all() -> &'static [&'static str] {
        &[
            "config",
            "arrangement",
            "segment",
            "layer",
            "transition",
            "mix",
            "harmony",
            "rhythm",
            "full",
            "diff",
        ]
    }

    /// Human-readable scope label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Arrangement => "arrangement",
            Self::Segment(_) => "segment",
            Self::Layer => "layer",
            Self::Transition => "transition",
            Self::Mix => "mix",
            Self::Harmony => "harmony",
            Self::Rhythm => "rhythm",
            Self::Full => "full",
            Self::Diff => "diff",
        }
    }
}

/// Run the symbolic planes for a scope.
///
/// `segment` scope validates only the given segment (or all segments when
/// the id is defaulted by `parse`); `diff` is handled by the caller, which
/// provides the changed ids via [`validate_diff`].
///
/// `mix` returns an empty vec here — the audio plane needs a render, which
/// this crate cannot do. Callers (CLI/MCP) must append
/// `audio_check::generate_audio_diagnostics` when the scope is `Mix`.
#[must_use]
pub fn validate_symbolic(project: &Project, scope: ValidationScope) -> Vec<Diagnostic> {
    match scope {
        ValidationScope::Config => crate::config_check::check_config(project),
        ValidationScope::Arrangement => {
            let mut diagnostics = Vec::new();
            for segment in &project.arrangement.segments {
                if let Some(d) = crate::segment_check::check_segment_has_layers(segment) {
                    diagnostics.push(d);
                }
                if let Some(d) = crate::segment_check::check_segment_bar_count(segment, 4, 64) {
                    diagnostics.push(d);
                }
            }
            diagnostics
        }
        ValidationScope::Segment(segment_id) => {
            let mut diagnostics = Vec::new();
            for segment in &project.arrangement.segments {
                if segment.id != segment_id && segment_id != SegmentId::default() {
                    continue;
                }
                if let Some(d) = crate::segment_check::check_segment_has_layers(segment) {
                    diagnostics.push(d);
                }
                if let Some(d) = crate::segment_check::check_segment_bar_count(segment, 4, 64) {
                    diagnostics.push(d);
                }
            }
            diagnostics
        }
        ValidationScope::Layer => {
            let mut diagnostics = crate::layer_check::check_layers(project);
            diagnostics.extend(crate::register_check::check_layer_registers(project));
            diagnostics
        }
        ValidationScope::Transition => {
            let segments = &project.arrangement.segments;
            if segments.len() >= 2 {
                crate::transition_check::check_all_transitions(segments, 0.15)
            } else {
                Vec::new()
            }
        }
        ValidationScope::Harmony => {
            let mut diagnostics = Vec::new();
            if let Some(key) = Key::parse(&project.config.key) {
                for pattern in &project.patterns {
                    diagnostics.extend(
                        crate::scale_check::check_scale_compliance(
                            pattern,
                            &key,
                            SegmentId::default(),
                            LayerId::default(),
                        )
                        .diagnostics,
                    );
                }
            }
            diagnostics
        }
        ValidationScope::Rhythm => {
            let mut diagnostics = Vec::new();
            for pattern in &project.patterns {
                let density = crate::density::rhythmic_density(pattern);
                if let Some(d) = crate::density::check_drum_density(
                    pattern,
                    density,
                    SegmentId::default(),
                    LayerId::default(),
                ) {
                    diagnostics.push(d);
                }
            }
            diagnostics
        }
        ValidationScope::Full => {
            let mut diagnostics = crate::config_check::check_config(project);
            diagnostics.extend(validate_symbolic(project, ValidationScope::Arrangement));
            diagnostics.extend(validate_symbolic(project, ValidationScope::Layer));
            diagnostics.extend(validate_symbolic(project, ValidationScope::Transition));
            diagnostics.extend(validate_symbolic(project, ValidationScope::Harmony));
            diagnostics.extend(validate_symbolic(project, ValidationScope::Rhythm));
            diagnostics
        }
        // These planes need the caller's audio engine / compile cache.
        ValidationScope::Mix | ValidationScope::Diff => Vec::new(),
    }
}

/// Run the segment + layer planes for only the given segments.
///
/// This is the `diff` scope body: the caller computes the changed segment
/// ids from the §7.3 compile cache, and only those segments are validated —
/// the "validate only what changed" fast loop.
#[must_use]
pub fn validate_diff(project: &Project, changed: &[SegmentId]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        if !changed.contains(&segment.id) {
            continue;
        }
        if let Some(d) = crate::segment_check::check_segment_has_layers(segment) {
            diagnostics.push(d);
        }
        if let Some(d) = crate::segment_check::check_segment_bar_count(segment, 4, 64) {
            diagnostics.push(d);
        }
        for layer in &segment.layers {
            if let Some(d) = crate::layer_check::check_layer(layer) {
                diagnostics.push(d);
            }
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{
        Arrangement, Brief, DeliveryTarget, Layer, PatternInstance, ProjectConfig, Segment,
        SegmentRole, Tick, TimeSignature,
    };
    use scorestrata_core::ids::{LayerId, PatternId, ProjectId, SegmentId};

    fn project() -> Project {
        Project {
            id: ProjectId::new(),
            name: "test".to_string(),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            brief: Brief {
                raw_text: "test".to_string(),
                genre: None,
                mood: vec![],
                references: vec![],
                delivery_target: Some(DeliveryTarget::Streaming),
                constraints: vec![],
                confidence: 0.5,
            },
            config: ProjectConfig {
                name: "test".to_string(),
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
                    id: SegmentId::default(),
                    name: "loop".to_string(),
                    role: SegmentRole::Chorus,
                    start_bar: 0,
                    bars: 8,
                    layers: vec![Layer {
                        id: LayerId::default(),
                        name: "lead".to_string(),
                        instrument: "synth".to_string(),
                        pattern_instances: vec![PatternInstance {
                            id: scorestrata_core::ids::PatternInstanceId::new(),
                            pattern_id: PatternId::new(),
                            start_tick: Tick::new(0),
                            transpose: 0,
                            velocity_multiplier: 1.0,
                        }],
                        register: None,
                        automation: vec![],
                        fx: None,
                        mix_gain_db: None,
                        mix_pan: None,
                        group: None,
                    }],
                }],
                energy_curve: Vec::new(),
            },
            tracks: vec![],
            patterns: vec![],
            spectral_reference: None,
            mixer: scorestrata_core::domain::MixerState::default(),
            sound_bank: None,
            bus_graph: None,
            humanize: None,
            master_chain: None,
            status: scorestrata_core::domain::ProjectStatus::ConfigApproved,
        }
    }

    #[test]
    fn parse_accepts_all_spec_scopes() {
        for scope in ValidationScope::all() {
            assert!(ValidationScope::parse(scope).is_some(), "{scope}");
        }
        assert!(ValidationScope::parse("bogus").is_none());
    }

    #[test]
    fn config_scope_only_emits_config_checks() {
        let mut project = project();
        project.config.bpm = 300;
        let diagnostics = validate_symbolic(&project, ValidationScope::Config);
        assert!(diagnostics.iter().all(|d| d.code.starts_with("CONFIG_")));
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "CONFIG_BPM_OUT_OF_RANGE")
        );
    }

    #[test]
    fn layer_scope_aggregates_layer_and_register_checks() {
        let project = project();
        let diagnostics = validate_symbolic(&project, ValidationScope::Layer);
        // The well-formed project yields no layer diagnostics.
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn segment_scope_targets_only_requested_segment() {
        let project = project();
        let other = SegmentId::new();
        let diagnostics = validate_symbolic(&project, ValidationScope::Segment(other));
        // Default-id segment (the only one) is skipped when another id asked.
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn full_scope_covers_all_symbolic_planes() {
        let project = project();
        let diagnostics = validate_symbolic(&project, ValidationScope::Full);
        // Healthy project: no diagnostics from any plane.
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn diff_scope_validates_only_changed_segments() {
        let mut project = project();
        let untouched = SegmentId::default();
        let changed_id = SegmentId::new();
        project.arrangement.segments.push(Segment {
            id: changed_id,
            name: "empty".to_string(),
            role: SegmentRole::Verse,
            start_bar: 8,
            bars: 8,
            layers: vec![],
        });
        let diagnostics = validate_diff(&project, &[untouched]);
        // Only the changed segment's checks run — the empty segment is not
        // touched, so no SEGMENT_EMPTY is reported for it.
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.scope.segment_id == Some(changed_id))
        );
        let diagnostics = validate_diff(&project, &[changed_id]);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "SEGMENT_EMPTY" && d.scope.segment_id == Some(changed_id))
        );
    }
}
