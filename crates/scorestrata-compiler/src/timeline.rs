//! Timeline IR — the compiler's primary output.
//!
//! The timeline is a flat, ordered sequence of events that can be
//! validated, diffed, and rendered deterministically.

use scorestrata_core::domain::{MidiNote, Tick, Velocity};
use scorestrata_core::ids::{LayerId, PatternId, ProjectId, SegmentId, TrackId};
use serde::{Deserialize, Serialize};

/// A single event on the timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Unique event identifier.
    pub id: u64,
    /// Absolute tick position.
    pub tick: Tick,
    /// Event kind.
    pub kind: EventKind,
    /// Track this event belongs to.
    pub track_id: TrackId,
    /// Segment this event belongs to.
    pub segment_id: SegmentId,
    /// Layer this event belongs to.
    pub layer_id: LayerId,
    /// Pattern that generated this event (if any).
    pub pattern_id: Option<PatternId>,
}

/// The kind of timeline event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// A note-on event.
    NoteOn {
        /// MIDI note number.
        pitch: MidiNote,
        /// Velocity.
        velocity: Velocity,
    },
    /// A note-off event.
    NoteOff {
        /// MIDI note number.
        pitch: MidiNote,
    },
    /// A control change event.
    ControlChange {
        /// CC number.
        controller: u8,
        /// CC value.
        value: u8,
    },
    /// A marker event (section start, transition, etc.).
    Marker {
        /// Marker name.
        name: String,
    },
}

/// The complete timeline IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineIR {
    /// Project this timeline belongs to.
    pub project_id: ProjectId,
    /// All events, sorted by tick.
    pub events: Vec<TimelineEvent>,
    /// Total duration in ticks.
    pub total_ticks: u64,
    /// BPM.
    pub bpm: u32,
    /// Time signature numerator.
    pub time_signature_num: u32,
    /// Hash of the input that produced this timeline.
    pub input_hash: String,
    /// Compiler version.
    pub compiler_version: String,
}

impl TimelineIR {
    /// Create an empty timeline.
    #[must_use]
    pub fn empty(project_id: ProjectId, bpm: u32) -> Self {
        Self {
            project_id,
            events: Vec::new(),
            total_ticks: 0,
            bpm,
            time_signature_num: 4,
            input_hash: String::new(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Get the number of events.
    #[must_use]
    pub const fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Get events for a specific segment.
    #[must_use]
    pub fn events_for_segment(&self, segment_id: SegmentId) -> Vec<&TimelineEvent> {
        self.events
            .iter()
            .filter(|e| e.segment_id == segment_id)
            .collect()
    }

    /// Get events for a specific layer.
    #[must_use]
    pub fn events_for_layer(&self, layer_id: LayerId) -> Vec<&TimelineEvent> {
        self.events
            .iter()
            .filter(|e| e.layer_id == layer_id)
            .collect()
    }

    /// Compute a content hash of this timeline.
    #[must_use]
    pub fn content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let json = serde_json::to_vec(self).unwrap_or_default();
        let hash = Sha256::digest(&json);
        hex::encode(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::ids::ProjectId;

    #[test]
    fn empty_timeline_has_zero_events() {
        let timeline = TimelineIR::empty(ProjectId::new(), 120);
        assert_eq!(timeline.event_count(), 0);
    }
}
