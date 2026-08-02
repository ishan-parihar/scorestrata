//! Automatic arrangement planning from a project config.
//!
//! Wires the Phase 1 "arrangement planning" step (§28 Step 3) into a real,
//! deterministic algorithm: given a validated [`ProjectConfig`], plan a full
//! arrangement (segments with roles), layers (drums / bass / lead), and
//! in-key patterns (kick/snare/hat steps, root-fifth bass, scale-degree
//! arpeggio) that together cover the configured duration.
//!
//! The planner is fully deterministic — no RNG — so the same config always
//! yields the same plan (per §17 determinism rules).

use scorestrata_core::domain::{
    Arrangement, Duration, Layer, MidiNote, Note, PPQ, Pattern, PatternInstance, ProjectConfig,
    Segment, SegmentRole, Step, Tick, TimeSignature, Velocity,
};
use scorestrata_core::ids::{LayerId, NoteId, PatternId, PatternInstanceId, SegmentId, StepId};
use scorestrata_theory::key::Key;
use scorestrata_theory::scale::Scale;
use sha2::Digest;

/// Deterministic ID generator seeded from the project config.
///
/// Per §17 determinism rules, the same config must always produce the same
/// plan — including its entity IDs — so snapshots and diffs are stable.
struct IdSeq {
    seed: u128,
    counter: u128,
}

impl IdSeq {
    /// Build a seed from the serialized config hash.
    fn from_config(config: &ProjectConfig) -> Self {
        let json = serde_json::to_string(config).unwrap_or_default();
        let mut hasher = sha2::Sha256::new();
        hasher.update(json.as_bytes());
        let digest = hasher.finalize();
        let seed = u128::from_be_bytes(digest[..16].try_into().expect("16-byte slice"));
        Self { seed, counter: 0 }
    }

    /// Next deterministic UUID as a 128-bit value.
    fn next_u128(&mut self) -> u128 {
        // Mix the counter into the seed with a golden-ratio avalanche so
        // consecutive IDs are visually distinct but still reproducible.
        let value = self
            .seed
            .wrapping_add(self.counter.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        self.counter += 1;
        value
    }
}

/// The planned arrangement plus its supporting patterns.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Segments and layers covering the full duration.
    pub arrangement: Arrangement,
    /// Patterns referenced by the layers.
    pub patterns: Vec<Pattern>,
}

/// Build a deterministic arrangement plan from a config.
///
/// Falls back to a neutral major key if the config key cannot be parsed;
/// the caller should have validated the key beforehand.
#[must_use]
pub fn plan_project(config: &ProjectConfig) -> Plan {
    let scale = Key::parse(&config.key)
        .map(|key| key.to_scale())
        .unwrap_or_else(|| Scale {
            root: 0,
            pattern: scorestrata_theory::scale::patterns::major(),
        });

    let mut ids = IdSeq::from_config(config);
    let patterns = build_patterns(&scale, &mut ids);

    let segments = build_segments(config, &patterns, &mut ids);
    let arrangement = Arrangement {
        total_bars: config.duration_bars,
        time_signature: config.time_signature,
        segments,
        energy_curve: Vec::new(),
    };

    Plan {
        arrangement,
        patterns,
    }
}

/// Build the three canonical patterns (drums, bass, lead) in the given scale.
fn build_patterns(scale: &Scale, ids: &mut IdSeq) -> Vec<Pattern> {
    vec![
        drum_pattern(ids),
        bass_pattern(scale, ids),
        lead_pattern(scale, ids),
    ]
}

/// 1-bar 16-step drum pattern: kick on 1 & 9, snare on 5 & 13, hats on odds.
fn drum_pattern(ids: &mut IdSeq) -> Pattern {
    let steps_per_bar: u32 = 16;
    let mut steps = Vec::new();
    for i in 0..steps_per_bar {
        // Kick on beats 0 and 8 (1 and 3 of the bar).
        let kick = i == 0 || i == 8;
        // Snare/clap on beats 4 and 12 (backbeat).
        let snare = i == 4 || i == 12;
        // Open hat on odd sixteenths.
        let hat = i % 2 == 1;
        let active = kick || snare || hat;
        let velocity = if kick {
            110
        } else if snare {
            100
        } else {
            85
        };
        steps.push(Step {
            id: StepId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            index: i,
            active,
            velocity: Velocity::new(velocity),
        });
    }
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Drums_16".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes: Vec::new(),
        steps,
    }
}

/// 1-bar bass pattern: root on the downbeat, fifth on beat 3, octave root
/// on the offbeat of beat 2 — all drawn from the scale.
fn bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let ppq = u64::from(PPQ);
    // Bass sits one octave below the lead (root around MIDI 36 + root pc).
    let low = MidiNote::new(24);
    let high = MidiNote::new(48);
    let tones = scale.notes_in_range(low, high);
    let root = *tones.first().unwrap_or(&MidiNote::new(36));
    let fifth = tones.get(4).copied().unwrap_or(root);
    let octave = *tones.last().unwrap_or(&root);

    let quarter = Duration::quarter();
    let half = Duration::half();

    let notes = vec![
        Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch: root,
            start: Tick::new(0),
            duration: half,
            velocity: Velocity::new(105),
            tied: false,
        },
        Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch: octave,
            start: Tick::new(ppq),
            duration: quarter,
            velocity: Velocity::new(90),
            tied: false,
        },
        Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch: fifth,
            start: Tick::new(ppq * 2),
            duration: half,
            velocity: Velocity::new(100),
            tied: false,
        },
    ];

    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_RootFifth".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// Tick offset for the `idx`-th eighth note within a bar.
