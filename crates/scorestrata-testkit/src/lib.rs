//! # Scorestrata Testkit
//!
//! Test fixtures, fake projects, sample patterns, test utilities.

#![deny(missing_docs)]

use scorestrata_core::domain::{
    Arrangement, Brief, DeliveryTarget, Duration, Layer, MidiNote, Note, Pattern, Project,
    ProjectConfig, ProjectStatus, Segment, SegmentRole, Tick, TimeSignature, Velocity,
};
use scorestrata_core::ids::{LayerId, NoteId, PatternId, ProjectId, SegmentId};

/// Create a minimal valid project for testing.
#[must_use]
pub fn minimal_project() -> Project {
    Project {
        id: ProjectId::new(),
        name: "Test Project".to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        brief: Brief {
            raw_text: "Dark melodic techno".to_string(),
            genre: Some("melodic_techno".to_string()),
            mood: vec!["dark".to_string(), "driving".to_string()],
            references: Vec::new(),
            delivery_target: Some(DeliveryTarget::Streaming),
            constraints: Vec::new(),
            confidence: 0.9,
        },
        config: ProjectConfig {
            name: "Test Project".to_string(),
            genre: "melodic_techno".to_string(),
            bpm: 124,
            key: "C minor".to_string(),
            time_signature: TimeSignature::default(),
            duration_bars: 16,
            delivery_target: DeliveryTarget::Streaming,
            mood: vec!["dark".to_string()],
            references: Vec::new(),
        },
        arrangement: Arrangement {
            total_bars: 16,
            time_signature: TimeSignature::default(),
            segments: vec![
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
                    }],
                },
                Segment {
                    id: SegmentId::new(),
                    name: "Chorus".to_string(),
                    role: SegmentRole::Chorus,
                    start_bar: 8,
                    bars: 8,
                    layers: vec![Layer {
                        id: LayerId::new(),
                        name: "Drums".to_string(),
                        instrument: "drum_sampler".to_string(),
                        pattern_instances: Vec::new(),
                    }],
                },
            ],
            energy_curve: Vec::new(),
        },
        tracks: Vec::new(),
        patterns: Vec::new(),
        status: ProjectStatus::ConfigApproved,
    }
}

/// Create a test pattern with some notes.
#[must_use]
pub fn test_pattern() -> Pattern {
    let pattern_id = PatternId::new();
    let notes = vec![
        Note {
            id: NoteId::new(),
            pitch: MidiNote::new(60), // C4
            start: Tick::new(0),
            duration: Duration::quarter(),
            velocity: Velocity::new(100),
            tied: false,
        },
        Note {
            id: NoteId::new(),
            pitch: MidiNote::new(63), // Eb4
            start: Tick::new(480),
            duration: Duration::quarter(),
            velocity: Velocity::new(90),
            tied: false,
        },
        Note {
            id: NoteId::new(),
            pitch: MidiNote::new(67), // G4
            start: Tick::new(960),
            duration: Duration::quarter(),
            velocity: Velocity::new(80),
            tied: false,
        },
    ];

    Pattern {
        id: pattern_id,
        name: "Test Pattern".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}
