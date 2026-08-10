//! MIDI import — standard MIDI file → scorestrata [`Project`].
//!
// Tick/tempo math converts bounded file-tick counts between u64/f64/u32;
// MIDI durations fit practical file lengths, so the pedantic cast lints are
// expected (same posture as the audio engine's DSP casts).
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "MIDI tick/tempo math converts bounded file-tick counts between u64/f64/u32; durations fit practical file lengths"
)]

//! The production loop closes with this module: `ss export-midi` writes a
//! project's timeline as a `.mid`, and `ss import-midi` reads any standard
//! format-0/1 `.mid` back into a full [`Project`] bundle — tracks become
//! layers, GM program changes become instrument families (which route to the
//! recorded SF2 bank / drum kit when rendered with the production banks),
//! tempo and time signature are recovered, and note pairs are quantized to
//! the engine's PPQ grid.
//!
//! # Determinism (R2)
//!
//! Parsing is a pure function of the MIDI bytes: the same file always
//! produces the same project. Imported note starts are quantized to the
//! tick grid, so the resulting compile/render is byte-stable.
//!
//! # Instrument mapping
//!
//! MIDI channels carry GM programs; the importer maps program numbers to the
//! instrument families the audio engine knows (`guitar`, `bass`, `strings`,
//! `brass`, `organ`, `flute`, `leadsaw`, `warmpad`, `drums`, …). Channel 9 is
//! the GM percussion channel and becomes a drum layer. The imported project
//! enables the recorded production banks (`melodic_bank: "sf2"`,
//! `sound_bank: "sf2"`), so `ss render` plays real recorded instruments.

use scorestrata_core::domain::{
    Arrangement, Articulation, Brief, DeliveryTarget, Duration, Layer, MidiNote, Note, Pattern,
    PatternInstance, Project, ProjectConfig, ProjectStatus, Segment, SegmentRole, Tick,
    TimeSignature, Velocity,
};
use scorestrata_core::error::ScorestrataError;
use scorestrata_core::ids::{LayerId, NoteId, PatternId, ProjectId, SegmentId};

use midly::{MidiMessage, Smf, Timing, TrackEventKind};

/// Engine PPQ (ticks per quarter note) — the import grid.
const PPQ: u32 = scorestrata_core::domain::PPQ;

