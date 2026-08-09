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
///
/// Partial equality is deliberate (not `Eq`): the optional
/// [`Articulation::Portamento`] carries `f32` glide milliseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Per-note articulation (C) — accent/staccato/legato/portamento/pedal.
    /// `None` = unarticulated (legacy behavior, byte-identical renders).
    #[serde(default)]
    pub articulation: Option<Articulation>,
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

impl SegmentRole {
    /// Map an arrangement-template role string (G6) to a segment role.
    ///
    /// `drop` maps to [`SegmentRole::Chorus`] (the energy peak), and any
    /// unknown template role falls back to [`SegmentRole::Custom`].
    #[must_use]
    pub fn from_template_role(role: &str) -> Self {
        match role {
            "intro" => Self::Intro,
            "build" => Self::Build,
            "drop" | "chorus" => Self::Chorus,
            "breakdown" => Self::Breakdown,
            "outro" => Self::Outro,
            "verse" => Self::Verse,
            "bridge" => Self::Bridge,
            _ => Self::Custom,
        }
    }
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
    /// Group-bus assignment (B): `Some("drums")` routes this layer through
    /// the named group bus (with its inserts) before the stereo sum, and its
    /// reverb/delay sends go to the shared returns. `None` = direct to sum
    /// (legacy behavior, byte-identical to pre-bus renders).
    #[serde(default)]
    pub group: Option<String>,
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

/// Reverb impulse-response character for the convolution return bus (B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReverbIr {
    /// Small room — short tail, dense early reflections.
    Room,
    /// Plate — mid tail, smooth bright metallic bloom.
    Plate,
    /// Hall — long tail, sparse shimmering decay.
    Hall,
}

/// One named group bus (B).
///
/// Layers assigned to the group sum here, then the bus inserts
/// (EQ → comp → saturation) shape the summed group before the stereo sum.
/// `None` group on a layer = direct routing (legacy).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
pub struct GroupBus {
    /// Bus name — must match [`Layer::group`] to attract layers.
    pub name: String,
    /// Bus-level insert chain applied to the summed group.
    pub inserts: FxChain,
}

/// Per-note performance articulation (Phase C).
///
/// Resolved by the compiler into timeline effects: accents boost velocity,
/// staccato/legato scale gate length, portamento emits a pitch-bend glide
/// curve, pedal emits sustain. `None` on a note = unarticulated (legacy).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum Articulation {
    /// Emphasize: velocity boost + slight gate cut.
    Accent,
    /// Short, separated: gate length ~50 %.
    Staccato,
    /// Full length, smooth: gate length ~100 %.
    #[default]
    Legato,
    /// Pitch glide from the written pitch up/down by `semitones` over
    /// `glide_ms` (emitted as a pitch-bend CC curve).
    Portamento {
        /// Glide distance in semitones (positive = up).
        semitones: i32,
        /// Glide time in milliseconds.
        glide_ms: f32,
    },
    /// Sustain pedal: hold the note past its written gate.
    Pedal,
}

/// Opt-in performance layer (Phase C) — the "humanize" that kills the
/// grid-perfect synthetic tell (plan §1.2, FL Studio `Alt+R` / Ableton
/// Groove Pool semantics).
///
/// All variation is **seeded and deterministic** (R2): the same spec on the
/// same project always produces the same performance, and `None` on the
/// project (or `enabled: false`) keeps renders byte-identical to Phase B
/// (R1). Per-voice streams derive from `(seed, layer_id)` so chords don't
/// shift as rigid blocks (Ableton Random semantics).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
pub struct HumanizeSpec {
    /// Master switch; `false` = identical to an un-humanized render (R1).
    pub enabled: bool,
    /// PRNG seed; `None` = derived deterministically from the project
    /// content hash (stable across runs, unique per project).
    pub seed: Option<u64>,
    /// Gaussian velocity spread as a fraction of 127 (typical 0.02–0.08).
    pub velocity_sigma: f32,
    /// Gaussian note-on timing jitter in ms (tight 1–5, loose up to 30).
    pub timing_sigma_ms: f32,
    /// Gate-length variation as a fraction of each note's written length.
    pub gate_length_jitter: f32,
    /// Groove template name (`swing16`, `laid_back`, `driving`, …);
    /// `None` = straight timing.
    pub groove: Option<String>,
    /// Groove strength (0.0–1.0); scaled linearly by the renderer.
    pub groove_amount: f32,
}

