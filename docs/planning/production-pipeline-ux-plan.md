# Production-Grade Music Pipeline & Agent UX Plan (G′)

> **Status:** plan — research-backed, awaiting implementation.
> **Companion:** `docs/planning/audio-quality-visualization-plan.md` (A′–F′),
> `docs/architecture/architecture.md` (§8.1 musical entities, §4.2 state machine, §11 planes).

---

## 0. Executive summary

The audio engine now sounds clean (no distortion, −1.0 dBTP ceiling, genre-distinct
fingerprints — Iterations A′–D′), but the *music* is not production grade because the
**compositional UX is not production grade**. A human producer in FL Studio / Ableton
composes through a *layered timeline with octave-aware instruments, repeated patterns,
automation lanes, per-track FX chains, and arranged sections*. Our agent has none of that
view — only flat tool calls and a JSON event list.

**Direct answer to "is the timeline view for the many layers working?" — No.**
The audit found:
- `TimelineIR` (§) is a flat `Vec<TimelineEvent>` with `events_for_layer`/`events_for_segment`
  filters — *data*, not a *view*. There is no per-layer timeline resource, no `ss timeline`
  command, no MCP tool that renders "layer × time × pitch".
- `isolate_layer` and `solo_segment` exist but mutate/summarize; they don't visualize.
- The domain model has `Segment`/`Layer`/`Pattern`/`PatternInstance`/`Note`/`Step` but the
  architecture's own planned `Automation`, `FxChain`, `MixerState` entities (§8.1, line ~900)
  are **not implemented**.
- Only the `PatternInstance.transpose` (i32) + `velocity_multiplier` primitives exist for
  octave/repetition work; there is no "repeat pattern ×N", no "bass lives in octave 1–2"
  constraint, no automation lanes, no per-track FX.

This plan specifies the DAW-inspired UX — a **layered timeline view** plus the
**octave/repeat/automation/FX tool set** — so the AI agent can act as a full-blown
professional music producer.

---

## 1. Research synthesis — how humans produce in FL Studio / Ableton

Source: web research on professional production workflows (FL Studio pattern/playlist
workflow, Ableton session→arrangement workflow) + reference-track arrangement analysis.

### 1.1 The human production pipeline

1. **Compose a foundational loop (8–16 bars)** in the pattern/clip editor — drums, bass,
   chords, lead, FX. This is the "atom" that gets repeated and mutated.
2. **Duplicate & arrange the loop across the timeline** (intro → build → drop → breakdown →
   second drop → outro), with per-clip transposition and variation.
3. **Assign octave registers per instrument** so layers don't fight (see §1.3).
4. **Humanize** with velocity variation and **automate** (filter cutoff, volume, pan, FX
   params) so the music isn't static.
5. **Route to mixer buses** with per-track effect chains: EQ → compression → (send) reverb /
   delay, plus **sidechain** the bass/pads to the kick.
6. **Transition design**: risers, white-noise sweeps, snare rolls, fills, impact hits at
   section boundaries.
7. **Mix** (gain staging, panning, stereo imaging, reference-track A/B) then **master**
   (multiband dynamics, stereo width, loudness limiting).

### 1.2 Arrangement structure (production-grade reference)

| Section | Typical bars | Role |
|---|---|---|
| Intro | 16–32 | stripped beat bed (kick, hats, faint sub) — DJ mix-in |
| Build / Groove rise | 16–32 | secondary percussion, chords enter, filter sweep / riser |
| Drop / Main peak | 32–64 | full bass + leads + wide chords |
| Breakdown / Bridge | 16–32 | drums strip out, pads/vocals carry |
| Second drop | 32–64 | full power + ornamentation / counter-melody |
| Outro | 16–32 | progressive strip-down back to kick/hats |

Techno is more linear: continuous modulation, elements swap every 16/32 bars, a strip-down
"valley" in the middle. A professional session has **20–60+ tracks** (drums 8–15, bass
4–10, harmony 4–10, leads 3–8, FX 6–15).

### 1.3 Octave / frequency allocation (the "which octave" map)

