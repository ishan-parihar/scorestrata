//! Core domain model for Scorestrata.
//!
//! This module defines the musical, operational, and evidence entities
//! that form the shared vocabulary of the system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{
    LayerId, NoteId, PatternId, PatternInstanceId, ProjectId, SegmentId, StepId, TrackId,
};

// ── Musical Time ────────────────────────────────────────────────────

/// Pulses per quarter note (PPQ). Standard MIDI resolution.
pub const PPQ: u32 = 480;

/// A position in musical time, measured in ticks from the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Tick(pub u64);

impl Tick {
    /// Create a new tick position.
    #[must_use]
    pub const fn new(ticks: u64) -> Self {
        Self(ticks)
    }

    /// Convert bar, beat, and sub-beat ticks to absolute ticks.
    #[must_use]
    pub fn from_bar_beat_tick(bar: u32, beat: u32, sub_tick: u32, time_signature_num: u32) -> Self {
        let ticks_per_bar = u64::from(PPQ) * u64::from(time_signature_num);
        let ticks_per_beat = u64::from(PPQ);
        Self(
            u64::from(bar) * ticks_per_bar + u64::from(beat) * ticks_per_beat + u64::from(sub_tick),
        )
    }

    /// Decompose into bar, beat, and sub-beat ticks.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Musical time values bounded to u32 range in practice"
    )]
    pub fn to_bar_beat_tick(self, time_signature_num: u32) -> (u32, u32, u32) {
        let ticks_per_bar = u64::from(PPQ) * u64::from(time_signature_num);
        let ticks_per_beat = u64::from(PPQ);
        let bar = (self.0 / ticks_per_bar) as u32;
        let remainder = self.0 % ticks_per_bar;
        let beat = (remainder / ticks_per_beat) as u32;
        let sub_tick = (remainder % ticks_per_beat) as u32;
        (bar, beat, sub_tick)
    }
}

/// Duration of a musical event in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Duration(pub u64);

impl Duration {
    /// Whole note duration in ticks.
    #[must_use]
    pub const fn whole() -> Self {
        Self(PPQ as u64 * 4)
    }

    /// Half note duration in ticks.
    #[must_use]
    pub const fn half() -> Self {
        Self(PPQ as u64 * 2)
    }

    /// Quarter note duration in ticks.
    #[must_use]
    pub const fn quarter() -> Self {
        Self(PPQ as u64)
    }

    /// Eighth note duration in ticks.
    #[must_use]
    pub const fn eighth() -> Self {
        Self(PPQ as u64 / 2)
    }

    /// Sixteenth note duration in ticks.
    #[must_use]
    pub const fn sixteenth() -> Self {
        Self(PPQ as u64 / 4)
    }
}

/// Time signature (numerator and denominator).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeSignature {
    /// Numerator (e.g. 4 in 4/4).
    pub numerator: u32,
    /// Denominator (e.g. 4 in 4/4).
    pub denominator: u32,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

// ── Pitch & Note ────────────────────────────────────────────────────

/// A MIDI note number (0–127).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MidiNote(pub u8);

impl MidiNote {
    /// Create a MIDI note from a number, clamping to 0–127.
    #[must_use]
    pub fn new(note: u8) -> Self {
        Self(note.min(127))
    }

    /// Get the note name (e.g. "C4", "F#3").
    #[must_use]
    pub fn name(&self) -> String {
        const NAMES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        #[expect(
            clippy::cast_possible_wrap,
            reason = "MIDI notes 0-127 map to safe octave range -1 to 9"
        )]
        let octave = (self.0 / 12) as i8 - 1;
        let name = NAMES[(self.0 % 12) as usize];
        format!("{name}{octave}")
    }

    /// Get the pitch class (0–11, where 0 = C).
    #[must_use]
    pub const fn pitch_class(&self) -> u8 {
        self.0 % 12
    }

    /// Get the octave number.
    #[must_use]
    #[expect(
        clippy::cast_possible_wrap,
        reason = "MIDI notes 0-127 map to safe octave range -1 to 9"
    )]
    pub const fn octave(&self) -> i8 {
        (self.0 / 12) as i8 - 1
    }
}

/// Velocity of a note (0–127).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Velocity(pub u8);

impl Velocity {
    /// Create a velocity, clamping to 0–127.
    #[must_use]
    pub fn new(vel: u8) -> Self {
        Self(vel.min(127))
    }

