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
    Register, Segment, SegmentRole, Step, Tick, TimeSignature, Velocity,
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
        // The digest is 32 bytes; the first 16 are always available.
        let mut seed_bytes = [0u8; 16];
        seed_bytes.copy_from_slice(&digest[..16]);
        let seed = u128::from_be_bytes(seed_bytes);
        Self { seed, counter: 0 }
    }

    /// Next deterministic UUID as a 128-bit value.
    const fn next_u128(&mut self) -> u128 {
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

/// Genre family driving pattern selection (F10: genre-differentiated content).
///
/// Each family owns its drum kit, bass line, and melodic voice so genres do
/// not compile to identical timelines (R5). S3 added the band families
/// (jazz/hip-hop/dnb/cinematic/pop/funk/blues/synthwave/lo-fi) so a brief
/// like "jazz swing" or "trap" produces a genre-appropriate arrangement,
/// not the electronic baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Genre {
    /// Driving baseline: 4/4 kick, 16th hats, root-fifth bass, arp lead.
    Techno,
    /// 4/4 kick + clap, swung offbeat hats, root-octave bass, stabs + chords.
    House,
    /// Sparse percussion, drone bass, bell tones, evolving pad layer.
    Ambient,
    /// Rock/nu-metal: backbeat drums, palm-muted power-chord guitar riff,
    /// driving root-fifth bass, and a vocal-range lead (per the §1.2
    /// reference-track research and the Linkin Park stress test).
    Rock,
    /// Swing ride cymbal, walking bass, melodic horn/keys line.
    Jazz,
    /// Boom-bap: sparse kick/snare grid, sub-heavy root bass, stab lead.
    HipHop,
    /// Breakbeat drums, rolling bass, ambient pads.
    DnB,
    /// Orchestral: sparse percussion, low drone bass, strings/choir pads.
    Cinematic,
    /// 4/4 kick + claps, catchy root-fifth bass, vocal-range lead, chords.
    Pop,
    /// Syncopated 16th groove: kick/snare grid, popping bass, stab lead.
    Funk,
    /// Shuffle feel: swing hats, walking bass, guitar riff lead.
    Blues,
    /// Driving 8th-note bass, 16th arp lead, dark pads.
    Synthwave,
    /// Mellow swing: soft kick/snare, warm bass, keys/pad bed.
    LoFi,
    /// Any genre the planner does not specialize yet (keeps the baseline).
    Other,
}

/// Detect the genre family from the project config.
fn detect_genre(config: &ProjectConfig) -> Genre {
    let genre = config.genre.to_lowercase();
    if genre.contains("house") {
        Genre::House
    } else if genre.contains("ambient")
        || genre.contains("drone")
        || genre.contains("chill")
        || genre.contains("new age")
    {
        Genre::Ambient
    } else if genre.contains("jazz") || genre.contains("swing") || genre.contains("bossa") {
        Genre::Jazz
    } else if genre.contains("hip")
        || genre.contains("trap")
        || genre.contains("drill")
        || genre.contains("boom")
        || genre.contains("rap")
        || genre.contains("grime")
        || genre.contains("phonk")
    {
        Genre::HipHop
    } else if genre.contains("drum and bass")
        || genre.contains("drum_and_bass")
        || genre.contains("dnb")
        || genre.contains("jungle")
        || genre.contains("breakbeat")
    {
        Genre::DnB
    } else if genre.contains("cinematic")
        || genre.contains("film")
        || genre.contains("orchestral")
        || genre.contains("classical")
        || genre.contains("score")
    {
        Genre::Cinematic
    } else if genre.contains("pop") {
        Genre::Pop
    } else if genre.contains("funk") || genre.contains("soul") || genre.contains("disco") {
        Genre::Funk
    } else if genre.contains("blues") {
        Genre::Blues
    } else if genre.contains("synthwave")
        || genre.contains("retrowave")
        || genre.contains("outrun")
        || genre.contains("vaporwave")
        || genre.contains("80s")
    {
        Genre::Synthwave
    } else if genre.contains("lo-fi") || genre.contains("lofi") || genre.contains("chill hop") {
        Genre::LoFi
    } else if genre.contains("techno") || genre.contains("trance") {
        Genre::Techno
    } else if genre.contains("rock")
        || genre.contains("metal")
        || genre.contains("nu_metal")
        || genre.contains("punk")
    {
        Genre::Rock
    } else {
        Genre::Other
    }
}

