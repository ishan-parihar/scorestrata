//! # Scorestrata Testkit
//!
//! Test fixtures, fake projects, sample patterns, test utilities.
//!
//! Responsibilities per architecture spec §18.2:
//! - fixtures,
//! - fake projects,
//! - sample patterns,
//! - test utilities,
//! - snapshot helpers.

#![deny(missing_docs)]

use scorestrata_core::domain::{
    Arrangement, Brief, DeliveryTarget, Duration, Layer, MidiNote, Note, Pattern, Project,
    ProjectConfig, ProjectStatus, Segment, SegmentRole, Step, Tick, TimeSignature, Velocity,
};
use scorestrata_core::ids::{LayerId, NoteId, PatternId, ProjectId, SegmentId, StepId};

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
                        register: None,
                        automation: vec![],
                        fx: None,
                        mix_gain_db: None,
                        mix_pan: None,
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
                        register: None,
                        automation: vec![],
                        fx: None,
                        mix_gain_db: None,
                        mix_pan: None,
                    }],
                },
            ],
            energy_curve: Vec::new(),
        },
        tracks: Vec::new(),
        patterns: Vec::new(),
        spectral_reference: None,
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

/// Build a project for a given genre and duration using the planner.
///
/// Uses the deterministic arrangement planner so the returned project has
/// real segments, layers, and in-key patterns and is directly compilable.
#[must_use]
pub fn planned_project(name: &str, brief: &str) -> Project {
    // Local generate_config equivalent (avoids a hard dependency on the MCP
    // crate from testkit): detect genre/BPM/key/bars from the brief with the
    // same conservative rules, then plan the arrangement.
    let (genre, bpm, key, bars) = parse_brief_hints(brief);
    let config = ProjectConfig {
        name: name.to_string(),
        genre,
        bpm,
        key,
        time_signature: TimeSignature::default(),
        duration_bars: bars,
        delivery_target: DeliveryTarget::Streaming,
        mood: vec!["dark".to_string()],
        references: Vec::new(),
    };
    let plan = scorestrata_compiler::plan::plan_project(&config);
    Project {
        id: ProjectId::new(),
        name: name.to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        brief: Brief {
            raw_text: brief.to_string(),
            genre: Some(config.genre.clone()),
            mood: Vec::new(),
            references: Vec::new(),
            delivery_target: Some(DeliveryTarget::Streaming),
            constraints: Vec::new(),
            confidence: 0.9,
        },
        config,
        arrangement: plan.arrangement,
        tracks: Vec::new(),
        patterns: plan.patterns,
        spectral_reference: None,
        status: ProjectStatus::ConfigApproved,
    }
}

/// A melodic techno fixture project (16 bars, C minor, 124 BPM).
#[must_use]
pub fn melodic_techno_project() -> Project {
    planned_project(
        "melodic_techno_fixture",
        "Dark melodic techno at 124 BPM in C minor, 16 bars",
    )
}

/// An ambient fixture project (32 bars, A minor, 80 BPM).
#[must_use]
pub fn ambient_project() -> Project {
    planned_project(
        "ambient_fixture",
        "Chill ambient drone at 80 BPM in A minor, 32 bars",
    )
}

/// A house fixture project (16 bars, G minor, 122 BPM).
#[must_use]
pub fn house_project() -> Project {
    planned_project(
        "house_fixture",
        "Uplifting house at 122 BPM in G minor, 16 bars",
    )
}

/// A 4/4 drum pattern (16 steps): kick, snare backbeat, offbeat hats.
#[must_use]
pub fn drum_pattern_four_on_floor() -> Pattern {
    let mut steps = Vec::new();
    for i in 0..16u32 {
        let active = i % 4 == 0 || i == 4 || i == 12 || i % 2 == 1;
        let velocity = if i % 4 == 0 {
            110 // kick
        } else if i == 4 || i == 12 {
            100 // snare
        } else {
            85 // hat
        };
        steps.push(Step {
            id: StepId::new(),
            index: i,
            active,
            velocity: Velocity::new(velocity),
        });
    }
    Pattern {
        id: PatternId::new(),
        name: "Drums_4onFloor".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes: Vec::new(),
        steps,
    }
}

/// A simple bass pattern on the root and fifth of the given key.
#[must_use]
pub fn bass_pattern(key: &str) -> Pattern {
    let scale = scorestrata_theory::key::Key::parse(key)
        .map(|k| k.to_scale())
        .expect("valid key");
    let tones = scale.notes_in_range(MidiNote::new(24), MidiNote::new(48));
    let root = *tones.first().expect("scale has notes");
    let fifth = tones.get(4).copied().unwrap_or(root);

    Pattern {
        id: PatternId::new(),
        name: "Bass_RootFifth".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes: vec![
            Note {
                id: NoteId::new(),
                pitch: root,
                start: Tick::new(0),
                duration: Duration::half(),
                velocity: Velocity::new(105),
                tied: false,
            },
            Note {
                id: NoteId::new(),
                pitch: fifth,
                start: Tick::new(960),
                duration: Duration::half(),
                velocity: Velocity::new(100),
                tied: false,
            },
        ],
        steps: Vec::new(),
    }
}