    /// Default velocity (100).
    #[must_use]
    pub const fn default_vel() -> Self {
        Self(100)
    }

    /// Normalize to 0.0–1.0.
    #[must_use]
    pub fn as_f32(&self) -> f32 {
        f32::from(self.0) / 127.0
    }
}

// ── Note & Step ─────────────────────────────────────────────────────

/// A musical note with pitch, position, duration, and velocity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Unique identifier for this note.
    pub id: NoteId,
    /// MIDI note number.
    pub pitch: MidiNote,
    /// Start position in ticks from the pattern/segment start.
    pub start: Tick,
    /// Duration in ticks.
    pub duration: Duration,
    /// Velocity (loudness).
    pub velocity: Velocity,
    /// Whether this note is tied to the next.
    #[serde(default)]
    pub tied: bool,
}

/// A step in a step-sequencer pattern (e.g. for drums).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// Unique identifier for this step.
    pub id: StepId,
    /// Step index (0-based).
    pub index: u32,
    /// Whether this step is active.
    pub active: bool,
    /// Velocity if active (defaults to 100).
    #[serde(default = "Velocity::default_vel")]
    pub velocity: Velocity,
}

// ── Pattern ─────────────────────────────────────────────────────────

/// A musical pattern containing notes or steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Unique identifier.
    pub id: PatternId,
    /// Human-readable name.
    pub name: String,
    /// Duration in bars.
    pub bars: u32,
    /// Time signature.
    #[serde(default)]
    pub time_signature: TimeSignature,
    /// Note-based pattern data.
    pub notes: Vec<Note>,
    /// Step-sequencer data (for drums, etc.).
    #[serde(default)]
    pub steps: Vec<Step>,
}

// ── Segment ─────────────────────────────────────────────────────────

/// The role a segment plays in the arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentRole {
    /// Introduction.
    Intro,
    /// Verse.
    Verse,
    /// Pre-chorus / build.
    Build,
    /// Chorus / drop.
    Chorus,
    /// Bridge.
    Bridge,
    /// Breakdown.
    Breakdown,
    /// Outro.
    Outro,
    /// Custom segment type.
    Custom,
}

/// A section of the arrangement (e.g. intro, verse, chorus).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    /// Unique identifier.
    pub id: SegmentId,
    /// Human-readable name.
    pub name: String,
    /// Role in the arrangement.
    pub role: SegmentRole,
    /// Start bar (0-indexed).
    pub start_bar: u32,
    /// Duration in bars.
    pub bars: u32,
    /// Layers within this segment.
    #[serde(default)]
    pub layers: Vec<Layer>,
}

/// A layer within a segment (e.g. drums, bass, lead).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    /// Unique identifier.
    pub id: LayerId,
    /// Human-readable name.
    pub name: String,
    /// Instrument type.
    pub instrument: String,
    /// Pattern instances placed in this layer.
    #[serde(default)]
    pub pattern_instances: Vec<PatternInstance>,
    /// Declared octave register (G2) — the range this layer is expected to
    /// occupy. The compiler clamps and the validator flags notes outside it,
    /// enforcing the production-grade register discipline (bass 1–2, chords
    /// 3–5, lead 5–7, …). `None` = no constraint (legacy layers).
    #[serde(default)]
    pub register: Option<Register>,
    /// Automation lanes (G4) — per-param curves (cutoff, volume, pan, …)
    /// that make the layer move instead of sitting static.
    #[serde(default)]
    pub automation: Vec<Automation>,
    /// Insert FX chain (G5) — HPF, EQ, compressor, sidechain, sends.
    /// `None` = clean insert (static mix gain/pan only).
    #[serde(default)]
    pub fx: Option<FxChain>,
    /// Per-layer mix gain override in dB (G5, mixer-fader action).
    /// `None` = the engine's static per-instrument gain staging.
    #[serde(default)]
    pub mix_gain_db: Option<f32>,
    /// Per-layer mix pan override (−1.0…1.0). `None` = static panning.
    #[serde(default)]
    pub mix_pan: Option<f32>,
}

/// An octave register — the vertical band a layer occupies on the timeline.
///
/// Encodes the DAW convention of keeping instruments in separate registers so
/// layers don't fight for the same frequency range (sub/bass in octaves 1–2,
/// chords/pads 3–5, leads 5–7, hats/air 6–8). Octaves follow the MIDI
/// convention (C4 = middle C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Register {
    /// Lowest octave (inclusive).
    pub min_octave: i8,
    /// Highest octave (inclusive).
    pub max_octave: i8,
}

