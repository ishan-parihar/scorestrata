//! Contract tests — executable verification that real serialized types
//! conform to their registered JSON Schemas (Phase -1 exit criteria:
//! "schemas are machine-readable, contracts are versioned").
//!
//! These tests serialize actual domain instances and verify:
//! 1. Required fields from the schema are present,
//! 2. Enum values match the serde representation,
//! 3. Numeric bounds are respected.

use crate::domain::{
    Arrangement, Brief, DeliveryTarget, Duration, Layer, MidiNote, Note, Pattern, ProjectConfig,
    Segment, SegmentRole, SpectralReference, Tick, TimeSignature, Velocity,
};
use crate::ids::{LayerId, NoteId, PatternId, SegmentId};
use crate::schema_registry::validate_instance;
use serde_json::json;

fn valid_config() -> ProjectConfig {
    ProjectConfig {
        name: "Dark Techno".to_string(),
        genre: "melodic_techno".to_string(),
        bpm: 124,
        key: "C minor".to_string(),
        time_signature: TimeSignature::default(),
        duration_bars: 16,
        delivery_target: DeliveryTarget::Streaming,
        mood: vec!["dark".to_string()],
        references: Vec::new(),
    }
}

fn valid_brief() -> Brief {
    Brief {
        raw_text: "Dark melodic techno".to_string(),
        genre: Some("melodic_techno".to_string()),
        mood: vec!["dark".to_string()],
        references: Vec::new(),
        delivery_target: Some(DeliveryTarget::Streaming),
        constraints: vec!["must be danceable".to_string()],
        confidence: 0.9,
    }
}

fn valid_note() -> Note {
    Note {
        id: NoteId::new(),
        pitch: MidiNote::new(60),
        start: Tick::new(0),
        duration: Duration::quarter(),
        velocity: Velocity::new(100),
        tied: false,
    }
}

fn valid_pattern() -> Pattern {
    Pattern {
        id: PatternId::new(),
        name: "Test Pattern".to_string(),
        bars: 4,
        time_signature: TimeSignature::default(),
        notes: vec![valid_note()],
        steps: Vec::new(),
    }
}

fn valid_segment() -> Segment {
    Segment {
        id: SegmentId::new(),
        name: "Verse".to_string(),
        role: SegmentRole::Verse,
        start_bar: 0,
        bars: 8,
        layers: vec![Layer {
            id: LayerId::new(),
            name: "Drums".to_string(),
            instrument: "drum_sampler".to_string(),
            pattern_instances: Vec::new(),
            register: None,
            automation: vec![],
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
        }],
    }
}

fn valid_arrangement() -> Arrangement {
    Arrangement {
        total_bars: 16,
        time_signature: TimeSignature::default(),
        segments: vec![valid_segment()],
        energy_curve: Vec::new(),
    }
}

#[test]
fn config_serializes_and_validates_against_schema() {
    let config = valid_config();
    let value = serde_json::to_value(&config).expect("serialize config");
    assert!(validate_instance("ProjectConfig", &value).is_ok());
}

#[test]
fn brief_serializes_and_validates_against_schema() {
    let brief = valid_brief();
    let value = serde_json::to_value(&brief).expect("serialize brief");
    assert!(validate_instance("Brief", &value).is_ok());
}

#[test]
fn note_serializes_and_validates_against_schema() {
    let note = valid_note();
    let value = serde_json::to_value(&note).expect("serialize note");
    assert!(validate_instance("Note", &value).is_ok());
}

#[test]
fn pattern_serializes_and_validates_against_schema() {
    let pattern = valid_pattern();
    let value = serde_json::to_value(&pattern).expect("serialize pattern");
    assert!(validate_instance("Pattern", &value).is_ok());
}

#[test]
fn segment_serializes_and_validates_against_schema() {
    let segment = valid_segment();
    let value = serde_json::to_value(&segment).expect("serialize segment");
    assert!(validate_instance("Segment", &value).is_ok());
}