| Instrument | MIDI octave range | Hz range | Engineering rule |
|---|---|---|---|
| Sub bass / kick fundamental | 1–2 (C1–B2) | 20–100 | mono, dead center, HPF everything else at ~30 Hz |
| Mid bass (reese/pluck) | 2–3 (C2–B3) | 65–261 | sidechain to kick |
| Chords / pads / keys | 3–5 (C3–B5) | 130–523 | voiced up, not close-voiced low |
| Leads / arps / plucks | 5–7 (C5–B7) | 523–2093 | cuts through via 2–4 kHz sensitivity |
| Hats / air / FX | 6–8 | 6k–20k | wide in stereo field |

Scoop 250–500 Hz (low-mids) on non-bass elements to avoid boxiness; high-pass everything
below 100–120 Hz except kick and bass.

---

## 2. Current-state audit — what exists vs. what the pipeline needs

### 2.1 What exists (verified in code)

| Area | Exists | Notes |
|---|---|---|
| Domain model | ✅ | `Project`, `Arrangement`, `Segment` (role/bars), `Layer` (instrument), `Pattern` (notes/steps), `PatternInstance` (transpose, velocity_multiplier), `Track` (muted/solo/gain_db/pan), `Note` (pitch/start/duration/velocity/tied), `Step`, `EnergyPoint` |
| Timeline IR | ✅ | `TimelineIR` flat events; `NoteOn/NoteOff/ControlChange/Marker` kinds; per-layer/per-segment filters; content_hash |
| Arrangement planning | ✅ | genre-aware planner (A′–C′): techno/house/ambient patterns + segment composition + energy curve |
| Note writing | ✅ | `write_pattern_notes`, `write_pattern_steps`, `create_pattern`, `place_pattern_in_segment`, `apply_motivic_variation` (transpose/retrograde/syncopate) |
| Production ops | ✅ | `quantize_layer_to_scale`, `harmonize_melody`, `add_drum_fill`, `add_transition_package` (fill/riser/impact), `increase_segment_density`, `reduce_masking`, `normalize_loudness_target` |
| Debug plane | ✅ | `isolate_layer`, `solo_segment`, `explain_metric`, `get_metric_evidence`, `get_snapshot_diff` |
| Audio engine | ✅ | polyBLEP synth, ADSR, real drums, limiter, stereo render, stems, spectral audit (`ss analyze`) |
| Validator | ✅ | L1–L4 (config/symbolic/timeline/audio), energy transition checks, timeline checks (overlap, automation range for energy, absolute timing) |

### 2.2 What's missing (the gap = this plan)

| # | Capability | Why the agent needs it | Where it lands |
|---|---|---|---|
| **G1** | **Layered timeline view** — per-layer × time × pitch representation (the "DAW arrange/piano-roll view" as data) | Agent must *see* the composition to edit it like a producer; today it only gets a flat JSON list | `resources.rs` (`TimelineView`), new `get_timeline_view` tool, `ss timeline` CLI |
| **G2** | **Octave-range discipline** — each layer declares a register (e.g. bass 1–2, lead 5–7); compiler clamps/validates | Prevents the exact muddy, same-sounding mixes the user heard; encodes §1.3 | domain (`Layer.register`), planner, validator check, `set_layer_register` tool |
| **G3** | **Repeat / clone / pattern-arranging primitives** — "write this loop, repeat it ×16 with octave lift at the drop" | The fundamental DAW action (loop duplication across the timeline); today each instance must be hand-placed | `repeat_pattern_in_segment`, `clone_pattern_transposed`, `extend_arrangement` tools |
| **G4** | **Automation lanes** — per-layer/per-param curves (filter cutoff, volume, pan, FX sends) | Static music is boring; producers automate everything | domain `Automation` entity (§8.1), timeline `ControlChange` emission, `write_automation` tool, validation |
| **G5** | **FX chain / mixer buses** — EQ, compression, reverb, delay, sidechain per track | The "production" in production-grade; today the mixer is gain/pan only | domain `FxChain` + `MixerState` (§8.1), `audio/src/fx.rs` (biquad EQ, comp, reverb, delay), `set_track_fx`, `set_track_mix` tools |
| **G6** | **Arrangement scaffolding** — intro/build/drop/breakdown/outro templates with correct bar counts + energy arc | Agents currently get whatever `plan_arrangement` picks; producers think in section archetypes | `arrangement_template` tool + golden section shapes per genre |
| **G7** | **Reference-track calibration** — import a reference's spectral profile (from `ss analyze`) and gate the mix against it | "Production grade" is defined by genre references; the audit (§D′) gives us the fingerprint machinery already | `calibration` + render gate extension (E′ plan) |