/// The parameter an automation lane drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AutomationParam {
    /// Filter cutoff sweep (MIDI CC 74) — the classic build-up "open up".
    Cutoff,
    /// Track volume (MIDI CC 7).
    Volume,
    /// Stereo pan (MIDI CC 10).
    Pan,
    /// Effect send level (reverb/delay bus, MIDI CC 91/93).
    Send,
}

impl AutomationParam {
    /// The MIDI CC controller number used to encode this lane in the timeline.
    #[must_use]
    pub const fn cc_number(self) -> u8 {
        match self {
            Self::Cutoff => 74,
            Self::Volume => 7,
            Self::Pan => 10,
            Self::Send => 91,
        }
    }
}

/// A single automation point: a value at a tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutomationPoint {
    /// Absolute tick position.
    pub tick: Tick,
    /// Parameter value, normalized 0.0–1.0.
    pub value: f32,
}

/// Wire-format automation point used by the MCP tools (`{ "tick": 0,
/// "value": 0.2 }`) so agents don't have to construct the `Tick` newtype.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutomationPointJson {
    /// Tick position.
    pub tick: u64,
    /// Parameter value, normalized 0.0–1.0.
    pub value: f32,
}

/// A peaking EQ band (RBJ bell filter) in an [`FxChain`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FreqBand {
    /// Center frequency in Hz.
    pub freq_hz: f32,
    /// Gain in dB (−12…+12).
    pub gain_db: f32,
    /// Filter Q (bandwidth).
    pub q: f32,
}

/// Feed-forward compressor configuration (§7.2 of the production plan).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CompressorCfg {
    /// Threshold in dBFS (e.g. −18).
    pub threshold_db: f32,
    /// Compression ratio (e.g. 3.0).
    pub ratio: f32,
    /// Attack time in ms.
    pub attack_ms: f32,
    /// Release time in ms.
    pub release_ms: f32,
    /// Make-up gain in dB.
    pub makeup_db: f32,
}

impl Default for CompressorCfg {
    fn default() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 3.0,
            attack_ms: 5.0,
            release_ms: 120.0,
            makeup_db: 6.0,
        }
    }
}

/// Effect send levels (0.0–1.0) for the reverb/delay buses (§7.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SendLevels {
    /// Reverb send amount.
    pub reverb: f32,
    /// Delay send amount.
    pub delay: f32,
}

impl Default for SendLevels {
    fn default() -> Self {
        Self {
            reverb: 0.0,
            delay: 0.0,
        }
    }
}

/// A per-track insert FX chain (§8.1, production-plan §7.1) — the
/// "production" in production-grade: high-pass, EQ scoops, compression,
/// sidechain ducking, and reverb/delay sends.
///
/// Deterministic, sample-rate-aware DSP lives in `scorestrata_audio::fx`;
/// this is the serializable configuration the tools write and the renderer
/// reads. `None` FX on a layer = clean insert (only the static mix gain/pan).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FxChain {
    /// High-pass cutoff in Hz (e.g. 30 / 100 / 120); `None` = no HPF.
    pub high_pass_hz: Option<f32>,
    /// Peaking EQ bands (low-mid scoop, presence, air shelf).
    pub eq: Vec<FreqBand>,
    /// Optional feed-forward compressor.
    pub compressor: Option<CompressorCfg>,
    /// Duck this layer against the kick's envelope (electronic-music staple).
    pub sidechain_from_kick: bool,
    /// Reverb / delay send amounts (0.0–1.0).
    pub sends: SendLevels,
}

impl Default for FxChain {
    fn default() -> Self {
        Self {
            high_pass_hz: None,
            eq: Vec::new(),
            compressor: None,
            sidechain_from_kick: false,
            sends: SendLevels::default(),
        }
    }
}

/// An automation lane — a per-layer, per-parameter curve (§8.1).
///
/// Points are linearly interpolated by the compiler when emitting
/// `ControlChange` events into the timeline, and by the audio renderer when
/// sweeping filters/levels, mirroring how a producer draws automation in a
/// DAW arrange view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    /// Layer this lane drives.
    pub layer_id: LayerId,
    /// Parameter being automated.
    pub param: AutomationParam,
    /// The curve, ordered by tick.
    pub points: Vec<AutomationPoint>,
}