/// Build a deterministic arrangement plan from a config.
///
/// Falls back to a neutral major key if the config key cannot be parsed;
/// the caller should have validated the key beforehand.
#[must_use]
pub fn plan_project(config: &ProjectConfig) -> Plan {
    let scale = Key::parse(&config.key).map_or_else(
        || Scale {
            root: 0,
            pattern: scorestrata_theory::scale::patterns::major(),
        },
        |key| key.to_scale(),
    );

    let genre = detect_genre(config);
    let mut ids = IdSeq::from_config(config);
    let patterns = build_patterns(&scale, genre, &mut ids);

    let segments = build_segments(config, &patterns, genre, &mut ids);
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

/// Build the canonical patterns for a genre (drums, bass, lead, and pads
/// where the genre calls for a pad/drone layer) in the given scale.
fn build_patterns(scale: &Scale, genre: Genre, ids: &mut IdSeq) -> Vec<Pattern> {
    match genre {
        Genre::House => vec![
            house_drum_pattern(ids),
            house_bass_pattern(scale, ids),
            house_lead_pattern(scale, ids),
            house_pad_pattern(scale, ids),
        ],
        Genre::Ambient => vec![
            ambient_drum_pattern(ids),
            ambient_bass_pattern(scale, ids),
            ambient_lead_pattern(scale, ids),
            ambient_pad_pattern(scale, ids),
        ],
        // Techno and un-specialized genres keep the original baseline
        // patterns (names and structure unchanged, so the golden gates and
        // pattern-shape tests stay stable).
        Genre::Rock => vec![
            rock_drum_pattern(ids),
            rock_bass_pattern(scale, ids),
            rock_guitar_pattern(scale, ids),
            rock_lead_pattern(scale, ids),
        ],
        Genre::Jazz => vec![
            jazz_drum_pattern(ids),
            jazz_bass_pattern(scale, ids),
            jazz_lead_pattern(scale, ids),
            jazz_pad_pattern(scale, ids),
        ],
        Genre::HipHop => vec![
            hiphop_drum_pattern(ids),
            hiphop_bass_pattern(scale, ids),
            hiphop_lead_pattern(scale, ids),
            hiphop_pad_pattern(scale, ids),
        ],
        Genre::DnB => vec![
            dnb_drum_pattern(ids),
            dnb_bass_pattern(scale, ids),
            dnb_lead_pattern(scale, ids),
            dnb_pad_pattern(scale, ids),
        ],
        Genre::Cinematic => vec![
            cinematic_drum_pattern(ids),
            cinematic_bass_pattern(scale, ids),
            cinematic_lead_pattern(scale, ids),
            cinematic_pad_pattern(scale, ids),
        ],
        Genre::Pop => vec![
            pop_drum_pattern(ids),
            pop_bass_pattern(scale, ids),
            pop_lead_pattern(scale, ids),
            pop_pad_pattern(scale, ids),
        ],
        Genre::Funk => vec![
            funk_drum_pattern(ids),
            funk_bass_pattern(scale, ids),
            funk_lead_pattern(scale, ids),
            funk_pad_pattern(scale, ids),
        ],
        Genre::Blues => vec![
            blues_drum_pattern(ids),
            blues_bass_pattern(scale, ids),
            blues_lead_pattern(scale, ids),
            blues_pad_pattern(scale, ids),
        ],
        Genre::Synthwave => vec![
            synthwave_drum_pattern(ids),
            synthwave_bass_pattern(scale, ids),
            synthwave_lead_pattern(scale, ids),
            synthwave_pad_pattern(scale, ids),
        ],
        Genre::LoFi => vec![
            lofi_drum_pattern(ids),
            lofi_bass_pattern(scale, ids),
            lofi_lead_pattern(scale, ids),
            lofi_pad_pattern(scale, ids),
        ],
        Genre::Techno | Genre::Other => vec![
            drum_pattern(ids),
            bass_pattern(scale, ids),
            lead_pattern(scale, ids),
        ],
    }
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

/// The absolute pitch classes of the root, third, and fifth of a scale.
///
/// Computed from the interval pattern so it is correct for **any** root key:
/// the third is the third scale tone, and the fifth is a perfect fifth above
/// the root (present in every scale the planner supports). Indexing a note
/// list by position is only root-aligned for C-rooted keys.
fn chord_tone_pcs(scale: &Scale) -> [u8; 3] {
    // Distinct pitch classes of the scale relative to its root.
    let mut rel = vec![0u8];
    let mut acc = 0u8;
    for &interval in &scale.pattern.intervals {
        acc = (acc + interval) % 12;
        if !rel.contains(&acc) {
            rel.push(acc);
        }
    }
    let third_rel = rel.get(2).copied().unwrap_or(0);
    [
        scale.root % 12,
        (scale.root + third_rel) % 12,
        (scale.root + 7) % 12,
    ]
}

/// 1-bar bass pattern: root on the downbeat, fifth on beat 3, octave root
/// on the offbeat of beat 2 — all drawn from the scale.
fn bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let ppq = u64::from(PPQ);
    // Bass sits one octave below the lead (root around MIDI 36 + root pc).
    let low = MidiNote::new(24);
    let high = MidiNote::new(48);
    let tones = scale.notes_in_range(low, high);
    let default_root = MidiNote::new(36);
    let [root_pc, _, fifth_pc] = chord_tone_pcs(scale);
    let root = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == root_pc)
        .unwrap_or(default_root);
    let fifth = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == fifth_pc && n.0 > root.0)
        .unwrap_or(root);
    let octave = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == root_pc && n.0 > root.0)
        .unwrap_or(root);

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

    // Root-anchored chord tones (root, third, fifth, octave) so the arpeggio
    // spells the tonic triad for any key — not just C-rooted ones.
    let [root_pc, third_pc, fifth_pc] = chord_tone_pcs(scale);
    let root = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == root_pc)
        .unwrap_or(tones[0]);
    let third = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == third_pc && n.0 > root.0)
        .unwrap_or(root);
    let fifth = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == fifth_pc && n.0 > root.0)
        .unwrap_or(root);
    let octave = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == root_pc && n.0 > root.0)
        .unwrap_or(root);

    let chord = [root, third, fifth, octave];
    let mut notes = Vec::new();
    for (idx, pitch) in chord.into_iter().enumerate() {
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

/// Chord tones (root, third, fifth, octave) of the scale in a MIDI range.
///
/// Shared by the house/ambient pattern builders so every genre spells the
/// tonic triad in-key for any root, not just C-rooted keys.
fn chord_tones_in_range(scale: &Scale, low: u8, high: u8) -> [MidiNote; 4] {
    let tones = scale.notes_in_range(MidiNote::new(low), MidiNote::new(high));
    let fallback = MidiNote::new(60);
    if tones.is_empty() {
        return [fallback; 4];
    }
    let [root_pc, third_pc, fifth_pc] = chord_tone_pcs(scale);
    let root = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == root_pc)
        .unwrap_or(tones[0]);
    let third = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == third_pc && n.0 > root.0)
        .unwrap_or(root);
    let fifth = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == fifth_pc && n.0 > root.0)
        .unwrap_or(root);
    let octave = tones
        .iter()
        .copied()
        .find(|n| n.pitch_class() == root_pc && n.0 > root.0)
        .unwrap_or(root);
    [root, third, fifth, octave]
}