/// Import a standard MIDI file into a pipeline-ready [`Project`].
///
/// - **Tempo**: the first `Tempo` meta event sets the project BPM.
/// - **Time signature**: the first `TimeSignature` meta (defaults to 4/4).
/// - **Tracks**: every non-conductor track becomes a layer; its name (if
///   any) becomes the layer name. Channel 9 becomes a `drums` layer.
/// - **Instruments**: each track's last GM program change picks the family.
/// - **Notes**: note-on/off pairs are paired per track+pitch into notes with
///   start ticks and durations, scaled to the PPQ grid and quantized to
///   1/16 grid steps (30 PPQ ticks).
///
/// # Errors
///
/// Returns a compiler error if the bytes are not a parseable SMF file.
#[expect(
    clippy::too_many_lines,
    reason = "MIDI import walks tracks, pairs notes, and rebuilds the project in one deterministic pass"
)]
pub fn import_midi(bytes: &[u8], name: &str) -> Result<Project, ScorestrataError> {
    let smf = Smf::parse(bytes).map_err(|e| ScorestrataError::CompilerError {
        pass: "midi_import".to_string(),
        message: format!("invalid standard MIDI file: {e}"),
    })?;

    let division = match smf.header.timing {
        Timing::Metrical(d) => u32::from(d.as_int()),
        // SMPTE-timed files are rare; treat one second per frame tick as a
        // metrical fallback is wrong, so refuse loudly rather than mis-time.
        Timing::Timecode { .. } => {
            return Err(ScorestrataError::CompilerError {
                pass: "midi_import".to_string(),
                message: "SMPTE-timed MIDI is not supported (use a metrical file)".to_string(),
            });
        }
    };
    let scale = f64::from(PPQ) / f64::from(division.max(1));

    let mut bpm: u32 = 120;
    let mut time_sig = TimeSignature::default();
    let mut found_time_sig = false;

    // Per-track note accumulation: pitch, vel, start, dur (file ticks).
    let mut track_notes: Vec<Vec<(u8, u8, u64, u64)>> = Vec::new();
    let mut track_programs: Vec<Option<u8>> = Vec::new();
    let mut track_names: Vec<String> = Vec::new();
    let mut track_is_drum: Vec<bool> = Vec::new();
    let mut max_file_tick: u64 = 0;

    for track in &smf.tracks {
        let mut notes: Vec<(u8, u8, u64, u64)> = Vec::new();
        let mut open: std::collections::HashMap<u8, (u8, u64)> = std::collections::HashMap::new();
        let mut program: Option<u8> = None;
        let mut track_name = String::new();
        let mut is_drum = false;
        let mut abs: u64 = 0;

        for event in track {
            abs += u64::from(event.delta.as_int());
            max_file_tick = max_file_tick.max(abs);
            match event.kind {
                TrackEventKind::Midi { channel, message } => {
                    if channel.as_int() == 9 {
                        is_drum = true;
                    }
                    match message {
                        MidiMessage::ProgramChange { program: p } => {
                            program = Some(p.as_int());
                        }
                        MidiMessage::NoteOn { key, vel } => {
                            let pitch = key.as_int();
                            let velocity = vel.as_int();
                            if velocity == 0 {
                                // Note-on with velocity 0 is a note-off.
                                if let Some((v, start)) = open.remove(&pitch) {
                                    notes.push((pitch, v, start, abs.saturating_sub(start)));
                                }
                            } else {
                                // Overlapping same-pitch notes: close the old one.
                                if let Some((v, start)) = open.insert(pitch, (velocity, abs)) {
                                    notes.push((pitch, v, start, abs.saturating_sub(start)));
                                }
                            }
                        }
                        MidiMessage::NoteOff { key, .. } => {
                            let pitch = key.as_int();
                            if let Some((v, start)) = open.remove(&pitch) {
                                notes.push((pitch, v, start, abs.saturating_sub(start)));
                            }
                        }
                        _ => {}
                    }
                }
                TrackEventKind::Meta(meta) => match meta {
                    midly::MetaMessage::Tempo(t) => {
                        if bpm == 120 {
                            // First tempo wins; microseconds per quarter note.
                            let us = u64::from(t.as_int());
                            bpm = (60_000_000u64 / us.max(1)) as u32;
                        }
                    }
                    midly::MetaMessage::TimeSignature(n, d, ..) => {
                        if !found_time_sig {
                            time_sig = TimeSignature {
                                numerator: u32::from(n),
                                denominator: 2u32.pow(u32::from(d)),
                            };
                            found_time_sig = true;
                        }
                    }
                    midly::MetaMessage::TrackName(bytes) => {
                        track_name = String::from_utf8_lossy(bytes).trim().to_string();
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        // Close any dangling notes at track end (truncated files).
        for (pitch, (v, start)) in open {
            notes.push((pitch, v, start, abs.saturating_sub(start).max(1)));
        }
        notes.sort_by_key(|n| (n.2, n.0));
        track_notes.push(notes);
        track_programs.push(program);
        track_names.push(track_name);
        track_is_drum.push(is_drum);
    }

    let total_file_ticks = max_file_tick;
    let ticks_per_bar_file = u64::from(PPQ) * u64::from(time_sig.numerator);
    let total_bars = ((total_file_ticks * scale as u64).div_ceil(ticks_per_bar_file)).max(1) as u32;

    // Build layers from every track that carries notes (skip empty tracks
    // and the conductor track — usually track 0 with only meta events).
    let mut patterns_out = Vec::new();
    let mut layers = Vec::new();

    for (i, notes) in track_notes.iter().enumerate() {
        if notes.is_empty() {
            continue;
        }
        let pattern = Pattern {
            id: PatternId::new(),
            name: format!("Track_{i}"),
            bars: total_bars,
            time_signature: time_sig,
            notes: notes
                .iter()
                .map(|(pitch, vel, start, dur)| {
                    let start_ppq = quantize(start_ticks_to_ppq(*start, scale));
                    let dur_ppq = ((*dur as f64 * scale).round() as u64).max(u64::from(PPQ) / 16);
                    Note {
                        id: NoteId::new(),
                        pitch: MidiNote::new(*pitch),
                        start: Tick::new(start_ppq),
                        duration: Duration(dur_ppq),
                        velocity: Velocity::new(*vel),
                        tied: false,
                        articulation: Some(Articulation::Legato),
                    }
                })
                .collect(),
            steps: Vec::new(),
        };

        let instrument = if track_is_drum[i] {
            "drums".to_string()
        } else {
            track_programs[i]
                .map_or("synth", instrument_for_program)
                .to_string()
        };
        let layer_name = if track_names[i].is_empty() {
            format!("Track_{i}")
        } else {
            track_names[i].clone()
        };
        let layer = Layer {
            id: LayerId::new(),
            name: layer_name,
            instrument,
            pattern_instances: vec![PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id: pattern.id,
                start_tick: Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            }],
            register: None,
            automation: Vec::new(),
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
            group: None,
        };
        layers.push(layer);
        patterns_out.push(pattern);
    }

    if layers.is_empty() {
        return Err(ScorestrataError::CompilerError {
            pass: "midi_import".to_string(),
            message: "MIDI contains no note events".to_string(),
        });
    }

    let config = ProjectConfig {
        name: name.to_string(),
        genre: "import".to_string(),
        bpm: bpm.clamp(40, 240),
        key: "C minor".to_string(),
        time_signature: time_sig,
        duration_bars: total_bars,
        delivery_target: DeliveryTarget::Streaming,
        mood: Vec::new(),
        references: Vec::new(),
    };

    let segment = Segment {
        id: SegmentId::new(),
        name: "Imported".to_string(),
        role: SegmentRole::Chorus,
        start_bar: 0,
        bars: total_bars,
        layers,
    };

    let project = Project {
        id: ProjectId::new(),
        name: name.to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        brief: Brief {
            raw_text: format!("Imported from MIDI: {name}"),
            genre: Some("import".to_string()),
            mood: Vec::new(),
            references: Vec::new(),
            delivery_target: None,
            constraints: Vec::new(),
            confidence: 0.5,
        },
        config,
        arrangement: Arrangement {
            total_bars,
            time_signature: time_sig,
            segments: vec![segment],
            energy_curve: Vec::new(),
        },
        tracks: Vec::new(),
        patterns: patterns_out,
        spectral_reference: None,
        mixer: scorestrata_core::domain::MixerState::default(),
        // Imported tracks render through the recorded production banks so an
        // imported song plays real instruments (and the GM drum kit) — not
        // bare synth voices. `None` stays available by clearing these.
        sound_bank: Some("sf2".to_string()),
        melodic_bank: Some("sf2".to_string()),
        bus_graph: None,
        humanize: None,
        master_chain: None,
        status: ProjectStatus::ConfigApproved,
    };
    Ok(project)
}

/// Convert file ticks to PPQ ticks (scaled by the division ratio).
fn start_ticks_to_ppq(file_tick: u64, scale: f64) -> u64 {
    (file_tick as f64 * scale).round() as u64
}

/// Quantize a tick to the 1/16-note grid (PPQ/4 = 120 ticks).
fn quantize(tick: u64) -> u64 {
    let grid = u64::from(PPQ) / 4;
    (tick / grid) * grid
}

/// Map a GM program number to the instrument family the audio engine knows.
///
/// The names flow into `voice_params_for_instrument` and (when the recorded
/// bank is enabled) `melodic_instrument_for`, so the program decides both
/// the synth timbre and the sample-bank voice.
#[must_use]
pub const fn instrument_for_program(program: u8) -> &'static str {
    match program {
        0..=7 => "piano",
        // Chromatic percussion (8-15) and harp (46-47) both land on keys.
        8..=15 | 46..=47 => "keys",
        16..=23 => "organ",
        24..=25 => "acoustic_guitar",
        26..=27 => "clean_guitar",
        28..=31 => "guitar",
        32..=45 => "bass",
        48..=55 => "strings",
        56..=63 => "brass",
        64..=79 => "flute",
        80..=81 => "leadsaw",
        82..=87 => "lead",
        88..=95 => "warmpad",
        96..=111 => "pad",
        _ => "synth",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi_export::export_midi;
    use crate::timeline::{EventKind, TimelineEvent, TimelineIR};
    use scorestrata_core::ids::{LayerId, SegmentId, TrackId};

    fn note_pair(id: u64, tick: u64, pitch: u8, vel: u8, dur: u64) -> Vec<TimelineEvent> {
        let track = TrackId::new();
        let segment = SegmentId::new();
        let layer = LayerId::new();
        vec![
            TimelineEvent {
                id,
                tick: Tick::new(tick),
                kind: EventKind::NoteOn {
                    pitch: MidiNote::new(pitch),
                    velocity: Velocity::new(vel),
                },
                track_id: track,
                segment_id: segment,
                layer_id: layer,
                pattern_id: None,
            },
            TimelineEvent {
                id: id + 1,
                tick: Tick::new(tick + dur),
                kind: EventKind::NoteOff {
                    pitch: MidiNote::new(pitch),
                },
                track_id: track,
                segment_id: segment,
                layer_id: layer,
                pattern_id: None,
            },
        ]
    }

    #[test]
    fn imports_tempo_track_and_notes_from_export() {
        // Build a tiny timeline: two notes on one layer at 120 BPM.
        let mut timeline = TimelineIR::empty(ProjectId::new(), 120);
        let mut events = note_pair(0, 0, 60, 100, 480);
        events.extend(note_pair(2, 480, 64, 90, 480));
        timeline.events = events;
        timeline.total_ticks = 960;

        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let midi = export_midi(&timeline).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let project = import_midi(&midi, "roundtrip").unwrap();

        assert_eq!(project.config.bpm, 120);
        assert_eq!(project.arrangement.total_bars, 1);
        assert_eq!(project.patterns.len(), 1);
        let notes = &project.patterns[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].pitch.0, 60);
        assert_eq!(notes[0].start.0, 0);
        assert_eq!(notes[0].velocity.0, 100);
        assert_eq!(notes[1].pitch.0, 64);
        assert_eq!(notes[1].start.0, 480);
        assert_eq!(project.sound_bank.as_deref(), Some("sf2"));
        assert_eq!(project.melodic_bank.as_deref(), Some("sf2"));

        // Round-trip: compiling the imported project must recover both notes.
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let compiled = crate::compile_project(&project).unwrap();
        let note_ons: Vec<(u64, u8, u8)> = compiled
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::NoteOn { pitch, velocity } => Some((e.tick.0, pitch.0, velocity.0)),
                _ => None,
            })
            .collect();
        assert_eq!(note_ons, vec![(0, 60, 100), (480, 64, 90)]);
    }

    #[test]
    fn import_quantizes_to_the_ppq_grid() {
        assert_eq!(quantize(5), 0);
        assert_eq!(quantize(119), 0);
        assert_eq!(quantize(120), 120);
        assert_eq!(quantize(125), 120);
        assert_eq!(quantize(240), 240);
    }

    #[test]
    fn instrument_mapping_covers_rock_families() {
        assert_eq!(instrument_for_program(30), "guitar");
        assert_eq!(instrument_for_program(25), "acoustic_guitar");
        assert_eq!(instrument_for_program(27), "clean_guitar");
        assert_eq!(instrument_for_program(34), "bass");
        assert_eq!(instrument_for_program(48), "strings");
        assert_eq!(instrument_for_program(61), "brass");
        assert_eq!(instrument_for_program(16), "organ");
        assert_eq!(instrument_for_program(73), "flute");
        assert_eq!(instrument_for_program(81), "leadsaw");
        assert_eq!(instrument_for_program(89), "warmpad");
        assert_eq!(instrument_for_program(0), "piano");
    }

    #[test]
    fn empty_midi_errors() {
        // A header-only SMF with no note events must fail loudly.
        let mut file = Vec::new();
        file.extend_from_slice(b"MThd");
        file.extend_from_slice(&[0, 0, 0, 6, 0, 0, 0, 1, 0, 120]);
        file.extend_from_slice(b"MTrk");
        file.extend_from_slice(&[0, 0, 0, 4]);
        file.extend_from_slice(&[0, 0xFF, 0x2F, 0x00]);
        assert!(import_midi(&file, "empty").is_err());
    }
}