fn eighth_tick(idx: usize) -> u64 {
    let eighth = u64::from(PPQ) / 2;
    eighth * u64::try_from(idx).unwrap_or(0)
}

/// 1-bar lead arpeggio: scale degrees 0, 2, 4, 7 as eighth notes.
fn lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let low = MidiNote::new(60);
    let high = MidiNote::new(84);
    let tones = scale.notes_in_range(low, high);
    if tones.is_empty() {
        return Pattern {
            id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            name: "Lead_Empty".to_string(),
            bars: 1,
            time_signature: TimeSignature::default(),
            notes: Vec::new(),
            steps: Vec::new(),
        };
    }

    // Pick scale degrees [0, 2, 4, 7] (root, third, fifth, octave) if present.
    let mut notes = Vec::new();
    for (idx, degree) in [0usize, 2, 4, 7].into_iter().enumerate() {
        let Some(&pitch) = tones.get(degree) else {
            continue;
        };
        notes.push(Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch,
            start: Tick::new(eighth_tick(idx)),
            duration: Duration::eighth(),
            velocity: Velocity::new(95),
            tied: false,
        });
    }

    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_Arp".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// Build segments covering the full duration, with layered pattern instances.
fn build_segments(config: &ProjectConfig, patterns: &[Pattern], ids: &mut IdSeq) -> Vec<Segment> {
    let total = config.duration_bars;
    if total == 0 {
        return Vec::new();
    }

    // Role distribution (in bars) that always sums to `total` and keeps
    // every segment within the validator's 4–64 bar window when possible.
    let roles = if total < 4 {
        // Very short tracks: a single section is the only sane shape.
        vec![(SegmentRole::Chorus, total)]
    } else if total < 8 {
        vec![(SegmentRole::Verse, total)]
    } else if total < 16 {
        // Two sections once there is room for two >= 4-bar segments.
        vec![(SegmentRole::Intro, 4), (SegmentRole::Chorus, total - 4)]
    } else {
        let intro = 4;
        let verse = 4;
        let build = 4;
        let chorus = total - intro - verse - build;
        vec![
            (SegmentRole::Intro, intro),
            (SegmentRole::Verse, verse),
            (SegmentRole::Build, build),
            (SegmentRole::Chorus, chorus),
        ]
    };

    let mut segments = Vec::new();
    let mut start_bar = 0u32;
    for (role, bars) in roles {
        let name = match role {
            SegmentRole::Intro => "Intro",
            SegmentRole::Verse => "Verse",
            SegmentRole::Build => "Build",
            SegmentRole::Chorus => "Chorus",
            SegmentRole::Bridge => "Bridge",
            SegmentRole::Outro => "Outro",
            _ => "Section",
        };
        let layers = build_layers(patterns, role, bars, ids);
        segments.push(Segment {
            id: SegmentId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            name: name.to_string(),
            role,
            start_bar,
            bars,
            layers,
        });
        start_bar += bars;
    }
    segments
}

/// Layer composition per segment role.
///
/// Every segment gets drums + bass; melodic layers are thinned out in the
/// Intro (no lead) and Build (lead only) to create a natural energy ramp.
fn build_layers(
    patterns: &[Pattern],
    role: SegmentRole,
    segment_bars: u32,
    ids: &mut IdSeq,
) -> Vec<Layer> {
    let drum = patterns.first();
    let bass = patterns.get(1);
    let lead = patterns.get(2);

    let mut layers = Vec::new();
    if let Some(p) = drum {
        layers.push(make_layer("Drums", "drum_sampler", p, 0, segment_bars, ids));
    }
    if let Some(p) = bass {
        layers.push(make_layer("Bass", "bass_synth", p, 0, segment_bars, ids));
    }
    match role {
        SegmentRole::Intro | SegmentRole::Outro => {}
        SegmentRole::Verse | SegmentRole::Bridge => {
            if let Some(p) = lead {
                layers.push(make_layer("Lead", "lead_synth", p, 12, segment_bars, ids));
            }
        }
        SegmentRole::Build => {
            if let Some(p) = lead {
                layers.push(make_layer("Lead", "lead_synth", p, 12, segment_bars, ids));
            }
        }
        SegmentRole::Chorus | _ => {
            if let Some(p) = lead {
                layers.push(make_layer("Lead", "lead_synth", p, 12, segment_bars, ids));
            }
            // Chorus doubles the lead an octave up for a denser, brighter
            // drop — a natural energy peak before the build-out.
            if let Some(p) = lead {
                layers.push(make_layer(
                    "Lead Octave",
                    "lead_synth",
                    p,
                    24,
                    segment_bars,
                    ids,
                ));
            }
        }
    }
    layers
}