/// Three-band Linkwitz–Riley 24 dB/oct multiband config (Phase D).
///
/// Crossovers at the classic low/mid and mid/high split points
/// (~120 Hz and ~3 kHz, plan §1.3). Each band gets its own makeup gain
/// and optional per-band width; a mid/side matrix is applied per band so
/// the widest element (highs) can be pushed and the low end kept mono.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
pub struct MultibandCfg {
    /// Enable the multiband stage (`false` = bypass, R1).
    pub enabled: bool,
    /// Low/mid crossover in Hz (default 120).
    pub low_crossover_hz: f32,
    /// Mid/high crossover in Hz (default 3000).
    pub high_crossover_hz: f32,
    /// Low-band makeup gain in dB (default 0).
    pub low_gain_db: f32,
    /// Mid-band makeup gain in dB (default 0).
    pub mid_gain_db: f32,
    /// High-band makeup gain in dB (default 0).
    pub high_gain_db: f32,
    /// Per-band width (1.0 = neutral; <1 mono-izes, >1 widens).
    pub low_width: f32,
    /// Per-band width (1.0 = neutral).
    pub mid_width: f32,
    /// Per-band width (1.0 = neutral).
    pub high_width: f32,
    /// Keep everything below this Hz fully mono (side = 0). Default 200.
    pub mono_below_hz: f32,
}

impl Default for MultibandCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            low_crossover_hz: 120.0,
            high_crossover_hz: 3000.0,
            low_gain_db: 0.0,
            mid_gain_db: 0.0,
            high_gain_db: 0.0,
            low_width: 1.0,
            mid_width: 1.0,
            high_width: 1.0,
            mono_below_hz: 200.0,
        }
    }
}

/// Soft-clip saturation stage (Phase D) — analog-ish drive with 2×
/// oversampling so harmonics above Nyquist fold cleanly (no aliasing).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
pub struct SaturationCfg {
    /// Drive: input gain in dB before the soft clip (default 6).
    pub drive_db: f32,
    /// Softness of the clip curve (0 = hard-ish, 1 = gentle tape).
    pub softness: f32,
    /// Oversampling factor applied before saturation (default 2).
    pub oversample: u32,
    /// Output trim in dB after the stage (default −3 to compensate drive).
    pub trim_db: f32,
}

impl Default for SaturationCfg {
    fn default() -> Self {
        Self {
            drive_db: 6.0,
            softness: 0.5,
            oversample: 2,
            trim_db: -3.0,
        }
    }
}

/// TPDF dither + bit-depth config (Phase D).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
pub struct DitherCfg {
    /// Target bit depth for export (16 or 24).
    pub bits: u16,
    /// Enable triangular-PDF dither (kills quantization distortion).
    pub enabled: bool,
}

impl Default for DitherCfg {
    fn default() -> Self {
        Self {
            bits: 16,
            enabled: true,
        }
    }
}

/// The mastering chain (Phase D) — the full "mastered" signal flow.
///
/// Stages (§1.3): multiband dynamics/width → saturation → limiter → dither.
/// `None` on the project keeps the current width+gain+limiter path
/// byte-identical (R1).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
pub struct MasterChain {
    /// Master switch; `false` = current path (R1).
    pub enabled: bool,
    /// 3-band LR-24 multiband with per-band width + mono-below.
    pub multiband: MultibandCfg,
    /// Soft-clip saturation stage; `None` = bypass.
    pub saturation: Option<SaturationCfg>,
    /// TPDF dither + bit-depth for the export; `None` = 16-bit no dither.
    pub dither: Option<DitherCfg>,
}

/// The send/return + group-bus routing graph (Phase B).
///
/// Mirrors the DAW model: layers route into named group busses (drums /
/// bass / harmony / lead / fx), each group applies its own inserts, and
/// reverb/delay sends from every layer fold into shared returns. `None` on
/// the project keeps the flat per-layer fold — byte-identical to pre-bus
/// renders (R1).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
pub struct BusGraph {
    /// Group busses in declaration order (first match wins by name).
    pub groups: Vec<GroupBus>,
    /// Convolution-reverb return character (B): `None` keeps the legacy
    /// Schroeder–Moorer reverb for the reverb send.
    pub reverb_ir: Option<ReverbIr>,
    /// Reverb pre-delay in ms (10–100) separating the dry attack from the
    /// wet tail; `None` = 0.
    pub reverb_pre_delay_ms: Option<f32>,
}

/// A per-track insert FX chain (§8.1, production-plan §7.1) — the
/// "production" in production-grade: high-pass, EQ scoops, compression,
/// sidechain ducking, and reverb/delay sends.
///
/// Deterministic, sample-rate-aware DSP lives in `scorestrata_audio::fx`;
/// this is the serializable configuration the tools write and the renderer
/// reads. `None` FX on a layer = clean insert (only the static mix gain/pan).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    /// Continuous sidechain duck amount (0.0–1.0) — the production-mix lane
    /// (B). `None` = legacy on/off via [`Self::sidechain_from_kick`]; `Some`
    /// drives the renderer's envelope follower with a continuous depth.
    #[serde(default)]
    pub sidechain_amount: Option<f32>,
}