/// Build an 8-bar loop project with drums, bass, and lead layers.
///
/// This is the canonical MVAS fixture: everything an agent needs to prove
/// the closed loop (brief → config → 8-bar loop → compile → validate →
/// patch → score improvement).
#[must_use]
pub fn eight_bar_loop() -> Project {
    let drums = drum_pattern_four_on_floor();
    let bass = bass_pattern("C minor");
    let lead = test_pattern();

    let loop_segment = Segment {
        id: SegmentId::new(),
        name: "Loop".to_string(),
        role: SegmentRole::Chorus,
        start_bar: 0,
        bars: 8,
        layers: vec![
            Layer {
                id: LayerId::new(),
                name: "Drums".to_string(),
                instrument: "drum_sampler".to_string(),
                pattern_instances: vec![scorestrata_core::domain::PatternInstance {
                    id: scorestrata_core::ids::PatternInstanceId::new(),
                    pattern_id: drums.id,
                    start_tick: Tick::new(0),
                    transpose: 0,
                    velocity_multiplier: 1.0,
                }],
                register: None,
                automation: vec![],
                fx: None,
                mix_gain_db: None,
                mix_pan: None,
            },
            Layer {
                id: LayerId::new(),
                name: "Bass".to_string(),
                instrument: "bass_synth".to_string(),
                pattern_instances: vec![scorestrata_core::domain::PatternInstance {
                    id: scorestrata_core::ids::PatternInstanceId::new(),
                    pattern_id: bass.id,
                    start_tick: Tick::new(0),
                    transpose: 0,
                    velocity_multiplier: 1.0,
                }],
                register: None,
                automation: vec![],
                fx: None,
                mix_gain_db: None,
                mix_pan: None,
            },
            Layer {
                id: LayerId::new(),
                name: "Lead".to_string(),
                instrument: "lead_synth".to_string(),
                pattern_instances: vec![scorestrata_core::domain::PatternInstance {
                    id: scorestrata_core::ids::PatternInstanceId::new(),
                    pattern_id: lead.id,
                    start_tick: Tick::new(0),
                    transpose: 12,
                    velocity_multiplier: 1.0,
                }],
                register: None,
                automation: vec![],
                fx: None,
                mix_gain_db: None,
                mix_pan: None,
            },
        ],
    };

    Project {
        id: ProjectId::new(),
        name: "Eight_Bar_Loop".to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        brief: Brief {
            raw_text: "Dark melodic techno at 128 BPM in C minor, 8 bars".to_string(),
            genre: Some("melodic_techno".to_string()),
            mood: vec!["dark".to_string()],
            references: Vec::new(),
            delivery_target: Some(DeliveryTarget::Streaming),
            constraints: Vec::new(),
            confidence: 0.9,
        },
        config: ProjectConfig {
            name: "Eight_Bar_Loop".to_string(),
            genre: "melodic_techno".to_string(),
            bpm: 128,
            key: "C minor".to_string(),
            time_signature: TimeSignature::default(),
            duration_bars: 8,
            delivery_target: DeliveryTarget::Streaming,
            mood: vec!["dark".to_string()],
            references: Vec::new(),
        },
        arrangement: Arrangement {
            total_bars: 8,
            time_signature: TimeSignature::default(),
            segments: vec![loop_segment],
            energy_curve: Vec::new(),
        },
        tracks: Vec::new(),
        patterns: vec![drums, bass, lead],
        spectral_reference: None,
        status: ProjectStatus::ConfigApproved,
    }
}