---

## 3. Design — the layered timeline view (G1)

### 3.1 `TimelineView` resource (JSON — the agent's "arrange view")

```jsonc
{
  "project_id": "...",
  "bpm": 124,
  "total_bars": 64,
  "sections": [                         // arrangement rows (intro/build/drop/…)
    { "name": "intro",   "start_bar": 0,  "bars": 16, "energy": 0.25 },
    { "name": "build",   "start_bar": 16, "bars": 16, "energy": 0.55 },
    { "name": "drop",    "start_bar": 32, "bars": 16, "energy": 1.0 }
  ],
  "layers": [                            // piano-roll rows, one per layer
    {
      "layer_id": "…", "name": "Lead", "instrument": "bell_synth",
      "register": { "min_octave": 5, "max_octave": 7 },   // G2
      "notes": [
        { "start_tick": 0, "pitch": 84, "name": "C6", "duration_ticks": 480, "velocity": 100 }
      ],
      "automation": [ { "param": "cutoff", "points": [ { "tick": 0, "value": 0.2 }, … ] } ]  // G4
    }
  ],
  "patterns": [ { "id": "…", "name": "Lead_Arp", "instances": [ { "segment": 2, "start_tick": 0, "transpose": 12 } ] } ]
}
```

Implementation: pure derivation from `Project` + `compile_project()` — no new storage.
`get_timeline_view(project_id, segment_scope?)` MCP tool → receipt with the view JSON;
`ss timeline <path> [--segment N] [--layer name]` prints a compact ASCII render
(section ruler + per-layer note lanes with pitch octaves + automation summary) so a human
and an agent both get the picture.

### 3.2 The ASCII render (CLI)

```
bar:  0         8         16        24        32        40
sec:  intro     build     drop      break     drop2     outro
Drums ██░░██░██░░██░██░█░░░░ ██████ ░░░░░░ ██████ ░░░░░░  [kick1/2, hat off]
Bass  ░░░░░░░░░░████████████ ██████ ░░░░░░ ██████ ░░░░░░  [oct 1-2]
Pads  ░░░░░░░░░░░░░░░░░░████ ██████ ██████ ░░░░░░ ░░░░░░  [oct 3-5]
Lead  ░░░░░░░░░░░░░░░░░░░░░░ ██████ ░░░░░░ ██████ ░░░░░░  [oct 5-7]
```

---

## 4. Design — octave discipline (G2)

### 4.1 `LayerRegister` on `Layer`

```rust
pub struct Register { pub min_octave: i8, pub max_octave: i8 }  // e.g. bass 1..=2
```

- `plan_project` populates registers per genre (techno bass 1–2, lead 5–7, etc.) from a
  genre table (embeds §1.3).
- **Compiler**: `write_pattern_notes`/`place_pattern_in_segment` clamp or reject notes
  outside the layer register (configurable: `clamp` vs `reject`), with a diagnostic.
- **Validator**: new `register_check` — notes outside register → warning
  `LAYER_NOTE_OUTSIDE_REGISTER` (metric `layers.register_violations`), feeding the golden
  gate.

### 4.2 Tool: `set_layer_register(layer_id, min_octave, max_octave)`

Agent tells the system "bass lives in octave 1–2, lead in 5–7" and the whole pipeline
respects it — exactly the UX the user described ("bass from octave N to N, lead from N…").

---

## 5. Design — repeat / clone / arrange (G3)

DAW parity: FL Studio "paint pattern across playlist", Ableton `Ctrl+D` duplicate.

| Tool | Behavior |
|---|---|
| `repeat_pattern_in_segment(layer_id, pattern_id, count, start_tick, step_ticks, transpose_step?)` | Place `count` instances spaced `step_ticks` apart; optional per-instance transpose ramp (e.g. +12 over 8 bars = octave lift at the drop). Returns the placed `PatternInstance`s. |
| `clone_pattern_transposed(pattern_id, semitones, name?)` | Copy a pattern shifted by N semitones (a new pattern id — the DAW "duplicate & pitch up" action). |
| `extend_arrangement(total_bars)` | Grow the arrangement with a template section (G6) — the "make it 64 bars" action. |

These compose into the workflow the user described: *"setting up the first octave, then
repeating it x times… other layers start from N octave and end till N."*