#[test]
fn arrangement_serializes_with_valid_segments() {
    let arrangement = valid_arrangement();
    let value = serde_json::to_value(&arrangement).expect("serialize arrangement");
    // Arrangement itself isn't in the registry; verify each segment validates.
    let segments = value["segments"].as_array().expect("segments array");
    for segment in segments {
        assert!(validate_instance("Segment", segment).is_ok());
    }
}

#[test]
fn config_with_all_fields_round_trips() {
    let config = valid_config();
    let json = serde_json::to_string(&config).expect("serialize");
    let back: ProjectConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(config.name, back.name);
    assert_eq!(config.bpm, back.bpm);
    assert_eq!(config.delivery_target, back.delivery_target);
}

#[test]
fn missing_required_field_in_config_fails() {
    let value = json!({
        "name": "Dark Techno",
        "bpm": 124,
        "key": "C minor",
        "duration_bars": 16
    });
    let err = validate_instance("ProjectConfig", &value);
    assert!(err.is_err());
}

#[test]
fn all_registry_schemas_have_unique_names() {
    let mut names: Vec<&str> = crate::schema_registry::list_schema_names();
    let original_len = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), original_len, "schema names must be unique");
}

#[test]
fn registry_version_is_semver() {
    let version = crate::schema_registry::SCHEMA_REGISTRY_VERSION;
    let parts: Vec<&str> = version.split('.').collect();
    assert_eq!(parts.len(), 3, "semver must have three components");
    for part in parts {
        assert!(part.parse::<u32>().is_ok(), "semver components numeric");
    }
}

#[test]
fn spectral_reference_deviation_is_zero_on_exact_match() {
    let reference = SpectralReference {
        track_name: "ref".to_string(),
        genre: "techno".to_string(),
        band_energy_expected: [0.15, 0.25, 0.2, 0.2, 0.1, 0.1],
        centroid_expected_hz: 2400.0,
        aliasing_max: 0.02,
        loudness_target_lufs: -8.0,
    };
    let deviation = reference.deviation(
        &reference.band_energy_expected,
        reference.centroid_expected_hz,
        0.01,
    );
    assert!(
        deviation < 1e-3,
        "matching spectrum should not deviate, got {deviation}"
    );
}

#[test]
fn spectral_reference_deviation_grows_with_centroid_drift() {
    let reference = SpectralReference {
        track_name: "ref".to_string(),
        genre: "techno".to_string(),
        band_energy_expected: [0.15, 0.25, 0.2, 0.2, 0.1, 0.1],
        centroid_expected_hz: 2400.0,
        aliasing_max: 0.02,
        loudness_target_lufs: -8.0,
    };
    // Far above the expected centroid → meaningful deviation (clamped ≤ 1).
    let deviation = reference.deviation(
        &reference.band_energy_expected,
        4800.0,
        reference.aliasing_max,
    );
    assert!(
        (0.25..=0.4).contains(&deviation),
        "half-octave centroid drift should land in the weight band, got {deviation}"
    );
}

#[test]
fn spectral_reference_deviation_penalizes_aliasing_above_limit() {
    let reference = SpectralReference::default();
    let aliased = reference.deviation(&reference.band_energy_expected, 0.0, 0.8);
    let clean = reference.deviation(&reference.band_energy_expected, 0.0, 0.1);
    assert!(
        aliased > clean,
        "aliasing above the max must add deviation: {aliased} vs {clean}"
    );
}

#[test]
fn segment_role_from_template_role_maps_all_archetypes() {
    assert_eq!(SegmentRole::from_template_role("intro"), SegmentRole::Intro);
    assert_eq!(SegmentRole::from_template_role("build"), SegmentRole::Build);
    assert_eq!(SegmentRole::from_template_role("drop"), SegmentRole::Chorus);
    assert_eq!(
        SegmentRole::from_template_role("breakdown"),
        SegmentRole::Breakdown
    );
    assert_eq!(SegmentRole::from_template_role("outro"), SegmentRole::Outro);
    assert_eq!(SegmentRole::from_template_role("verse"), SegmentRole::Verse);
    assert_eq!(
        SegmentRole::from_template_role("unknown_thing"),
        SegmentRole::Custom
    );
}