impl Automation {
    /// Interpolated value at `tick` (linear between points, clamped to the
    /// first/last point outside the curve).
    #[must_use]
    pub fn value_at(&self, tick: Tick) -> f32 {
        let tick = tick.0;
        if self.points.is_empty() {
            return 0.0;
        }
        let first = self.points[0];
        if tick <= first.tick.0 {
            return first.value;
        }
        let last = self.points[self.points.len() - 1];
        if tick >= last.tick.0 {
            return last.value;
        }
        // Linear interpolation between the bracketing points.
        for pair in self.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if tick >= a.tick.0 && tick <= b.tick.0 {
                let span = (b.tick.0 - a.tick.0).max(1);
                let frac = (tick - a.tick.0) as f32 / span as f32;
                return a.value + (b.value - a.value) * frac;
            }
        }
        last.value
    }

    /// The MIDI CC value (0–127) at `tick`, for timeline emission.
    #[must_use]
    pub fn cc_value_at(&self, tick: Tick) -> u8 {
        (self.value_at(tick).clamp(0.0, 1.0) * 127.0).round() as u8
    }
}

impl Register {
    /// Create a register, normalizing an inverted range.
    #[must_use]
    pub fn new(min_octave: i8, max_octave: i8) -> Self {
        Self {
            min_octave: min_octave.min(max_octave),
            max_octave: max_octave.max(min_octave),
        }
    }

    /// Whether a MIDI note falls inside this register.
    #[must_use]
    pub fn contains_note(&self, pitch: MidiNote) -> bool {
        let octave = pitch.octave();
        octave >= self.min_octave && octave <= self.max_octave
    }

    /// Clamp a MIDI note into this register (moves it to the nearest octave
    /// boundary while keeping the pitch class).
    #[must_use]
    pub fn clamp_note(&self, pitch: MidiNote) -> MidiNote {
        let octave = pitch.octave();
        if octave < self.min_octave {
            let lift = 12 * u8::try_from(self.min_octave - octave).unwrap_or(0);
            MidiNote::new(pitch.0.saturating_add(lift))
        } else if octave > self.max_octave {
            let drop = 12 * u8::try_from(octave - self.max_octave).unwrap_or(0);
            MidiNote::new(pitch.0.saturating_sub(drop))
        } else {
            pitch
        }
    }
}

/// A reference to a pattern placed in a specific time position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternInstance {
    /// Unique identifier for this instance.
    pub id: PatternInstanceId,
    /// The pattern being referenced.
    pub pattern_id: PatternId,
    /// Start tick within the segment.
    pub start_tick: Tick,
    /// Transpose amount in semitones.
    #[serde(default)]
    pub transpose: i32,
    /// Velocity multiplier (1.0 = no change).
    #[serde(default = "default_one")]
    pub velocity_multiplier: f32,
}

const fn default_one() -> f32 {
    1.0
}

// ── Track ───────────────────────────────────────────────────────────

/// A track groups segments and layers for a specific instrument voice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Unique identifier.
    pub id: TrackId,
    /// Human-readable name.
    pub name: String,
    /// Instrument identifier.
    pub instrument: String,
    /// Whether this track is muted.
    #[serde(default)]
    pub muted: bool,
    /// Whether this track is soloed.
    #[serde(default)]
    pub solo: bool,
    /// Gain in dB.
    #[serde(default)]
    pub gain_db: f32,
    /// Pan position (-1.0 left, 0.0 center, 1.0 right).
    #[serde(default)]
    pub pan: f32,
}

// ── Arrangement ─────────────────────────────────────────────────────

/// The arrangement defines the structure of the composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arrangement {
    /// Total duration in bars.
    pub total_bars: u32,
    /// Time signature.
    #[serde(default)]
    pub time_signature: TimeSignature,
    /// Segments in order.
    pub segments: Vec<Segment>,
    /// Energy curve targets per segment.
    #[serde(default)]
    pub energy_curve: Vec<EnergyPoint>,
}

/// An energy target for a segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyPoint {
    /// Which segment this applies to.
    pub segment_id: SegmentId,
    /// Target energy level (0.0–1.0).
    pub target_energy: f32,
}

// ── Project Config ──────────────────────────────────────────────────

/// Delivery target for the final render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryTarget {
    /// Streaming platforms.
    Streaming,
    /// CD quality.
    Cd,
    /// Club / DJ.
    Club,
    /// Broadcast.
    Broadcast,
}