---

## 6. Design — automation lanes (G4)

```rust
pub struct Automation {                       // §8.1 entity
    pub layer_id: LayerId,
    pub param: AutomationParam,               // Cutoff | Volume | Pan | Send(reverb|delay) | …
    pub points: Vec<AutomationPoint>,         // (tick, value 0..1), linear interpolation
}
```

- Domain entity + `write_automation(layer_id, param, points)` tool (append/replace lane).
- Compiler emits `TimelineIR::EventKind::ControlChange { controller, value }` per lane
  (CC mapping per param, e.g. cutoff=74, volume=7, pan=10, reverb send=91, delay=93).
- Audio renderer consumes CC lanes: cutoff → synth filter cutoff over time (this is what
  makes build-ups open up), volume/pan → mix automation, sends → reverb/delay bus levels.
- Validator: `check_automation_range` already exists for energy; generalize to all lanes.

---

## 7. Design — FX chain & mixer buses (G5)

### 7.1 `FxChain` + `MixerState` (per §8.1, architecture line ~900)

```rust
pub struct FxChain {                          // per track
    pub high_pass_hz: f32,                    // HPF 30 / 100 / 120
    pub eq: [FreqBand; 3],                    // low-mid scoop, presence, air shelf
    pub compressor: Option<CompressorCfg>,    // threshold, ratio, attack, release, makeup
    pub sidechain_from_kick: bool,            // the electronic-music staple
    pub sends: SendLevels,                    // reverb / delay send amounts
}
pub struct MixerState { pub gain_db: f32, pub pan: f32 }   // extends existing Track fields
```

### 7.2 Audio implementations (`audio/src/fx.rs`, deterministic, tested)

| Effect | Implementation |
|---|---|
| HPF/LPF + bell EQ | biquad (RBJ) filters — already have 2-pole LP; add HP/peaking |
| Compressor | feed-forward gain computer (detector + attack/release smoothing), 1-VU makeup |
| Sidechain | envelope follower driven by the kick track; duck the target bus |
| Reverb | Schroeder–Moorer (4 comb + 2 all-pass), deterministic |
| Delay | ping-pong delay with feedback + LP dampening |
| Limiter | already exists (soft-knee) — reuse on master and buses |

### 7.3 Tools

- `set_track_fx(track_id, fx: FxChain)` — "high-pass the pads at 120 Hz, scoop 350 Hz −2 dB,
  sidechain to kick, send 0.3 to reverb".
- `set_track_mix(track_id, gain_db, pan)` — the mixer-fader action.
- `apply_genre_mix_preset(genre)` — one-shot: gain staging + per-instrument HPF/EQ/sidechain
  defaults from the genre table (so an agent can go from loop → production-ready mix in one
  call, then refine).

---

## 8. Design — arrangement scaffolding (G6)

- `arrangement_templates(genre)` resource: intro/build/drop/breakdown/outro bar-count
  archetypes per genre (from §1.2), with energy arc + which layers are active in each
  section.
- `apply_arrangement_template(genre, total_bars)` — build the section skeleton + energy
  curve; the planner then fills each section's layers (extends `plan_project`).
- Keeps the golden invariant: sections must stay within validator transition/energy gates.

---

## 9. Design — reference-track calibration (G7)

- `import_reference_profile(track_name, genre)` — the user runs `ss analyze reference.wav`,
  and the resulting spectral fingerprint (band energies, centroid, aliasing, LUFS target)
  is stored as the project's genre reference.
- Render gate (E′ plan §2.1 item 3) then measures deviation of the render's audit from the
  *reference* profile, not just the built-in genre average → "production grade" becomes
  measurable against a real track.

---

## 10. New MCP tool catalog (summary)

| Tool | Params | Plane |
|---|---|---|
| `get_timeline_view` | project_id, segment? | G1 |
| `set_layer_register` | layer_id, min_octave, max_octave | G2 |
| `repeat_pattern_in_segment` | layer_id, pattern_id, count, start_tick, step_ticks, transpose_step? | G3 |
| `clone_pattern_transposed` | pattern_id, semitones, name? | G3 |
| `extend_arrangement` | total_bars, template? | G3/G6 |
| `write_automation` | layer_id, param, points[] | G4 |
| `set_track_fx` | track_id, fx | G5 |
| `set_track_mix` | track_id, gain_db, pan | G5 |
| `apply_genre_mix_preset` | genre | G5 |
| `apply_arrangement_template` | genre, total_bars | G6 |
| `import_reference_profile` | track_name, genre | G7 |
| `arrangement_templates` (resource) | genre | G6 |

