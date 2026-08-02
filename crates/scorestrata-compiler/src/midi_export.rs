//! MIDI export for Timeline IR.
//!
//! Converts `TimelineIR` events to standard MIDI file bytes.
//! This completes the Phase 1 exit criteria: "exportable MIDI."

use crate::timeline::{EventKind, TimelineIR};
use scorestrata_core::domain::PPQ;
use scorestrata_core::error::ScorestrataError;

/// MIDI file header constants.
const MIDI_HEADER_MAGIC: &[u8; 4] = b"MThd";
const MIDI_TRACK_MAGIC: &[u8; 4] = b"MTrk";
const MIDI_HEADER_LENGTH: u32 = 6;
const MIDI_FORMAT: u16 = 0; // Single track
const MIDI_TRACK_NUM: u16 = 1;

/// Note On status byte (channel 0).
const NOTE_ON: u8 = 0x90;
/// Note Off status byte (channel 0).
const NOTE_OFF: u8 = 0x80;

/// Export a `TimelineIR` to MIDI file bytes.
///
/// Events must be sorted by tick for correct MIDI delta times.
/// Use `timeline.events.sort_by_key(|e| e.tick)` before calling.
///
/// # Errors
///
/// Returns an error if the timeline has no events.
pub fn export_midi(timeline: &TimelineIR) -> Result<Vec<u8>, ScorestrataError> {
    debug_assert!(
        timeline.events.windows(2).all(|w| w[0].tick <= w[1].tick),
        "export_midi expects events sorted by tick"
    );

    if timeline.events.is_empty() {
        return Err(ScorestrataError::CompilerError {
            pass: "midi_export".to_string(),
            message: "No events to export".to_string(),
        });
    }

    let mut track_data = Vec::new();

    // Write tempo meta event (FF 51 03 tttttt)
    let microseconds_per_beat: u32 = 60_000_000u32 / timeline.bpm;
    track_data.push(0x00); // delta time = 0
    track_data.push(0xFF);
    track_data.push(0x51);
    track_data.push(0x03);
    track_data.push(((microseconds_per_beat >> 16) & 0xFF) as u8);
    track_data.push(((microseconds_per_beat >> 8) & 0xFF) as u8);
    track_data.push((microseconds_per_beat & 0xFF) as u8);

    // Convert events to MIDI track data
    let mut current_tick: u64 = 0;

    for event in &timeline.events {
        let delta = event.tick.0.saturating_sub(current_tick);
        match &event.kind {
            EventKind::NoteOn { pitch, velocity } => {
                write_variable_length(&mut track_data, delta);
                track_data.push(NOTE_ON);
                track_data.push(pitch.0);
                track_data.push(velocity.0);
            }
            EventKind::NoteOff { pitch } => {
                write_variable_length(&mut track_data, delta);
                track_data.push(NOTE_OFF);
                track_data.push(pitch.0);
                track_data.push(0x00);
            }
            // ControlChange and Marker events are not exported to MIDI (skip silently)
            EventKind::ControlChange { .. } | EventKind::Marker { .. } => {}
        }
        current_tick = event.tick.0;
    }

    // End of track meta event (FF 2F 00)
    track_data.push(0x00); // delta time = 0
    track_data.push(0xFF);
    track_data.push(0x2F);
    track_data.push(0x00);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "Track data length fits in u32 for MIDI"
    )]
    let track_length = track_data.len() as u32;

    // Assemble MIDI file
    let mut midi = Vec::new();

    // Header chunk
    midi.extend_from_slice(MIDI_HEADER_MAGIC);
    write_u32(&mut midi, MIDI_HEADER_LENGTH);
    write_u16(&mut midi, MIDI_FORMAT);
    write_u16(&mut midi, MIDI_TRACK_NUM);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "PPQ constant fits in u16 for MIDI"
    )]
    write_u16(&mut midi, PPQ as u16);

    // Track chunk
    midi.extend_from_slice(MIDI_TRACK_MAGIC);
    write_u32(&mut midi, track_length);
    midi.extend_from_slice(&track_data);

    Ok(midi)
}

/// Write a 32-bit big-endian integer.
fn write_u32(buf: &mut Vec<u8>, value: u32) {
    buf.push(((value >> 24) & 0xFF) as u8);
    buf.push(((value >> 16) & 0xFF) as u8);
    buf.push(((value >> 8) & 0xFF) as u8);
    buf.push((value & 0xFF) as u8);
}

/// Write a 16-bit big-endian integer.
fn write_u16(buf: &mut Vec<u8>, value: u16) {
    buf.push(((value >> 8) & 0xFF) as u8);
    buf.push((value & 0xFF) as u8);
}

/// Write a variable-length quantity (MIDI VLQ encoding).
///
/// Supports values up to 0x0FFFFFFF (4 bytes). Larger values may produce
/// invalid MIDI data.
fn write_variable_length(buf: &mut Vec<u8>, mut value: u64) {
    debug_assert!(
        value <= 0x0FFF_FFFF,
        "VLQ value {value} exceeds MIDI maximum 0x0FFFFFFF"
    );
    let mut bytes = Vec::new();
    bytes.push((value & 0x7F) as u8);
    value >>= 7;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "VLQ bytes masked to 7 bits, provably fit in u8"
    )]
    while value > 0 {
        bytes.push(((value & 0x7F) | 0x80) as u8);
        value >>= 7;
    }
    bytes.reverse();
    buf.extend_from_slice(&bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{EventKind, TimelineEvent};
    use scorestrata_core::domain::{MidiNote, Tick, Velocity};
    use scorestrata_core::ids::*;

    #[test]
    fn export_midi_produces_valid_header() {
        let mut timeline = TimelineIR::empty(ProjectId::new(), 120);
        timeline.events.push(TimelineEvent {
            id: 0,
            tick: Tick::new(0),
            kind: EventKind::NoteOn {
                pitch: MidiNote::new(60),
                velocity: Velocity::new(100),
            },
            track_id: TrackId::new(),
            segment_id: SegmentId::new(),
            layer_id: LayerId::new(),
            pattern_id: None,
        });
        timeline.events.push(TimelineEvent {
            id: 1,
            tick: Tick::new(480),
            kind: EventKind::NoteOff {
                pitch: MidiNote::new(60),
            },
            track_id: TrackId::new(),
            segment_id: SegmentId::new(),
            layer_id: LayerId::new(),
            pattern_id: None,
        });

        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let midi = export_midi(&timeline).unwrap();
        // MThd(4) + header_len(4) + format(2) + track_count(2) + division(2) = 14
        // Then MTrk starts at offset 14
        assert!(midi.len() >= 18, "MIDI too short: {} bytes", midi.len());
        assert_eq!(&midi[0..4], b"MThd");
        assert_eq!(&midi[14..18], b"MTrk");
    }

    #[test]
    fn export_empty_timeline_errors() {
        let timeline = TimelineIR::empty(ProjectId::new(), 120);
        assert!(export_midi(&timeline).is_err());
    }

    #[test]
    fn write_variable_length_correct() {
        let mut buf = Vec::new();
        write_variable_length(&mut buf, 0);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        write_variable_length(&mut buf, 127);
        assert_eq!(buf, vec![0x7F]);

        buf.clear();
        write_variable_length(&mut buf, 128);
        assert_eq!(buf, vec![0x81, 0x00]);
    }
}