/// High-level project configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project name.
    pub name: String,
    /// Genre (e.g. `melodic_techno`).
    pub genre: String,
    /// BPM (beats per minute).
    pub bpm: u32,
    /// Musical key (e.g. "C minor").
    pub key: String,
    /// Time signature.
    #[serde(default)]
    pub time_signature: TimeSignature,
    /// Total duration in bars.
    pub duration_bars: u32,
    /// Delivery target.
    pub delivery_target: DeliveryTarget,
    /// Mood tags.
    #[serde(default)]
    pub mood: Vec<String>,
    /// Reference tracks.
    #[serde(default)]
    pub references: Vec<String>,
}

// ── Brief ───────────────────────────────────────────────────────────

/// A production brief describing what to create.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brief {
    /// Raw text from the user/agent.
    pub raw_text: String,
    /// Parsed genre.
    #[serde(default)]
    pub genre: Option<String>,
    /// Mood tags.
    #[serde(default)]
    pub mood: Vec<String>,
    /// Reference tracks.
    #[serde(default)]
    pub references: Vec<String>,
    /// Delivery target.
    #[serde(default)]
    pub delivery_target: Option<DeliveryTarget>,
    /// Hard constraints.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Confidence in the parsed brief (0.0–1.0).
    #[serde(default)]
    pub confidence: f32,
}

// ── Project ─────────────────────────────────────────────────────────

/// The current status of a project.
///
/// Implements the full state machine from architecture spec §4.2:
///
/// ```text
/// BriefReceived
///   → ConfigDrafted → ConfigValidationFailed / ConfigApproved
///   → ArrangementPlanned
///   → SkeletonCreated → SegmentDrafting
///   → SegmentCandidatesGenerated → SegmentValidationFailed / SegmentValidated
///   → TransitionValidationFailed / TransitionValidated
///   → MixDrafting → MixValidationFailed
///   → Compiling → Validating
///   → Experimenting → Calibrating
///   → Rendering → RenderFailed / FinalValidationPassed
///   → RenderFinalized
///   → AwaitingHumanDecision / NeedsPolicyReview / NeedsHumanReview
///   → Replaying / Quarantined / Archived
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    /// Brief received, not yet processed.
    BriefReceived,
    /// Config drafted.
    ConfigDrafted,
    /// Config validation failed.
    ConfigValidationFailed,
    /// Config approved.
    ConfigApproved,
    /// Arrangement planned.
    ArrangementPlanned,
    /// Skeleton created.
    SkeletonCreated,
    /// Segments being drafted.
    SegmentDrafting,
    /// Segment candidates generated (multi-candidate evaluation).
    SegmentCandidatesGenerated,
    /// Segment validation failed.
    SegmentValidationFailed,
    /// Segments validated.
    SegmentsValidated,
    /// Transition validation failed.
    TransitionValidationFailed,
    /// Transition validated.
    TransitionValidated,
    /// Mix being drafted.
    MixDrafting,
    /// Mix validation failed.
    MixValidationFailed,
    /// Compiling.
    Compiling,
    /// Validating.
    Validating,
    /// Experimenting (multi-candidate evaluation).
    Experimenting,
    /// Calibrating against genre profiles.
    Calibrating,
    /// Rendering.
    Rendering,
    /// Render failed.
    RenderFailed,
    /// Final validation passed.
    FinalValidationPassed,
    /// Render finalized.
    RenderFinalized,
    /// Awaiting human decision.
    AwaitingHumanDecision,
    /// Needs policy review.
    NeedsPolicyReview,
    /// Needs human review.
    NeedsHumanReview,
    /// Replaying from a snapshot.
    Replaying,
    /// Quarantined (stuck/regression).
    Quarantined,
    /// Archived.
    Archived,
}

/// The root project entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Unique identifier.
    pub id: ProjectId,
    /// Project name.
    pub name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp.
    pub modified_at: DateTime<Utc>,
    /// Production brief.
    pub brief: Brief,
    /// Project configuration.
    pub config: ProjectConfig,
    /// The arrangement.
    pub arrangement: Arrangement,
    /// Tracks.
    #[serde(default)]
    pub tracks: Vec<Track>,
    /// Patterns.
    #[serde(default)]
    pub patterns: Vec<Pattern>,
    /// Current status.
    pub status: ProjectStatus,
}