/// Parse the genre, BPM, key, and bar count from a brief with conservative
/// rules (mirrors the MCP config generator's heuristics).
fn parse_brief_hints(text: &str) -> (String, u32, String, u32) {
    let lower = text.to_lowercase();

    let genre = if lower.contains("techno") || lower.contains("melodic house") {
        "melodic_techno"
    } else if lower.contains("house") {
        "house"
    } else if lower.contains("ambient") || lower.contains("drone") {
        "ambient"
    } else if lower.contains("cinematic") || lower.contains("orchestral") {
        "cinematic"
    } else if lower.contains("trance") {
        "trance"
    } else {
        "experimental"
    }
    .to_string();

    let bpm = text
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|t| t.parse::<u32>().ok())
        .find(|n| (60..=180).contains(n))
        .unwrap_or(match genre.as_str() {
            "melodic_techno" => 124,
            "house" => 122,
            "ambient" => 80,
            "cinematic" => 90,
            "trance" => 138,
            _ => 120,
        });

    let key = if lower.contains("minor") || lower.contains("major") {
        extract_key(&lower).unwrap_or_else(|| "C minor".to_string())
    } else {
        "C minor".to_string()
    };

    let bars = text
        .split(|c: char| !c.is_ascii_alphanumeric() && c != ' ')
        .filter_map(|t| {
            let t = t.trim();
            if t.ends_with("bar") || t.ends_with("bars") {
                t.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u32>()
                    .ok()
            } else {
                None
            }
        })
        .next()
        .unwrap_or(8);

    (genre, bpm, key, bars)
}

/// Extract a `X minor` / `X major` key phrase from lowercased text.
fn extract_key(lower: &str) -> Option<String> {
    const ROOTS: &[&str] = &[
        "c#", "db", "d#", "eb", "f#", "gb", "g#", "ab", "a#", "bb", "c", "d", "e", "f", "g", "a",
        "b",
    ];
    for mode in ["minor", "major"] {
        if let Some(idx) = lower.find(mode) {
            let prefix = &lower[..idx];
            let root = prefix
                .split(|c: char| !c.is_ascii_alphabetic() && c != '#')
                .filter(|s| !s.is_empty())
                .last();
            if let Some(root) = root
                && ROOTS.contains(&root)
            {
                let canonical: String = {
                    let mut chars = root.chars();
                    let first = chars.next()?.to_uppercase().to_string();
                    format!("{first}{}", chars.as_str())
                };
                let canonical_mode = if mode == "minor" { "minor" } else { "Major" };
                return Some(format!("{canonical} {canonical_mode}"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_project_is_valid() {
        let project = minimal_project();
        assert_eq!(project.status, ProjectStatus::ConfigApproved);
        assert!(!project.arrangement.segments.is_empty());
    }

    #[test]
    fn planned_projects_cover_full_duration() {
        for project in [melodic_techno_project(), ambient_project(), house_project()] {
            let covered: u32 = project.arrangement.segments.iter().map(|s| s.bars).sum();
            assert_eq!(covered, project.config.duration_bars);
            assert!(!project.patterns.is_empty(), "patterns are planned");
            assert!(
                project
                    .arrangement
                    .segments
                    .iter()
                    .all(|s| !s.layers.is_empty()),
                "every segment has layers"
            );
        }
    }

    #[test]
    fn genre_fixtures_have_correct_keys() {
        assert_eq!(melodic_techno_project().config.bpm, 124);
        assert_eq!(melodic_techno_project().config.key, "C minor");
        assert_eq!(ambient_project().config.bpm, 80);
        assert_eq!(ambient_project().config.key, "A minor");
        assert_eq!(house_project().config.bpm, 122);
    }

    #[test]
    fn eight_bar_loop_is_compilable_shape() {
        let loop_project = eight_bar_loop();
        assert_eq!(loop_project.config.duration_bars, 8);
        assert_eq!(loop_project.arrangement.total_bars, 8);
        assert_eq!(loop_project.patterns.len(), 3, "drums + bass + lead");
        let covered: u32 = loop_project
            .arrangement
            .segments
            .iter()
            .map(|s| s.bars)
            .sum();
        assert_eq!(covered, 8);
        assert_eq!(loop_project.arrangement.segments[0].layers.len(), 3);
    }

    #[test]
    fn drum_pattern_has_backbeat() {
        let drums = drum_pattern_four_on_floor();
        assert_eq!(drums.steps.len(), 16);
        assert!(drums.steps[0].active); // kick on beat 1
        assert!(drums.steps[4].active); // snare on beat 2
        assert!(drums.steps[8].active); // kick on beat 3
        assert!(drums.steps[12].active); // snare on beat 4
    }

    #[test]
    fn bass_pattern_is_in_key() {
        let bass = bass_pattern("C minor");
        let scale = scorestrata_theory::key::Key::parse("C minor")
            .expect("parses")
            .to_scale();
        for note in &bass.notes {
            assert!(scale.contains_note(&note.pitch));
        }
    }

    #[test]
    fn brief_hints_parse_correctly() {
        let (genre, bpm, key, bars) =
            parse_brief_hints("Dark melodic techno at 128 BPM in C minor, 8 bars");
        assert_eq!(genre, "melodic_techno");
        assert_eq!(bpm, 128);
        assert_eq!(key, "C minor");
        assert_eq!(bars, 8);
    }
}
