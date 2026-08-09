# Real-Instrument Synthesis Investigation

> **Purpose:** diagnose why every genre (incl. the "rock"/nu-metal Linkin-Park demo) comes out
> sounding like layered water-drop/sinewave/electronic synth, investigate how to build *real
> instrument* synthesis infrastructure, survey public Rust crates, and identify what the current
> infrastructure is missing.
>
> **Method:** source audit of `scorestrata-audio` (synth/sampler/samplebank/render/fx),
> `scorestrata-compiler` (plan.rs rock/nu-metal instrumentation), crates.io + docs research on the
> Rust audio ecosystem. Read-only — no source changes made in this pass.
>
> **Date:** 2026-08-10 · **HEAD:** `b6014ed`

---

## 1. Root-cause diagnosis — why it sounds synthetic

Every melodic layer — guitar, bass, lead, pad, bell, brass — is rendered by **one engine**:

```
render.rs::voice_params_for_instrument(instrument) -> SynthParams   (line 1133)
```

`SynthParams` is a **subtractive oscillator voice**: waveform (sine/saw/square/triangle) → optional
detuned 2nd osc + sub + noise → tanh drive → resonant low-pass → ADSR. There is *no* string
physics, *no* pick transient, *no* body/cabinet resonance, *no* sample playback.

The evidence, directly from the code:

| Layer | Instrument | Actual voice | Result |
|---|---|---|---|
| Guitar | `distorted_guitar` | **Saw osc + 0.85 tanh drive** + noise mix (render.rs:1165) | "synth with distortion", not a guitar |
| Bass | `bass_synth` | Detuned saw pair + sine sub (render.rs:1145) | classic synth bass |
| Lead | `lead_synth` | Square osc + drive + vibrato (render.rs:1206) | synth lead |
| Drums | `drum_sampler` | Procedural one-shots: sine-sweep kick, noise+190 Hz ring snare, HP-noise hats (samplebank.rs) | "synth drums", not a rock kit |

**There is no Karplus-Strong plucked string anywhere** — the plan's §3.3 `physical.rs` was never
implemented. **There is no multisample engine** — no key zones, velocity layers, round-robins, or
loop points. **There is no amp/cabinet modeling** — no preamp distortion, no cabinet impulse
response (ironically, `convolve.rs` — partitioned convolution — *exists* and is used only for
reverb).

The user's ear is exactly right: it is an electronic-music stack labelled with rock instrument
names.

---

## 2. What the plan promised vs. what exists

| Plan item | Promised (§) | Status |
|---|---|---|
| `physical.rs` — Karplus-Strong plucked string, fractional-delay tuning, membrane drums | §3.3 | ❌ never implemented |
| `instrument_resolution` pass — synth \| sampler (multisample + velocity layers + RR) \| physical | §3.2 | ❌ never implemented |
| SFZ sample support — key ranges, velocity layers, round-robins, loop points | §1.4 | ❌ never implemented |
| CC0 embedded sample banks via `include_bytes!` | §1.4 / §3.3 / R7 | 🟡 partially — procedural drum kits only, no melodic samples |
| Amp/cabinet modeling for electric guitar | §3.3 (implied by "physical" guitar) | ❌ not present (only a generic tanh drive) |
| `sound_bank` on `RenderConfig` | Phase A | ✅ but only routes *drum* layers (render.rs:1011) |

The gap is structural: **the audio crate has exactly one voice model** (`SynthParams`), and the
compiler's instrument names are cosmetic labels on top of it.

---

## 3. Public Rust crate landscape (verified)

### 3.1 Crates worth using

| Crate | License | Pure Rust | What it gives | Verdict |
|---|---|---|---|---|
| `hound` | MIT/Apache-2.0 | ✅ | Robust WAV read/write (multi-format, seek) | **Add** — sample loading; replaces our ad-hoc `read_wav_stereo` for libraries |
| `symphonia` | MPL-2.0 | ✅ | FLAC/Ogg/MP3/WAV decode | Optional — only if we ship compressed sample banks (MPL is file-level copyleft; fine for internal assets) |
| `rubato` | MIT | ✅ | High-quality polyphase resampling | **Add** — fractional pitch-shift of samples; also unlocks the plan's 96 kHz mode |
| `fundsp` | MIT/Apache-2.0 | ✅ | Prebuilt biquads, Moog, waveshapers, noise, graph combinators | Optional — could accelerate amp/filter work, but we already have correct DSP primitives; prefer our own for byte-determinism |
| `dasp` | MIT/Apache-2.0 | ✅ | `Frame`/`Signal` traits, interpolation, SRC | Optional — utility only |
| `soxr` | LGPL-2.1 | ❌ (C bindings) | Gold-standard resampling | **Avoid** — license + C dep break R7 |
| SF2 (`rust-sf2`/`sf2`) | MIT-ish | ✅ | SoundFont parsing | 🟡 Immature; SF2 is a deep rabbit hole. The plan chose **SFZ** (plain text + WAV) — simpler and asset-native. Skip SF2. |

### 3.2 The important finding

**There is no mature Rust crate that does "real instrument synthesis" for us.** The DSP-heavy
real-instrument stack — plucked-string physical modeling, membrane drum modeling, amp/cabinet
emulation — has no turnkey Rust library. The ecosystem offers *building blocks* (resampling,
WAV loading, DSP primitives) but the physical-modeling and amp-modeling engines must be
**implemented in-house** (~300–500 lines of well-understood DSP). This matches the project's R7
posture (pure Rust, no external runtime deps) and R1 (byte-deterministic renders) — both are
achievable because these algorithms are pure math.