/// 1-bar house drum pattern: kick on 1 & 3, clap on 2 & 4, swung offbeat hats.
///
/// The swung offbeat eighths (steps 2/6/10/14 with shuffle-weighted
/// velocities) are what separate house's groove from techno's constant
/// 16th-note hat stream.
fn house_drum_pattern(ids: &mut IdSeq) -> Pattern {
    let mut steps = Vec::new();
    for i in 0..16u32 {
        let (active, velocity) = match i {
            0 | 8 => (true, 110),  // kick on 1 & 3
            4 | 12 => (true, 100), // clap on 2 & 4
            2 | 10 => (true, 92),  // swung offbeat hat (accented)
            6 | 14 => (true, 72),  // lighter shuffle hat
            _ => (false, 0),
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
        name: "Drums_House_16".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes: Vec::new(),
        steps,
    }
}

/// 1-bar house bass: pumping root-octave eighth-note groove.
///
/// The root-octave alternation on every offbeat is the classic house pump
/// (per §5 the house bass is a root-octave groove, not a root-fifth line).
fn house_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, fifth, octave] = chord_tones_in_range(scale, 24, 48);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 100, ids),
        note(octave, eighth, 85, ids),
        note(root, eighth * 2, 95, ids),
        note(octave, eighth * 3, 82, ids),
        note(root, eighth * 4, 90, ids),
        note(fifth, eighth * 5, 78, ids),
        note(root, eighth * 6, 88, ids),
        note(octave, eighth * 7, 76, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_OctaveGroove".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar house lead: plucky chord-tone stabs on the offbeat eighths.
fn house_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    // Stabs land on the "and" of each beat for an off-grid, plucky feel.
    let notes = vec![
        note(third, eighth, 95, ids),
        note(root, eighth * 3, 92, ids),
        note(fifth, eighth * 5, 90, ids),
        note(third, eighth * 7, 86, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_Stabs".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar house pads: two sustained filtered chord hits per bar.
fn house_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let half = u64::from(PPQ) * 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::half(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 64, ids),
        note(third, 0, 58, ids),
        note(fifth, 0, 55, ids),
        note(root, half, 62, ids),
        note(third, half, 56, ids),
        note(fifth, half, 52, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_Chords".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar ambient drums: very sparse — a soft kick on 1 and one soft hit.
fn ambient_drum_pattern(ids: &mut IdSeq) -> Pattern {
    let mut steps = Vec::new();
    for i in 0..16u32 {
        let (active, velocity) = match i {
            0 => (true, 105), // soft kick on the downbeat
            8 => (true, 50),  // one soft hit mid-bar
            _ => (false, 0),
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
        name: "Drums_Ambient_16".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes: Vec::new(),
        steps,
    }
}

/// 1-bar ambient bass: a single slow root drone across the whole bar.
fn ambient_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, _, _] = chord_tones_in_range(scale, 24, 48);
    let notes = vec![Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch: root,
        start: Tick::new(0),
        duration: Duration::whole(),
        velocity: Velocity::new(85),
        tied: false,
    }];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_Drone".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar ambient lead: sparse, long-sustained bell tones.
fn ambient_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let half = u64::from(PPQ) * 2;
    let notes = vec![
        Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch: root,
            start: Tick::new(0),
            duration: Duration::half(),
            velocity: Velocity::new(80),
            tied: false,
        },
        Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch: fifth,
            start: Tick::new(half),
            duration: Duration::half(),
            velocity: Velocity::new(72),
            tied: false,
        },
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_Bells".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar ambient pad drone: long overlapping chord tones.
///
/// The two root notes overlap at the same pitch by design — ambient pads
/// sustain across strikes — which is also the exact one same-pitch overlap
/// the ambient golden's `timeline.note_overlap` metric expects.
fn ambient_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let quarter = u64::from(PPQ);
    let half = quarter * 2;
    let note =
        |pitch: MidiNote, start: u64, duration: Duration, velocity: u8, ids: &mut IdSeq| Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch,
            start: Tick::new(start),
            duration,
            velocity: Velocity::new(velocity),
            tied: false,
        };
    let notes = vec![
        note(root, 0, Duration::half(), 70, ids),
        // Same-pitch restrike over the first root: the drone overlap.
        note(root, quarter, Duration::quarter(), 62, ids),
        note(third, 0, Duration::half(), 58, ids),
        note(fifth, half, Duration::quarter(), 55, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_Drone".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// Build segments covering the full duration, with layered pattern instances.
/// 1-bar rock drums: driving backbeat with a busier hi-hat.
///
/// Kick on 1 and 3, snare on 2 and 4 (the rock backbeat — §1.2 reference
/// research), with eighth-note hats on the beat and offbeat for drive.
fn rock_drum_pattern(ids: &mut IdSeq) -> Pattern {
    let mut steps = Vec::new();
    for i in 0..16u32 {
        // Kick on 1 & 3, snare backbeat on 2 & 4, and hats riding the
        // offbeat eighths (the on-beat eighths are covered by the kick and
        // snare hits themselves).
        let (active, velocity) = match i {
            0 | 8 => (true, 115),
            4 | 12 => (true, 108),
            2 | 6 | 10 | 14 => (true, 88),
            _ => (false, 0),
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
        name: "Drums_Rock_Backbeat".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes: Vec::new(),
        steps,
    }
}

/// 1-bar rock bass: driving root-fifth eighth-note line with a pickup
/// accent, sitting low (octaves 1–2 per the §1.3 register map).
fn rock_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, fifth, _] = chord_tones_in_range(scale, 12, 36);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    // Driving eighths with the fifth on beat 3 and an offbeat pickup.
    let notes = vec![
        note(root, 0, 104, ids),
        note(root, eighth, 92, ids),
        note(root, eighth * 2, 100, ids),
        note(fifth, eighth * 3, 90, ids),
        note(root, eighth * 4, 102, ids),
        note(root, eighth * 5, 88, ids),
        note(root, eighth * 6, 96, ids),
        note(fifth, eighth * 7, 84, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_Rock_Drive".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar rock rhythm guitar: palm-muted power-chord riff on the offbeat
/// eighth grid (the classic nu-metal chug).
///
/// Power chords are root + fifth, voiced in the guitar register (octaves
/// 2–4, §1.3) and heavily compressed at the mix stage via the rock guitar
/// FX chain.
fn rock_guitar_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    // Octaves 2–4 = MIDI 36–59 (the §1.3 guitar band).
    let [root, _, fifth, _] = chord_tones_in_range(scale, 36, 60);
    let eighth = u64::from(PPQ) / 2;
    let note =
        |pitch: MidiNote, start: u64, duration: Duration, velocity: u8, ids: &mut IdSeq| Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch,
            start: Tick::new(start),
            duration,
            velocity: Velocity::new(velocity),
            tied: false,
        };
    let chug = Duration::sixteenth();
    let mut notes = Vec::new();
    // Root-fifth dyads chugging on the offbeat eighths.
    for (i, tick) in [1u64, 3, 5, 7, 9, 11, 13, 15].into_iter().enumerate() {
        let velocity = if i % 4 == 3 { 100 } else { 92 };
        notes.push(note(root, tick * eighth, chug, velocity, ids));
        notes.push(note(fifth, tick * eighth, chug, velocity - 4, ids));
    }
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Guitar_PowerChord_Chug".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar rock lead: a vocal-range line (octaves 4–6) with held notes on
/// the beat and a pickup, in the §1.3 lead register.
fn rock_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let quarter = u64::from(PPQ);
    let note =
        |pitch: MidiNote, start: u64, duration: Duration, velocity: u8, ids: &mut IdSeq| Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch,
            start: Tick::new(start),
            duration,
            velocity: Velocity::new(velocity),
            tied: false,
        };
    let notes = vec![
        note(third, 0, Duration::quarter(), 100, ids),
        note(fifth, quarter, Duration::eighth(), 92, ids),
        note(third, quarter * 2, Duration::half(), 96, ids),
        note(
            root,
            quarter * 3 + u64::from(PPQ) / 4,
            Duration::eighth(),
            88,
            ids,
        ),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_VocalRange".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// Build a 16-step drum pattern from a per-step `(active, velocity)`
/// decision table (shared by every drum-family builder).
fn drum_steps(ids: &mut IdSeq, name: &str, f: impl Fn(u32) -> (bool, u8)) -> Pattern {
    let steps: Vec<Step> = (0..16u32)
        .map(|i| {
            let (active, velocity) = f(i);
            Step {
                id: StepId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
                index: i,
                active,
                velocity: Velocity::new(velocity),
            }
        })
        .collect();
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: name.to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes: Vec::new(),
        steps,
    }
}

/// 1-bar jazz drums: ride swing on the eighths, backbeat snare, sparse kick.
fn jazz_drum_pattern(ids: &mut IdSeq) -> Pattern {
    drum_steps(ids, "Drums_Jazz_Swing", |i| match i {
        0 => (true, 100),      // kick on 1
        3 | 11 => (true, 96),  // ride + ghost snare swings
        4 | 12 => (true, 104), // snare backbeat on 2 & 4
        6 | 14 => (true, 88),  // ride offbeats (swung)
        8 => (true, 90),       // light kick on 3
        _ => (false, 0),
    })
}

/// 1-bar jazz bass: walking root-fifth-quarters with an octave pickup.
fn jazz_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, octave] = chord_tones_in_range(scale, 24, 48);
    let quarter = u64::from(PPQ);
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::quarter(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 100, ids),
        note(third, quarter, 88, ids),
        note(fifth, quarter * 2, 96, ids),
        note(octave, quarter * 3, 90, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_Walking".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar jazz lead: a horn-like melodic line in the mid register.
fn jazz_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let quarter = u64::from(PPQ);
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(third, 0, 98, ids),
        note(fifth, quarter, 92, ids),
        note(third, quarter * 2, 96, ids),
        note(root, quarter * 3, 88, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_HornLine".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar jazz pads: warm chord voicings, two per bar.
fn jazz_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let half = u64::from(PPQ) * 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::half(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 66, ids),
        note(third, 0, 60, ids),
        note(fifth, 0, 56, ids),
        note(root, half, 62, ids),
        note(fifth, half, 54, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_JazzVoicings".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar hip-hop drums: boom-bap grid with swung hats.
fn hiphop_drum_pattern(ids: &mut IdSeq) -> Pattern {
    drum_steps(ids, "Drums_BoomBap", |i| match i {
        0 | 6 => (true, 108),  // kick on 1 and the "and" of 2
        4 | 12 => (true, 104), // snare backbeat on 2 & 4
        10 | 14 => (true, 86), // swung ghost hats
        _ => (false, 0),
    })
}

/// 1-bar hip-hop bass: sub-heavy root with an octave drop on the offbeat.
fn hiphop_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, _, octave] = chord_tones_in_range(scale, 24, 48);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 102, ids),
        note(root, eighth, 90, ids),
        note(octave, eighth * 4, 92, ids),
        note(root, eighth * 6, 96, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_SubGroove".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar hip-hop lead: sparse sample-style stabs.
fn hiphop_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, eighth * 3, 94, ids),
        note(third, eighth * 7, 90, ids),
        note(fifth, eighth * 11, 86, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_StabSamples".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar hip-hop pads: dark sustained chords.
fn hiphop_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let note = |pitch: MidiNote, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(0),
        duration: Duration::whole(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 60, ids),
        note(third, 54, ids),
        note(fifth, 50, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_DarkChords".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar drum-and-bass drums: breakbeat syncopation with fast hats.
fn dnb_drum_pattern(ids: &mut IdSeq) -> Pattern {
    drum_steps(ids, "Drums_Breakbeat", |i| match i {
        0 | 6 | 14 => (true, 106),                      // kick syncopation
        4 | 12 => (true, 102),                          // snare on the 2 & 4 break
        1 | 3 | 5 | 7 | 9 | 11 | 13 | 15 => (true, 88), // 16th hat stream
        _ => (false, 0),
    })
}

/// 1-bar `DnB` bass: rolling root-fifth eighths with a sub feel.
fn dnb_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, fifth, _] = chord_tones_in_range(scale, 24, 48);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 100, ids),
        note(fifth, eighth, 92, ids),
        note(root, eighth * 2, 98, ids),
        note(fifth, eighth * 3, 90, ids),
        note(root, eighth * 4, 96, ids),
        note(fifth, eighth * 5, 88, ids),
        note(root, eighth * 6, 94, ids),
        note(fifth, eighth * 7, 86, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_Rolling".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar `DnB` lead: airy stabs over the break.
fn dnb_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [_root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(third, eighth * 2, 94, ids),
        note(fifth, eighth * 6, 90, ids),
        note(third, eighth * 10, 92, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_AiryStabs".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar `DnB` pads: deep sustained chords.
fn dnb_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let note = |pitch: MidiNote, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(0),
        duration: Duration::whole(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 62, ids),
        note(third, 56, ids),
        note(fifth, 52, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_Deep".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar cinematic drums: sparse timpani hits, not a beat.
fn cinematic_drum_pattern(ids: &mut IdSeq) -> Pattern {
    drum_steps(ids, "Drums_Timpani", |i| match i {
        0 => (true, 104),
        8 => (true, 86),
        _ => (false, 0),
    })
}

/// 1-bar cinematic bass: a low root drone with a fifth swell.
fn cinematic_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, fifth, _] = chord_tones_in_range(scale, 24, 48);
    let half = u64::from(PPQ) * 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::half(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![note(root, 0, 90, ids), note(fifth, half, 78, ids)];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_DroneFifth".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar cinematic lead: long strings/choir lines.
fn cinematic_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [_root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let half = u64::from(PPQ) * 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::half(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![note(third, 0, 88, ids), note(fifth, half, 80, ids)];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_StringsLine".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar cinematic pads: big orchestral chord swells.
fn cinematic_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let note = |pitch: MidiNote, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(0),
        duration: Duration::whole(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 64, ids),
        note(third, 58, ids),
        note(fifth, 54, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_Orchestral".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar pop drums: 4/4 kick, claps on 2 & 4, 8th hats.
fn pop_drum_pattern(ids: &mut IdSeq) -> Pattern {
    drum_steps(ids, "Drums_Pop", |i| match i {
        0 | 8 => (true, 110),
        4 | 12 => (true, 104),         // clap backbeat
        2 | 6 | 10 | 14 => (true, 88), // offbeat hats
        _ => (false, 0),
    })
}

/// 1-bar pop bass: catchy root-fifth with an octave pickup.
fn pop_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, fifth, octave] = chord_tones_in_range(scale, 24, 48);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 102, ids),
        note(root, eighth, 90, ids),
        note(fifth, eighth * 2, 96, ids),
        note(octave, eighth * 3, 86, ids),
        note(root, eighth * 4, 98, ids),
        note(root, eighth * 5, 88, ids),
        note(fifth, eighth * 6, 92, ids),
        note(octave, eighth * 7, 84, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_PopGroove".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar pop lead: a singable vocal-range hook.
fn pop_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let eighth = u64::from(PPQ) / 2;
    let note =
        |pitch: MidiNote, start: u64, duration: Duration, velocity: u8, ids: &mut IdSeq| Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch,
            start: Tick::new(start),
            duration,
            velocity: Velocity::new(velocity),
            tied: false,
        };
    let notes = vec![
        note(third, 0, Duration::quarter(), 100, ids),
        note(root, eighth * 3, Duration::eighth(), 92, ids),
        note(fifth, eighth * 5, Duration::eighth(), 96, ids),
        note(third, eighth * 7, Duration::quarter(), 90, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_VocalHook".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar pop pads: bright chord stabs.
fn pop_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let half = u64::from(PPQ) * 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::half(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 64, ids),
        note(third, 0, 58, ids),
        note(fifth, 0, 54, ids),
        note(root, half, 60, ids),
        note(fifth, half, 52, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_PopChords".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar funk drums: syncopated 16th grid with ghost notes.
fn funk_drum_pattern(ids: &mut IdSeq) -> Pattern {
    drum_steps(ids, "Drums_Funk", |i| match i {
        0 => (true, 108),
        4 | 12 => (true, 104), // snare backbeat
        6 => (true, 92),       // kick pickup on "and" of 2
        10 | 14 => (true, 84), // ghost hat
        _ => (false, 0),
    })
}

/// 1-bar funk bass: popping syncopated 16th line.
fn funk_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, fifth, octave] = chord_tones_in_range(scale, 24, 48);
    let sixteenth = u64::from(PPQ) / 4;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::sixteenth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 102, ids),
        note(octave, sixteenth * 2, 90, ids),
        note(root, sixteenth * 6, 96, ids),
        note(fifth, sixteenth * 8, 92, ids),
        note(root, sixteenth * 10, 88, ids),
        note(octave, sixteenth * 12, 94, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_FunkPop".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar funk lead: offbeat horn-stab riff.
fn funk_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [_root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let sixteenth = u64::from(PPQ) / 4;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(third, sixteenth * 4, 96, ids),
        note(fifth, sixteenth * 10, 92, ids),
        note(third, sixteenth * 14, 88, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_HornStabs".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar funk pads: tight chord hits.
fn funk_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let quarter = u64::from(PPQ);
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::quarter(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, quarter * 3, 66, ids),
        note(third, quarter * 3, 60, ids),
        note(fifth, quarter * 3, 54, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_FunkHits".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar blues drums: shuffle with swing hats.
fn blues_drum_pattern(ids: &mut IdSeq) -> Pattern {
    drum_steps(ids, "Drums_Shuffle", |i| match i {
        0 | 8 => (true, 104),
        4 | 12 => (true, 102),         // snare backbeat
        2 | 6 | 10 | 14 => (true, 90), // shuffle triplet hats
        _ => (false, 0),
    })
}

/// 1-bar blues bass: walking shuffle line.
fn blues_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 24, 48);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 0, 100, ids),
        note(third, eighth, 90, ids),
        note(fifth, eighth * 2, 96, ids),
        note(third, eighth * 3, 88, ids),
        note(root, eighth * 4, 98, ids),
        note(fifth, eighth * 5, 88, ids),
        note(third, eighth * 6, 92, ids),
        note(root, eighth * 7, 84, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_ShuffleWalk".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar blues lead: a wailing guitar-style riff.
fn blues_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let eighth = u64::from(PPQ) / 2;
    let note =
        |pitch: MidiNote, start: u64, duration: Duration, velocity: u8, ids: &mut IdSeq| Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch,
            start: Tick::new(start),
            duration,
            velocity: Velocity::new(velocity),
            tied: false,
        };
    let notes = vec![
        note(third, 0, Duration::quarter(), 96, ids),
        note(fifth, eighth * 2, Duration::eighth(), 92, ids),
        note(third, eighth * 4, Duration::quarter(), 90, ids),
        note(root, eighth * 6, Duration::eighth(), 86, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_BluesRiff".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar blues pads: smoky sustained chords.
fn blues_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let note = |pitch: MidiNote, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(0),
        duration: Duration::whole(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 60, ids),
        note(third, 54, ids),
        note(fifth, 50, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_Smoky".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar synthwave drums: 4/4 kick with 16th hats and clap.
fn synthwave_drum_pattern(ids: &mut IdSeq) -> Pattern {
    drum_steps(ids, "Drums_Synthwave", |i| match i {
        0 | 8 => (true, 112),
        4 | 12 => (true, 106),
        _ if i % 2 == 1 => (true, 90), // driving 16th hats
        _ => (false, 0),
    })
}

/// 1-bar synthwave bass: pulsing 8th-note octave drive.
fn synthwave_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, _, octave] = chord_tones_in_range(scale, 24, 48);
    let eighth = u64::from(PPQ) / 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::eighth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = (0..8)
        .map(|i| {
            let pitch = if i % 2 == 0 { root } else { octave };
            note(pitch, eighth * i, if i % 2 == 0 { 100 } else { 88 }, ids)
        })
        .collect();
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_Pulsing8th".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar synthwave lead: 16th arpeggio.
fn synthwave_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, octave] = chord_tones_in_range(scale, 60, 84);
    let sixteenth = u64::from(PPQ) / 4;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::sixteenth(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let pattern_notes = [root, third, fifth, octave, fifth, third];
    let notes = pattern_notes
        .iter()
        .enumerate()
        .map(|(i, pitch)| note(*pitch, sixteenth * u64::try_from(i).unwrap_or(0), 96, ids))
        .collect();
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_Arp16".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar synthwave pads: dark retro chords.
fn synthwave_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let note = |pitch: MidiNote, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(0),
        duration: Duration::whole(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 60, ids),
        note(third, 54, ids),
        note(fifth, 50, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_Retro".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar lo-fi drums: mellow swing with soft hits.
fn lofi_drum_pattern(ids: &mut IdSeq) -> Pattern {
    drum_steps(ids, "Drums_LofiSwing", |i| match i {
        0 | 8 => (true, 96),
        4 | 12 => (true, 92),
        2 | 6 | 10 | 14 => (true, 78),
        _ => (false, 0),
    })
}

/// 1-bar lo-fi bass: warm root-octave half notes.
fn lofi_bass_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, _, _, octave] = chord_tones_in_range(scale, 24, 48);
    let half = u64::from(PPQ) * 2;
    let note = |pitch: MidiNote, start: u64, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(start),
        duration: Duration::half(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![note(root, 0, 86, ids), note(octave, half, 74, ids)];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Bass_WarmDrone".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar lo-fi lead: mellow keys melody.
fn lofi_lead_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 60, 84);
    let quarter = u64::from(PPQ);
    let note =
        |pitch: MidiNote, start: u64, duration: Duration, velocity: u8, ids: &mut IdSeq| Note {
            id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
            pitch,
            start: Tick::new(start),
            duration,
            velocity: Velocity::new(velocity),
            tied: false,
        };
    let notes = vec![
        note(third, 0, Duration::half(), 84, ids),
        note(fifth, quarter * 2, Duration::quarter(), 78, ids),
        note(root, quarter * 3, Duration::quarter(), 74, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Lead_KeysMellow".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

/// 1-bar lo-fi pads: warm vinyl chord bed.
fn lofi_pad_pattern(scale: &Scale, ids: &mut IdSeq) -> Pattern {
    let [root, third, fifth, _] = chord_tones_in_range(scale, 48, 72);
    let note = |pitch: MidiNote, velocity: u8, ids: &mut IdSeq| Note {
        id: NoteId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        pitch,
        start: Tick::new(0),
        duration: Duration::whole(),
        velocity: Velocity::new(velocity),
        tied: false,
    };
    let notes = vec![
        note(root, 58, ids),
        note(third, 52, ids),
        note(fifth, 48, ids),
    ];
    Pattern {
        id: PatternId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: "Pads_WarmVinyl".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes,
        steps: Vec::new(),
    }
}

fn build_segments(
    config: &ProjectConfig,
    patterns: &[Pattern],
    genre: Genre,
    ids: &mut IdSeq,
) -> Vec<Segment> {
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
        let layers = build_layers(patterns, role, bars, genre, ids);
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

/// Layer composition per segment role, per genre.
///
/// Every genre keeps drums + bass in every segment; melodic and pad layers
/// are then thinned out in the Intro/Outro to create a natural energy ramp.
fn build_layers(
    patterns: &[Pattern],
    role: SegmentRole,
    segment_bars: u32,
    genre: Genre,
    ids: &mut IdSeq,
) -> Vec<Layer> {
    match genre {
        Genre::House => house_layers(patterns, role, segment_bars, ids),
        Genre::Ambient => ambient_layers(patterns, role, segment_bars, ids),
        Genre::Rock => rock_layers(patterns, role, segment_bars, ids),
        Genre::Jazz | Genre::Pop | Genre::Funk | Genre::Blues => band_layers(
            patterns,
            role,
            segment_bars,
            ids,
            "Lead",
            "lead_synth",
            "Pads",
            "pad_synth",
            false,
        ),
        Genre::HipHop | Genre::DnB | Genre::Cinematic | Genre::Synthwave | Genre::LoFi => {
            band_layers(
                patterns,
                role,
                segment_bars,
                ids,
                "Lead",
                "lead_synth",
                "Pads",
                "pad_synth",
                true,
            )
        }
        Genre::Techno | Genre::Other => techno_layers(patterns, role, segment_bars, ids),
    }
}

/// Shared band-layer composition for the drum/bass/lead/pads families
/// (jazz, hip-hop, dnb, cinematic, pop, funk, blues, synthwave, lo-fi).
///
/// Drums + bass always; lead joins in the mid/peak sections; pads either
/// ride everywhere (`pads_always`, for pad-heavy families) or join the
/// chorus only.
#[allow(clippy::too_many_arguments)]
fn band_layers(
    patterns: &[Pattern],
    role: SegmentRole,
    segment_bars: u32,
    ids: &mut IdSeq,
    lead_name: &str,
    lead_instrument: &str,
    pad_name: &str,
    pad_instrument: &str,
    pads_always: bool,
) -> Vec<Layer> {
    let drum = patterns.first();
    let bass = patterns.get(1);
    let lead = patterns.get(2);
    let pads = patterns.get(3);

    let mut layers = Vec::new();
    if let Some(p) = drum {
        layers.push(make_layer("Drums", "drum_sampler", p, 0, segment_bars, ids));
    }
    if let Some(p) = bass {
        layers.push(make_layer("Bass", "bass_synth", p, 0, segment_bars, ids));
    }
    if pads_always {
        // Pad-heavy families (hip-hop, dnb, cinematic, synthwave, lo-fi)
        // carry the pad bed in every segment — including intro/outro.
        if let Some(p) = pads {
            layers.push(make_layer(
                pad_name,
                pad_instrument,
                p,
                0,
                segment_bars,
                ids,
            ));
        }
    }
    match role {
        SegmentRole::Intro | SegmentRole::Outro => {}
        SegmentRole::Verse | SegmentRole::Build | SegmentRole::Breakdown | SegmentRole::Bridge => {
            if let Some(p) = lead {
                layers.push(make_layer(
                    lead_name,
                    lead_instrument,
                    p,
                    0,
                    segment_bars,
                    ids,
                ));
            }
        }
        SegmentRole::Chorus | SegmentRole::Custom => {
            if let Some(p) = lead {
                layers.push(make_layer(
                    lead_name,
                    lead_instrument,
                    p,
                    0,
                    segment_bars,
                    ids,
                ));
            }
            if let Some(p) = pads {
                layers.push(make_layer(
                    pad_name,
                    pad_instrument,
                    p,
                    0,
                    segment_bars,
                    ids,
                ));
            }
        }
    }
    layers
}

/// Baseline techno layer composition (unchanged from the original planner).
///
/// Intro/Outro: drums + bass. Mid sections add the lead; the chorus peaks
/// with a doubled lead an octave up.
fn techno_layers(
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
        // Intros and outros stay sparse: rhythm section only.
        SegmentRole::Intro | SegmentRole::Outro => {}
        // Mid sections carry the lead.
        SegmentRole::Verse | SegmentRole::Bridge | SegmentRole::Build | SegmentRole::Breakdown => {
            if let Some(p) = lead {
                layers.push(make_layer("Lead", "lead_synth", p, 12, segment_bars, ids));
            }
        }
        // Chorus (and custom sections) peak with a doubled lead an octave
        // up — a denser, brighter drop before the build-out.
        SegmentRole::Chorus | SegmentRole::Custom => {
            if let Some(p) = lead {
                layers.push(make_layer("Lead", "lead_synth", p, 12, segment_bars, ids));
            }
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

/// Rock/nu-metal layer composition: the full band in the chorus, guitar +
/// bass + drums in the verse, and the stripped rhythm section in intro/
/// outro — the classic verse→chorus energy ramp (Linkin Park stress test).
fn rock_layers(
    patterns: &[Pattern],
    role: SegmentRole,
    segment_bars: u32,
    ids: &mut IdSeq,
) -> Vec<Layer> {
    let drum = patterns.first();
    let bass = patterns.get(1);
    let guitar = patterns.get(2);
    let lead = patterns.get(3);

    let mut layers = Vec::new();
    if let Some(p) = drum {
        layers.push(make_layer("Drums", "drum_sampler", p, 0, segment_bars, ids));
    }
    if let Some(p) = bass {
        layers.push(make_layer("Bass", "bass_synth", p, 0, segment_bars, ids));
    }
    match role {
        // Intro/outro: rhythm section only — the band builds in.
        SegmentRole::Intro | SegmentRole::Outro => {}
        // Verse: add the chugging power-chord guitar riff.
        SegmentRole::Verse | SegmentRole::Build | SegmentRole::Breakdown => {
            if let Some(p) = guitar {
                layers.push(make_layer(
                    "Guitar",
                    "distorted_guitar",
                    p,
                    0,
                    segment_bars,
                    ids,
                ));
            }
        }
        // Chorus: full band — guitar + lead in the vocal register.
        SegmentRole::Chorus | SegmentRole::Bridge | SegmentRole::Custom => {
            if let Some(p) = guitar {
                layers.push(make_layer(
                    "Guitar",
                    "distorted_guitar",
                    p,
                    0,
                    segment_bars,
                    ids,
                ));
            }
            if let Some(p) = lead {
                layers.push(make_layer("Lead", "lead_synth", p, 0, segment_bars, ids));
            }
        }
    }
    layers
}

/// House layer composition: stabs in the mid sections, filtered chords in
/// the build, and a full stack (stabs + chords + octave lead) in the chorus.
fn house_layers(
    patterns: &[Pattern],
    role: SegmentRole,
    segment_bars: u32,
    ids: &mut IdSeq,
) -> Vec<Layer> {
    let drum = patterns.first();
    let bass = patterns.get(1);
    let lead = patterns.get(2);
    let pads = patterns.get(3);

    let mut layers = Vec::new();
    if let Some(p) = drum {
        layers.push(make_layer("Drums", "drum_sampler", p, 0, segment_bars, ids));
    }
    if let Some(p) = bass {
        layers.push(make_layer("Bass", "bass_synth", p, 0, segment_bars, ids));
    }
    match role {
        SegmentRole::Intro | SegmentRole::Outro => {}
        // Verse carries the plucky stabs.
        SegmentRole::Verse | SegmentRole::Breakdown | SegmentRole::Bridge => {
            if let Some(p) = lead {
                layers.push(make_layer("Lead", "pluck_synth", p, 0, segment_bars, ids));
            }
        }
        // Build adds the filtered chords for the pre-drop tension.
        SegmentRole::Build => {
            if let Some(p) = lead {
                layers.push(make_layer("Lead", "pluck_synth", p, 0, segment_bars, ids));
            }
            if let Some(p) = pads {
                layers.push(make_layer("Pads", "pad_synth", p, 0, segment_bars, ids));
            }
        }
        // Chorus peaks: stabs + chords + a doubled octave stab.
        SegmentRole::Chorus | SegmentRole::Custom => {
            if let Some(p) = lead {
                layers.push(make_layer("Lead", "pluck_synth", p, 0, segment_bars, ids));
            }
            if let Some(p) = pads {
                layers.push(make_layer("Pads", "pad_synth", p, 0, segment_bars, ids));
            }
            if let Some(p) = lead {
                layers.push(make_layer(
                    "Lead Octave",
                    "pluck_synth",
                    p,
                    12,
                    segment_bars,
                    ids,
                ));
            }
        }
    }
    layers
}

/// Ambient layer composition: drums, drone bass, and the evolving pad layer
/// in every segment; sparse bell tones join the mid and peak sections.
fn ambient_layers(
    patterns: &[Pattern],
    role: SegmentRole,
    segment_bars: u32,
    ids: &mut IdSeq,
) -> Vec<Layer> {
    let drum = patterns.first();
    let bass = patterns.get(1);
    let lead = patterns.get(2);
    let pads = patterns.get(3);

    let mut layers = Vec::new();
    if let Some(p) = drum {
        layers.push(make_layer("Drums", "drum_sampler", p, 0, segment_bars, ids));
    }
    if let Some(p) = bass {
        layers.push(make_layer("Bass", "bass_synth", p, 0, segment_bars, ids));
    }
    if let Some(p) = pads {
        layers.push(make_layer("Pads", "pad_synth", p, 0, segment_bars, ids));
    }
    match role {
        // Intros/outros stay sparse: drums, drone bass, pads only.
        SegmentRole::Intro | SegmentRole::Outro => {}
        // Mid and peak sections float sparse bell tones over the pad bed.
        SegmentRole::Verse
        | SegmentRole::Bridge
        | SegmentRole::Build
        | SegmentRole::Breakdown
        | SegmentRole::Chorus
        | SegmentRole::Custom => {
            if let Some(p) = lead {
                layers.push(make_layer("Bells", "bell_synth", p, 12, segment_bars, ids));
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

    // G2 — declare the octave register this layer occupies, derived from the
    // pattern's real pitch range plus the instance transpose. Percussion has
    // no register (drums aren't pitched by octave). This becomes the contract
    // the validator enforces: notes written later outside the register are
    // clamped or flagged, keeping layers in production-grade octave bands.
    let register = if instrument == "drum_sampler" {
        None
    } else {
        let (min_pitch, max_pitch) = pattern.notes.iter().fold((127u8, 0u8), |(lo, hi), n| {
            (lo.min(n.pitch.0), hi.max(n.pitch.0))
        });
        if min_pitch > max_pitch {
            None
        } else {
            let semis = transpose;
            let lo = i64::from(min_pitch) + i64::from(semis);
            let hi = i64::from(max_pitch) + i64::from(semis);
            #[expect(clippy::cast_possible_truncation, reason = "MIDI octave -1..9 fits i8")]
            let oct_of = |midi: i64| (midi.clamp(0, 127) / 12 - 1) as i8;
            Some(Register::new(oct_of(lo), oct_of(hi)))
        }
    };

    Layer {
        id: LayerId::from_uuid(uuid::Uuid::from_u128(ids.next_u128())),
        name: name.to_string(),
        instrument: instrument.to_string(),
        pattern_instances,
        register,
        automation: vec![],
        fx: None,
        mix_gain_db: None,
        mix_pan: None,
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "Tests assert plan invariants explicitly via expect()"
)]
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
    fn rock_genre_builds_power_chord_guitar_and_rock_drums() {
        // The Linkin Park stress test: a rock/nu-metal config must produce
        // the band-shaped plan — backbeat drums, driving bass, a power-chord
        // guitar riff, and a vocal-range lead — not the generic techno set.
        let mut cfg = config(32);
        cfg.genre = "nu_metal".to_string();
        let plan = plan_project(&cfg);

        let names: Vec<&str> = plan.patterns.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Drums_Rock_Backbeat",
                "Bass_Rock_Drive",
                "Guitar_PowerChord_Chug",
                "Lead_VocalRange",
            ]
        );
        // Rock drums: backbeat (kick 1&3, snare 2&4) with offbeat hats.
        let drums = &plan.patterns[0];
        assert!(drums.steps[0].active);
        assert!(drums.steps[4].active); // snare on 2
        assert!(drums.steps[8].active); // kick on 3
        assert!(drums.steps[12].active); // snare on 4
        assert!(drums.steps[2].active); // offbeat hat
        // Power-chord guitar: root+fifth dyads only (all notes in 2..4).
        let guitar = &plan.patterns[2];
        assert!(!guitar.notes.is_empty());
        assert!(
            guitar
                .notes
                .iter()
                .all(|n| (2..=4).contains(&n.pitch.octave())),
            "power chords sit in octaves 2..4"
        );
        // The chorus carries the full band: drums, bass, guitar, lead.
        let chorus = plan
            .arrangement
            .segments
            .iter()
            .find(|s| s.role == SegmentRole::Chorus)
            .expect("chorus exists");
        let layer_names: Vec<&str> = chorus.layers.iter().map(|l| l.name.as_str()).collect();
        assert!(
            layer_names.contains(&"Guitar") && layer_names.contains(&"Lead"),
            "chorus has guitar + lead: {layer_names:?}"
        );
        let guitar_layer = chorus.layers.iter().find(|l| l.name == "Guitar");
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let guitar_layer = guitar_layer.unwrap();
        assert_eq!(guitar_layer.instrument, "distorted_guitar");
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

    fn config_for(genre: &str, key: &str, duration_bars: u32) -> ProjectConfig {
        let mut cfg = config(duration_bars);
        cfg.genre = genre.to_string();
        cfg.key = key.to_string();
        cfg
    }

    #[test]
    fn genres_plan_different_patterns_not_identical() {
        // §5/F10 (R5): the genres must not compile to identical timelines.
        let techno = plan_project(&config_for("melodic_techno", "C minor", 16));
        let house = plan_project(&config_for("house", "G minor", 16));
        let ambient = plan_project(&config_for("ambient", "A minor", 32));

        let names =
            |plan: &Plan| -> Vec<String> { plan.patterns.iter().map(|p| p.name.clone()).collect() };
        assert_eq!(
            names(&techno),
            vec!["Drums_16", "Bass_RootFifth", "Lead_Arp"],
            "techno keeps the baseline patterns"
        );
        assert_eq!(
            names(&house),
            vec![
                "Drums_House_16",
                "Bass_OctaveGroove",
                "Lead_Stabs",
                "Pads_Chords"
            ],
            "house gets its own kit/bass/lead/pads"
        );
        assert_eq!(
            names(&ambient),
            vec!["Drums_Ambient_16", "Bass_Drone", "Lead_Bells", "Pads_Drone"],
            "ambient gets a drone/pad bed"
        );
        assert_ne!(names(&techno), names(&house));
        assert_ne!(names(&techno), names(&ambient));
        assert_ne!(names(&house), names(&ambient));
    }

    #[test]
    fn house_drums_are_four_on_floor_with_swung_offbeats() {
        let plan = plan_project(&config_for("house", "G minor", 16));
        let drums = &plan.patterns[0];
        assert_eq!(drums.name, "Drums_House_16");
        // Kick on 1 & 3, clap on 2 & 4.
        assert!(drums.steps[0].active && drums.steps[8].active);
        assert!(drums.steps[4].active && drums.steps[12].active);
        // Swung offbeat hats on the "and" eighths.
        for &step in &[2u32, 6, 10, 14] {
            assert!(drums.steps[step as usize].active, "step {step} hat active");
        }
        // The accent weighting differs from the techno 16th stream.
        assert!(drums.steps[2].velocity.0 > drums.steps[6].velocity.0);
    }

    #[test]
    fn ambient_pad_drone_has_exactly_one_same_pitch_overlap() {
        let plan = plan_project(&config_for("ambient", "A minor", 32));
        let pads = plan
            .patterns
            .iter()
            .find(|p| p.name == "Pads_Drone")
            .expect("ambient pad pattern exists");
        let mut overlaps = 0usize;
        for (idx, first) in pads.notes.iter().enumerate() {
            for second in pads.notes.iter().skip(idx + 1) {
                let same_pitch = first.pitch.0 == second.pitch.0;
                let overlaps_time = first.start.0
                    < second.start.0.saturating_add(second.duration.0)
                    && second.start.0 < first.start.0.saturating_add(first.duration.0);
                if same_pitch && overlaps_time && !first.tied && !second.tied {
                    overlaps += 1;
                }
            }
        }
        assert_eq!(
            overlaps, 1,
            "the ambient drone has exactly one same-pitch overlap "
        );
    }

    #[test]
    fn ambient_layers_include_pads_in_every_segment() {
        let plan = plan_project(&config_for("ambient", "A minor", 32));
        for segment in &plan.arrangement.segments {
            assert!(
                segment.layers.iter().any(|l| l.name == "Pads"),
                "segment '{}' must carry the pad layer",
                segment.name
            );
            assert!(
                segment.layers.iter().any(|l| l.instrument == "bell_synth")
                    || segment.role == SegmentRole::Intro
                    || segment.role == SegmentRole::Outro,
                "bells appear everywhere except sparse intros/outros"
            );
        }
    }

    #[test]
    fn house_and_ambient_patterns_are_in_key() {
        let house = plan_project(&config_for("house", "G minor", 16));
        let house_scale = Key::parse("G minor").expect("parses").to_scale();
        for pattern in &house.patterns {
            for note in &pattern.notes {
                assert!(
                    house_scale.contains_note(&note.pitch),
                    "{} in {}",
                    note.pitch.0,
                    pattern.name
                );
            }
        }
        let ambient = plan_project(&config_for("ambient", "A minor", 32));
        let ambient_scale = Key::parse("A minor").expect("parses").to_scale();
        for pattern in &ambient.patterns {
            for note in &pattern.notes {
                assert!(
                    ambient_scale.contains_note(&note.pitch),
                    "{} in {}",
                    note.pitch.0,
                    pattern.name
                );
            }
        }
    }

    #[test]
    fn house_chorus_is_denser_than_intro() {
        let plan = plan_project(&config_for("house", "G minor", 16));
        let intro = plan
            .arrangement
            .segments
            .iter()
            .find(|s| s.role == SegmentRole::Intro)
            .expect("intro");
        let chorus = plan
            .arrangement
            .segments
            .iter()
            .find(|s| s.role == SegmentRole::Chorus)
            .expect("chorus");
        assert_eq!(intro.layers.len(), 2, "intro is drums + bass");
        assert_eq!(
            chorus.layers.len(),
            5,
            "chorus stacks stabs + pads + octave"
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "Test enumerates 13 genre cases; splitting would hide the matrix shape"
    )]
    fn s3_genres_each_have_distinct_pattern_sets() {
        // The S3 genre families must each produce their own pattern names —
        // jazz, hip-hop, dnb, cinematic, pop, funk, blues, synthwave, lo-fi
        // — so a "jazz swing" brief never compiles to the techno baseline.
        let cases = [
            (
                "jazz",
                vec![
                    "Drums_Jazz_Swing",
                    "Bass_Walking",
                    "Lead_HornLine",
                    "Pads_JazzVoicings",
                ],
            ),
            (
                "hip-hop",
                vec![
                    "Drums_BoomBap",
                    "Bass_SubGroove",
                    "Lead_StabSamples",
                    "Pads_DarkChords",
                ],
            ),
            (
                "drum_and_bass",
                vec![
                    "Drums_Breakbeat",
                    "Bass_Rolling",
                    "Lead_AiryStabs",
                    "Pads_Deep",
                ],
            ),
            (
                "cinematic",
                vec![
                    "Drums_Timpani",
                    "Bass_DroneFifth",
                    "Lead_StringsLine",
                    "Pads_Orchestral",
                ],
            ),
            (
                "pop",
                vec![
                    "Drums_Pop",
                    "Bass_PopGroove",
                    "Lead_VocalHook",
                    "Pads_PopChords",
                ],
            ),
            (
                "funk",
                vec![
                    "Drums_Funk",
                    "Bass_FunkPop",
                    "Lead_HornStabs",
                    "Pads_FunkHits",
                ],
            ),
            (
                "blues",
                vec![
                    "Drums_Shuffle",
                    "Bass_ShuffleWalk",
                    "Lead_BluesRiff",
                    "Pads_Smoky",
                ],
            ),
            (
                "synthwave",
                vec![
                    "Drums_Synthwave",
                    "Bass_Pulsing8th",
                    "Lead_Arp16",
                    "Pads_Retro",
                ],
            ),
            (
                "lo-fi",
                vec![
                    "Drums_LofiSwing",
                    "Bass_WarmDrone",
                    "Lead_KeysMellow",
                    "Pads_WarmVinyl",
                ],
            ),
        ];
        let mut seen = std::collections::HashSet::new();
        for (genre, expected) in &cases {
            let mut cfg = config(16);
            cfg.genre = (*genre).to_string();
            let plan = plan_project(&cfg);
            let names: Vec<String> = plan.patterns.iter().map(|p| p.name.clone()).collect();
            assert_eq!(
                names,
                expected
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>(),
                "genre '{genre}' must use its own pattern set"
            );
            seen.insert(names.join(","));
        }
        assert_eq!(
            seen.len(),
            cases.len(),
            "every family's pattern set is unique"
        );
    }

    #[test]
    fn s3_genre_patterns_are_in_key() {
        for genre in [
            "jazz",
            "hip-hop",
            "drum_and_bass",
            "cinematic",
            "pop",
            "funk",
            "blues",
            "synthwave",
            "lo-fi",
        ] {
            let mut cfg = config(16);
            cfg.genre = genre.to_string();
            let plan = plan_project(&cfg);
            let scale = Key::parse("C minor").expect("parses").to_scale();
            for pattern in &plan.patterns {
                for note in &pattern.notes {
                    assert!(
                        scale.contains_note(&note.pitch),
                        "{} note {} out of key in genre '{genre}'",
                        pattern.name,
                        note.pitch.0
                    );
                }
            }
        }
    }

    #[test]
    fn s3_genre_chorus_is_denser_than_intro() {
        for genre in [
            "jazz",
            "hip-hop",
            "drum_and_bass",
            "cinematic",
            "pop",
            "funk",
            "blues",
            "synthwave",
            "lo-fi",
        ] {
            let mut cfg = config(16);
            cfg.genre = genre.to_string();
            let plan = plan_project(&cfg);
            let intro = plan
                .arrangement
                .segments
                .iter()
                .find(|s| s.role == SegmentRole::Intro)
                .expect("intro");
            let chorus = plan
                .arrangement
                .segments
                .iter()
                .find(|s| s.role == SegmentRole::Chorus)
                .expect("chorus");
            assert!(
                chorus.layers.len() > intro.layers.len(),
                "'{genre}': chorus ({} layers) denser than intro ({} layers)",
                chorus.layers.len(),
                intro.layers.len()
            );
        }
    }

    #[test]
    fn bass_roots_anchor_on_any_key_not_just_c() {
        // G minor: root pitch class 7. The bass range starts at MIDI 24 (C),
        // so positional indexing would wrongly pick C as the root — the
        // pattern must anchor on the actual key root (G = MIDI 31).
        let mut cfg = config(8);
        cfg.key = "G minor".to_string();
        let plan = plan_project(&cfg);
        let bass = plan.patterns[1].clone();
        assert_eq!(bass.name, "Bass_RootFifth");
        let root_note = &bass.notes[0];
        assert_eq!(root_note.pitch.0 % 12, 7, "bass downbeat must be G");
        // The fifth (D) must sit above the root.
        let fifth_note = &bass.notes[2];
        assert_eq!(fifth_note.pitch.0 % 12, 2, "beat-3 note must be D (fifth)");
        assert!(fifth_note.pitch.0 > root_note.pitch.0);
    }

    #[test]
    fn lead_arpeggio_anchors_on_non_c_root() {
        // G minor lead: root pc 7 → MIDI 67 (G4), third 70 (Bb4), fifth 74 (D5).
        let mut cfg = config(8);
        cfg.key = "G minor".to_string();
        let plan = plan_project(&cfg);
        let lead = plan.patterns[2].clone();
        assert_eq!(lead.name, "Lead_Arp");
        let pcs: Vec<u8> = lead.notes.iter().map(|n| n.pitch.0 % 12).collect();
        assert_eq!(pcs, vec![7, 10, 2, 7], "arpeggio must spell G-Bb-D-G");
    }
}