/// Create a layer repeating the pattern once per bar across the segment.
///
/// A 1-bar pattern placed in an 8-bar segment must repeat 8 times for the
/// loop to actually be 8 bars long — otherwise the compiled timeline would
/// only contain one bar of material.
fn make_layer(
    name: &str,
    instrument: &str,
    pattern: &Pattern,
    transpose: i32,
    segment_bars: u32,
    ids: &mut IdSeq,
) -> Layer {
    let ticks_per_bar = u64::from(PPQ) * u64::from(TimeSignature::default().numerator);
    let repeat_every = u64::from(pattern.bars.max(1)) * ticks_per_bar;
    let instance_count = segment_bars.div_ceil(pattern.bars.max(1));

    let pattern_instances = (0..instance_count)
        .map(|i| PatternInstance {
            id: PatternInstanceId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pattern_id: pattern.id,
            start_tick: Tick::new(repeat_every * u64::from(i)),
            transpose,
            velocity_multiplier: 1.0,
        })
        .collect();

    Layer {
        id: LayerId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: name.to_string(),
        instrument: instrument.to_string(),
        pattern_instances,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::DeliveryTarget;

    fn config(duration_bars: u32) -> ProjectConfig {
        ProjectConfig {
            name: "Plan Test".to_string(),
            genre: "melodic_techno".to_string(),
            bpm: 124,
            key: "C minor".to_string(),
            time_signature: TimeSignature::default(),
            duration_bars,
            delivery_target: DeliveryTarget::Streaming,
            mood: Vec::new(),
            references: Vec::new(),
        }
    }

    #[test]
    fn plans_cover_full_duration() {
        for total in [4, 8, 16, 32, 64] {
            let plan = plan_project(&config(total));
            let covered: u32 = plan.arrangement.segments.iter().map(|s| s.bars).sum();
            assert_eq!(covered, total, "segments must cover {total} bars");
            assert_eq!(plan.arrangement.total_bars, total);
        }
    }

    #[test]
    fn segments_are_contiguous() {
        let plan = plan_project(&config(16));
        let mut start = 0u32;
        for segment in &plan.arrangement.segments {
            assert_eq!(segment.start_bar, start);
            start += segment.bars;
        }
    }

    #[test]
    fn patterns_are_in_key() {
        let plan = plan_project(&config(16));
        let scale = Key::parse("C minor").expect("key parses").to_scale();
        for pattern in &plan.patterns {
            for note in &pattern.notes {
                assert!(
                    scale.contains_note(&note.pitch),
                    "note {} in pattern {} is out of key",
                    note.pitch.0,
                    pattern.name
                );
            }
        }
    }

    #[test]
    fn drums_use_steps_and_no_notes() {
        let plan = plan_project(&config(8));
        let drums = &plan.patterns[0];
        assert_eq!(drums.name, "Drums_16");
        assert_eq!(drums.steps.len(), 16);
        assert!(drums.notes.is_empty());
        // Backbeat: step 4 (snare) and step 0 (kick) active.
        assert!(drums.steps[0].active);
        assert!(drums.steps[4].active);
        assert!(drums.steps[8].active);
    }

    #[test]
    fn planning_is_deterministic() {
        let a = plan_project(&config(16));
        let b = plan_project(&config(16));
        assert_eq!(
            serde_json::to_string(&a.arrangement).expect("serializes"),
            serde_json::to_string(&b.arrangement).expect("serializes")
        );
        assert_eq!(
            serde_json::to_string(&a.patterns).expect("serializes"),
            serde_json::to_string(&b.patterns).expect("serializes")
        );
    }

    #[test]
    fn intro_has_no_lead_layer() {
        let plan = plan_project(&config(16));
        let intro = &plan.arrangement.segments[0];
        assert_eq!(intro.role, SegmentRole::Intro);
        let layer_names: Vec<&str> = intro.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(layer_names, vec!["Drums", "Bass"]);
    }

    #[test]
    fn chorus_is_denser_than_verse() {
        let plan = plan_project(&config(16));
        let chorus = plan
            .arrangement
            .segments
            .iter()
            .find(|s| s.role == SegmentRole::Chorus)
            .expect("chorus segment exists");
        let verse = plan
            .arrangement
            .segments
            .iter()
            .find(|s| s.role == SegmentRole::Verse)
            .expect("verse segment exists");
        assert!(
            chorus.layers.len() > verse.layers.len(),
            "chorus ({} layers) should be denser than verse ({} layers)",
            chorus.layers.len(),
            verse.layers.len()
        );
    }

    #[test]
    fn all_segments_within_validator_bar_window() {
        for total in [8, 12, 16, 32, 64] {
            let plan = plan_project(&config(total));
            for segment in &plan.arrangement.segments {
                assert!(
                    (4..=64).contains(&segment.bars),
                    "segment '{}' has {} bars (outside 4-64) for total {total}",
                    segment.name,
                    segment.bars
                );
            }
        }
    }

    #[test]
    fn empty_duration_yields_empty_plan() {
        let plan = plan_project(&config(0));
        assert!(plan.arrangement.segments.is_empty());
    }
}