---

## 4. Infrastructure gap analysis

Current audio-crate capability inventory:

| Capability | Have? | Notes |
|---|---|---|
| Oscillator subtractive voice | ✅ | 4 waveforms, polyBLEP, 2× oversampled drive |
| ADSR + filter env | ✅ | Time-based, rendered release |
| Drum one-shots (procedural) | ✅ | 5 voices, 5 kits |
| Partitioned convolution | ✅ | `convolve.rs` — currently reverb-only |
| Bus graph / sends / master chain | ✅ | Phases B/D |
| WAV read (single) | ✅ | `wav.rs::read_wav_file` (stereo, for reference tracks) |
| **Plucked-string physical model** | ❌ | THE missing piece for guitar/bass |
| **Membrane/waveguide drum model** | ❌ | The missing piece for real-feeling drums |
| **Multi-sample instrument engine** | ❌ | key zones, velocity layers, RR, loops, pitch-shift |
| **Amp/cabinet model** | ❌ | preamp saturation + cabinet IR convolution |
| **Embedded melodic sample bank** | ❌ | only 5 drum voices |
| **`instrument_resolution` pass** | ❌ | compiler always emits `SynthParams` |

---

## 5. Recommended architecture — three voice engines + amp model

```
Layer.instrument
  ├── synth     → SynthParams            (existing; unchanged, R1-safe)
  ├── physical  → physical.rs            (NEW: Karplus-Strong + membrane drums)
  │                └── routed through per-layer FX chain (existing)
  └── sampler   → multisample.rs         (NEW: SFZ-style key zones / vel layers / RR)
                   └── pitch-shift via rubato (fractional) or linear (whole-step fallback)

Electric guitar path (the rock gap):
  physical KS string (pluck + palm-mute) → preamp/amp model (asymmetric soft clip)
    → cabinet IR (convolve.rs, embedded/generated IR) → per-layer FX → mix
```

### 5.1 `physical.rs` — Karplus-Strong plucked string
- Noise-excited delay line with one-zero low-pass in the feedback loop (classic KS).
- **Fractional-delay tuning** (linear/allpass interpolation of the delay length) for correct pitch.
- Pluck position (fixed noise burst + position-dependent notch) — gives the "twang".
- **Palm-mute mode** (articulation-driven): much shorter delay decay + low-pass clamp → chug.
- Deterministic: seeded excitation, pure `f32` math, no hidden state (R1/R2).
- Extends naturally to **membrane drums**: two delay lines + bridge coupling (or a banded
  waveguide) → real-feeling toms/snares.

### 5.2 `multisample.rs` — sample instrument engine
- Zone model: `key_min/key_max`, `vel_min/vel_max`, `loop_start/loop_end`, `root_note`,
  `round_robin` — the SFZ subset the plan named.
- Playback: deterministic resampling (rubato polyphase for fractional pitch, linear for coarse).
- Bank loading: embedded CC0 WAVs via `include_bytes!` (R7) or procedurally generated
  multisamples layered from `physical.rs` (zero-asset fallback).

### 5.3 `amp.rs` — electric guitar amp/cabinet
- Preamp: asymmetric soft-clip (triode-ish transfer) + gain staging + presence EQ.
- Cabinet: one embedded/generated cabinet IR run through the existing `convolve.rs`.
- This is what turns "saw + tanh" into "guitar through an amp".

### 5.4 Compiler `instrument_resolution` pass (§3.2)
- Map layer roles → voice model: `distorted_guitar → physical(guitar, palm_mute?)`,
  `bass_synth → physical(bass)`, `lead_synth → synth`, `drum_sampler → physical drums or sampler kit`.
- Defaults must keep current instruments on `synth` so existing renders stay byte-identical (R1).

---

## 6. Proposed implementation phases

| Step | Scope | Gate |
|---|---|---|
| 1 | `physical.rs`: Karplus-Strong plucked string + tests (pitch accuracy, decay, determinism, palm-mute) | validator/audio tests green |
| 2 | `amp.rs`: preamp + cabinet IR via `convolve.rs` + tests (harmonic content, rolloff) | audio tests green |
| 3 | Wire `voice_model` selection in render + compiler `instrument_resolution` (guitar/bass → physical; R1 default kept) | full workspace green + golden byte-identity |
| 4 | `multisample.rs`: SFZ-style engine + `rubato` pitch-shift + embedded/CC0 melodic bank (or procedural KS multisamples) | audio tests green |
| 5 | Re-run the Linkin-Park demo with the new stack; spectrogram + LUFS audit | `ss demo` green |

---

## 7. Constraints honored

- **R1** — byte-identical legacy renders: new engines are opt-in per instrument; defaults unchanged.
- **R2** — determinism: all new DSP is seeded, pure `f32`, no wall-clock/time state.
- **R7** — no external runtime deps: new crates pure-Rust (`hound`, `rubato`); assets embedded via
  `include_bytes!` or generated; no C bindings (`soxr` excluded).
- **Licensing** — MIT/Apache-2.0 only for new deps; CC0/MIT assets only if embedded.