/// The master-bus state of the project's mixer (§8.1 / §8.6, G5).
///
/// Controls the renderer applies to the *final mix fold*, distinct from
/// per-layer insert FX ([`Layer::fx`]) and per-layer faders
/// ([`Layer::mix_gain_db`] / [`Layer::mix_pan`]).
///
/// Mirrors the DAW mastering chain the production plan requires: master
/// gain staging, stereo width (mid/side), and a peak limiter ceiling. The
/// audio engine maps this onto its `RenderConfig` so `Project.mixer` is
/// the single serializable source of truth for the mix bus.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixerState {
    /// Master output gain in dB applied to the folded mix (0.0 = unity).
    pub master_gain_db: f32,
    /// Stereo width of the master bus: 1.0 = neutral, 0.0 = mono fold,
    /// >1.0 widens the side channel (mid/side transform).
    pub stereo_width: f32,
    /// Master limiter true-peak ceiling in dBTP; `None` disables the
    /// master limiter (leaves headroom for the platform's own limiter).
    pub limiter_ceiling_db: Option<f32>,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            master_gain_db: 0.0,
            stereo_width: 1.0,
            limiter_ceiling_db: Some(-1.0),
        }
    }
}

/// A project's genre reference profile (G7) — the spectral fingerprint of a
/// reference track the agent is producing *toward*.
///
/// Produced by `ss analyze reference.wav` and imported with
/// `import_reference_profile`; the render gate then measures the deviation
/// of the project's render from this reference instead of only the built-in
/// genre average. Fields mirror [`scorestrata_audio::analysis::GenreFingerprint`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpectralReference {
    /// Name of the reference track the fingerprint came from.
    pub track_name: String,
    /// Genre this reference belongs to.
    pub genre: String,
    /// Expected energy share in the six audit bands (sub/bass/lo-mid/mid/hi-mid/air).
    pub band_energy_expected: [f32; 6],
    /// Expected spectral centroid in Hz.
    pub centroid_expected_hz: f32,
    /// Maximum acceptable energy share above 16 kHz (aliasing proxy).
    pub aliasing_max: f32,
    /// Loudness target in LUFS (from the reference master).
    pub loudness_target_lufs: f32,
}

impl SpectralReference {
    /// L1 deviation of an observed spectrum from this reference (0 = match,
    /// approaches 1 = divergent) — the render gate metric (G7).
    #[must_use]
    pub fn deviation(&self, band_energy: &[f32; 6], centroid_hz: f32, aliasing_ratio: f32) -> f32 {
        let band_dev = band_energy
            .iter()
            .zip(self.band_energy_expected.iter())
            .map(|(actual, expected)| (actual - expected).abs())
            .sum::<f32>()
            / 2.0;
        let centroid_dev = if self.centroid_expected_hz > 0.0 {
            ((centroid_hz - self.centroid_expected_hz).abs() / self.centroid_expected_hz).min(1.0)
        } else {
            0.0
        };
        let alias_dev = if aliasing_ratio > self.aliasing_max {
            ((aliasing_ratio - self.aliasing_max) / 0.5).min(1.0)
        } else {
            0.0
        };
        band_dev.mul_add(0.6, centroid_dev.mul_add(0.3, alias_dev * 0.1))
    }
}

impl Default for SpectralReference {
    fn default() -> Self {
        Self {
            track_name: String::new(),
            genre: String::new(),
            band_energy_expected: [0.0; 6],
            centroid_expected_hz: 0.0,
            aliasing_max: 0.3,
            loudness_target_lufs: -14.0,
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
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "Automation interpolation fraction fits f32 for practical spans"
                )]
                let frac = (tick - a.tick.0) as f32 / span as f32;
                return (b.value - a.value).mul_add(frac, a.value);
            }
        }
        last.value
    }

    /// The MIDI CC value (0–127) at `tick`, for timeline emission.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Value is clamped to 0..=1 then scaled to 0..=127, so the u8 cast is lossless"
    )]
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
    pub const fn contains_note(&self, pitch: MidiNote) -> bool {
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
    /// Genre reference profile (G7) — the spectral fingerprint the render is
    /// produced toward, imported from `ss analyze` of a reference track.
    #[serde(default)]
    pub spectral_reference: Option<SpectralReference>,
    /// Mixer master-bus state (G5) — master gain, stereo width, and the
    /// master limiter ceiling the renderer applies to the final mix fold.
    #[serde(default)]
    pub mixer: MixerState,
    /// Opt-in sampled percussion bank (A) — kit name from the embedded
    /// catalog (`rock`, `house`, `hiphop`, `techno`, `trap`). `None` =
    /// the legacy synth-drum path, keeping bankless renders byte-identical
    /// (R1).
    #[serde(default)]
    pub sound_bank: Option<String>,
    /// Send/return + group-bus routing graph (B). `None` = flat per-layer
    /// fold, byte-identical to pre-bus renders (R1).
    #[serde(default)]
    pub bus_graph: Option<BusGraph>,
    /// Opt-in performance layer (C): humanize + groove + articulation.
    /// `None` = today's behavior, byte-identical renders (R1).
    #[serde(default)]
    pub humanize: Option<HumanizeSpec>,
    /// Opt-in mastering chain (D): multiband + saturation + dither.
    /// `None` = current width+gain+limiter path, byte-identical (R1).
    #[serde(default)]
    pub master_chain: Option<MasterChain>,
    /// Current status.
    pub status: ProjectStatus,
}