All follow the existing `ToolReceipt` contract (status/summary/score_delta/evidence_uris/
diagnostics), get registered in `dispatch.rs` + `capabilities.rs`, and are pure/deterministic.

---

## 11. CLI additions

| Command | Purpose |
|---|---|
| `ss timeline <path> [--segment N] [--layer name]` | ASCII layered timeline (G1) |
| `ss layer <path> set-register <layer> <minOct> <maxOct>` | G2 |
| `ss pattern <path> repeat <layer> <pattern> <count> [--step]` | G3 |
| `ss automation <path> write <layer> <param> <jsonPoints>` | G4 |
| `ss mix <path> fx <track> <jsonFx>` / `ss mix <path> preset <genre>` | G5 |
| `ss template <path> apply <genre> <bars>` | G6 |

---

## 12. Test matrix

| # | Test | Where |
|---|---|---|
| 31 | `timeline_view_reflects_layers_segments_and_octaves` | mcp |
| 32 | `layer_register_clamps_out_of_range_notes` (compiler) + `register_violations` diagnostic (validator) | compiler/validator |
| 33 | `repeat_pattern_places_n_instances_with_transpose_ramp` | mcp |
| 34 | `clone_pattern_transposed_shifts_all_notes` | mcp |
| 35 | `automation_lane_emits_control_change_events` (compiler) + renderer applies cutoff sweep | compiler/audio |
| 36 | `fx_chain_hpf_scoop_sidechain_alter_spectrum` (audio unit) | audio |
| 37 | `genre_mix_preset_moves_render_toward_reference_profile` | eval/mvas |
| 38 | `golden_gates_stay_green_with_register_and_automation` | eval |

Every slice runs the full workspace gate (700+ tests, 0 clippy, fmt) and commits+pushes.

---

## 13. Iteration order (each ships → tests → review → commit → push)

| Iteration | Scope | Exit criteria |
|---|---|---|
| **G′1** | G1 timeline view: `TimelineView` resource + `get_timeline_view` + `ss timeline` + tests | `ss timeline` shows sections × layers × octave registers; tests 31 green |
| **G′2** | G2 register discipline + G3 repeat/clone/arrange tools | octave violations flagged; `repeat_pattern` places N instances; tests 32–34 green |
| **G′3** | G4 automation lanes (domain → timeline CC → renderer) | cutoff/volume automation audible + validated; tests 35 green |
| **G′4** | G5 FX chain + mixer (audio/fx.rs: HP/EQ/comp/sidechain/reverb/delay) + mix presets | per-track FX alters spectrum provably; tests 36–37 green |
| **G′5** | G6 templates + G7 reference calibration + gate integration | arrangement templates produce section shapes; render gate measures vs reference; tests 38 green; full DoD |
| **F′** (fold-in) | `ss demo`, `--segment` render, CI | full-scope plan DoD |

**Definition of done (this plan):** an agent can go from brief → arranged multi-section
track with octave-disciplined layers, repeated/mutated patterns, automation, per-track FX,
mixed against a genre reference — with the whole state *visible* via `ss timeline` — and the
result passes the spectral + golden gates. Workspace 700+ tests green, 0 clippy, fmt clean,
all iterations committed and pushed.

---

## 14. Traceability

- **Architecture §8.1** "musical entities": `Automation`, `FxChain`, `MixerState` — this plan
  implements exactly these (G4, G5).
- **Architecture §8.2/§4.2**: every new tool returns `ToolReceipt`; states unchanged.
- **Architecture §11 planes**: G1 is the L3 timeline plane made *visible*; G2/G4 add L3
  checks; G5 extends L4; G7 extends L6 (render regression against reference).
- **Architecture §14 (ops list)**: `add_sidechain_pump` → G5 `sidechain_from_kick`;
  `reduce_low_mid_mud` → G5 EQ scoop; `transpose_pattern` → exists, G3 composes it.
- **Audio-quality plan (A′–F′)**: D′ gave us the spectral audit that G7 reuses; E′ gates are
  the enforcement point for G2/G4/G5/G7.
