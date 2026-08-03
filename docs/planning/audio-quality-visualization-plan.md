# Audio Quality & Post-Generation Visualization Plan

**Status:** Proposed (planning)
**Owner:** scorestrata engineering loop
**Date:** 2026-08-03
**Supersedes/extends:** `docs/planning/full-scope-audio-testing-plan.md` (kept; this doc layers the
*quality* and *auditing* work onto its per-phase + validation iterations)
**Related spec:** `docs/architecture/architecture.md` §11 (audio), §12 (diagnostics), §16 (test
pyramid), §17 (reproducibility)

---

## 0. Executive summary

A live **listening** test (first time the output was actually heard, not just measured) found the
music **distorted, same-y across genres, and with an inaudible lead**. This plan is grounded in a
full **root-cause investigation** — including an empirical **spectral audit of the rendered WAVs**
(FFT analysis, spectrogram/waveform PNGs, per-stem spectra) that *proves* each defect and doubles
as the blueprint for the **post-generation visualization system** the user asked for.

The headline finding: **the instrument-identity layer is dead code.** `render_timeline_to_analysis`
calls `render_notes(..., &[])` — an empty voice table — so *every* layer (drums, bass, lead, every
genre) silently renders with the **same default voice**: a raw, aliasing **sawtooth** with a heavy
low-pass. Drums are therefore saw buzzes at 64–110 Hz, the lead is a 10 dB-quieter muffled saw, and
"techno / house / ambient" differ only in BPM and key — because `plan_project` builds the *same*
three patterns for every genre. The music cannot be anything but "distorted synth techno" until the
engine and the planner are fixed.

---

## 1. Root-cause analysis (empirically verified)

### 1.1 Evidence — spectral audit of `demo_techno` (from the live render)

| Artifact | Peak (dBFS) | Clipped | RMS (dB) | Spectral centroid | Top peaks | Meaning |
|---|---|---|---|---|---|---|
| **master.wav** | **0.00** | **204 smp** | −11.8 | 3233 Hz | 29–94 Hz | **Hard clipping**; 6.4% energy >16 kHz (aliasing) |
| **Bass stem** | −0.40 | 0 | −9.4 | 2302 Hz | 29–64 Hz | Pure sub-bass **saw buzz** |
| **Drums stem** | −3.23 | 0 | −12.2 | 2483 Hz | **64–94 Hz** | "Drums" are **low-frequency tones**, not drums |
| **Lead stem** | −4.05 | 0 | **−17.9** | 5626 Hz | 521–1049 Hz | **10 dB below bass** → inaudible; 12% aliasing |

Kick transient isolation: dominant freqs **70 / 59 / 129 Hz** → a **sine/saw blip with no noise,
no pitch-sweep, no transient** — not a real kick.

### 1.2 The five root causes

| # | Root cause | Code | Effect heard |
|---|---|---|---|
| **R1** | **Empty voice table** — `render_timeline_to_analysis` passes `&[]` to `render_notes`; every layer falls back to `SynthParams::default()` | `mcp/src/tools.rs` `render_timeline_to_analysis`; `audio/src/render.rs` `render_layer_buffer` | **Every instrument is the same timbre.** `RenderVoice` / `waveform_for_instrument` (Saw→bass, Square→lead, Triangle→pad, Sine→drums) is **dead code** — referenced only in tests. This is THE reason genres and instruments are indistinguishable. |
| **R2** | **Naive non-band-limited oscillators** — `Saw = 2φ−1`, `Square = ±1`, `Triangle` naive | `audio/src/synth.rs` `oscillator` | Aliasing at 48 kHz → harsh "digital" buzz/distortion (6–12% energy >16 kHz measured). |
| **R3** | **Envelope is not a real ADSR** — normalized to *note length* (`t = i/len`), **`release` is never used**, note tails hard-truncate at buffer end, `decay`/`sustain` phases are wrong-shaped | `audio/src/synth.rs` `render_note` | **Click/pop on every note end** (the worst "distortion" source); drums sound like blips, not hits. |
| **R4** | **No limiter / headroom** — layers summed at 0.8 gain × N with no bus limit; WAV writer clamps to ±1.0 | `audio/src/render.rs`, `audio/src/wav.rs` | **204 hard-clipped samples**, true peak +1.8…+3.7 dBTP (matches the earlier measured overs). |
| **R5** | **Genre-blind planner** — `plan_project` → `build_patterns` emits the *same* 4-on-floor drums, root-fifth bass, 8th-note arpeggio for **every genre**; genre only changes BPM/key/bars | `compiler/src/plan.rs` | Techno/house/ambient all "sound like synth techno". No pads/drones for ambient, no chord progressions, no genre drum kits. |

Secondary: the one-pole "filter" is not a real filter (`cutoff` semantics inverted vs. docstring,
`resonance` is a blend not a resonant peak); drums use MIDI-note-as-frequency (snare=100 Hz tone,
hat=85 Hz tone) because `drum_sampler` has no sampler; lead is low-passed at cutoff 0.8 AND 10 dB
quieter; mono fold collapses the existing stereo mixer.

### 1.3 The "lead is blank" explanation

Lead events exist (stem has content) but the stem is **−17.9 dB RMS vs −9.4 dB bass** (≈10 dB gap,
×10 in amplitude) and its square-wave intent was replaced by a heavily low-passed saw. Against
clipped, loud drums+bass in mono, the lead is inaudible. Fixes: proper timbre + envelope, gain
staging, and stereo separation (pan), all below.

---

## 2. Post-generation music visualization as debug logs (the brainstorm)

The audit above *is* the visualization concept — but done in throwaway Python. The system should own
it: a first-class, **deterministic, testable** Rust analysis + visualization module so every render
produces its own "debug log" the way `timeline.json` does today.

### 2.1 Design — `scorestrata_audio::audit` (new module) + `ss analyze <wav>`

| Capability | Implementation | Output artifact |
|---|---|---|
| Time-domain stats | DC offset, peak, RMS, crest, clipped-sample count (already in `analysis.rs` — extend) | text log |
| **Spectral analysis** | **`rustfft`** STFT; per-band energy (sub/bass/mid/high), spectral centroid/rolloff, **aliasing energy** (>16 kHz share), silence detection | text log + metrics |
| **Spectrogram PNG** | **`plotters`** bitmap of the STFT (log-mag, magma-ish colormap, 0–12 kHz) | `audit/<artifact>.spectrogram.png` |
| **Waveform PNG** | `plotters` polyline of the buffer (first N seconds + zoom) | `audit/<artifact>.waveform.png` |
| **Spectrum overlay PNG** | averaged per-stem spectra on one plot → **see instrument separation / genre fingerprints** at a glance | `audit/spectra_overlay.png` |
| **Genre fingerprint** | expected spectral shape per genre (drums-dominant techno, pad-heavy ambient, plucky house) — deviation becomes a diagnostic | `GenreFingerprint` in analysis |
| **Text debug log** | the `AUDIT:` block from §1.1, written per artifact next to its manifest | `audit/<artifact>.txt` |

### 2.2 Why this is the debugging workflow

1. `ss analyze render/master.wav` prints the audit log + writes the PNGs → **one command, human-readable debug log**.
2. Every `render_project` / `ss render` receipt gains `render.audit` (spectral centroid, aliasing %, DC, clipped count) and `audit_uri` → the agent loop and the user both see *why* a render sounds bad, in metrics.
3. The **run-time post-generation gate** (§4.2 of the existing plan) is extended: `clipping == 0`, `aliasing_energy < 5%`, `|dc| < 0.01`, `silence_ratio < 5%`, `genre_fingerprint_deviation < threshold` become hard/soft gates feeding `render.verified`.
4. The MVAS simulation adds a **step that renders, runs the audit, and asserts the gate** — so "does it sound right" is a test, not an ear.

### 2.3 The spec traceability

- New traceability rows: `SPEC-AUDIO-AUDIT` (§11.4 analysis), `SPEC-AUDIO-VISUALIZATION` (new §11.4 extension), `SPEC-AUDIO-GENRE-FINGERPRINT` (§11.8 genre calibration).

---

## 3. Crate / library investigation (user request)

The audio crate today has a deliberately minimal dep set (`serde`, `thiserror`, `sha2`, `hex`) and a
project-wide ethos of deterministic, hand-rolled, dependency-light DSP. Findings and recommendation:

| Need | Recommended | Why | Alternative |
|---|---|---|---|
| Anti-aliased oscillators (saw/square/triangle) | **Hand-rolled polyBLEP** (~40 lines, deterministic, no dep) | Matches project ethos; naive waveforms are the *bug*, not the library choice; `fundsp` graphs are streaming-oriented and heavier | **`fundsp`** (mature DSP graph lib, Sander Mertens) if we want richer voices quickly |
| Envelope / filter | **Hand-rolled** — time-based ADSR with release + proper tail, and a 2-pole biquad low-pass (~50 lines) | Fully deterministic, unit-testable, zero deps | `dasp` (RustAudio primitives: `dasp_signal`, `dasp_envelope`) |
| **FFT / spectral analysis** | **`rustfft`** (pure Rust, very mature, actively maintained; powers symphonia) | Hand-rolling FFT is genuinely not worth it; deterministic; needed for §2.1 | `realfft` (thin wrapper, same author, real-input optimized) |
| WAV I/O | **Keep hand-rolled, extend to stereo** (~15 lines: interleave L/R + channels=2) | Already correct; stereo is trivial; zero dep | **`hound`** (mature, minimal, multi-channel/24-bit) if we later need 24-bit/float |
| Spectrogram/waveform **PNG** | **`plotters`** (pure Rust, deterministic bitmap output, mature) | Deterministic, testable image generation; no Python in the loop | `image` crate direct (manual pixel writes) |
| Playback (optional) | **`rodio`** (`ss play`) | Mature cpal-based player — nice-to-have, not required for the pipeline | — |
| Dev-time external audit | `sox stat`, `ffmpeg ebur128/astats`, Python `numpy/scipy/matplotlib` (all verified installed) | The §1.1 audit was run with these; good cross-check for CI | `librosa`/`essentia` (pip, heavier) |

**Verdict:** synthesis stays hand-rolled (fixed properly — R2/R3), **`rustfft` + `plotters` are the
two new dependencies** (analysis + visualization), WAV stays hand-rolled with stereo support,
`hound`/`rodio` are optional conveniences. This keeps determinism guarantees (§17) intact and adds
no runtime deps to the render path that aren't needed.

---

## 4. Fix stack (extends F1–F4 of the full-scope plan)

| Fix | Root cause | Scope | Deps |
|---|---|---|---|
| **F5 — Real synth voices** | R1, R2, R3 | `audio/src/synth.rs` rewrite: polyBLEP oscillators; **time-based ADSR with release + note-tail extension** (notes render attack/decay/sustain/release over absolute time; buffer extends by release); 2-pole biquad low-pass with correct cutoff semantics; per-`Waveform` char (Saw/Square/Triangle/Sine + Noise) | none |
| **F6 — Wire instrument identity** | R1 | Build the voice table in `render_timeline_to_analysis` from layer **instrument** (drum_sampler / bass_synth / lead_synth / pad) + genre; delete the dead `waveform_for_instrument` path or repoint it; every layer gets an explicit voice | none |
| **F7 — Real drum synthesis** | R1, R5 | `audio/src/synth.rs` percussion: **kick** = sine with pitch-sweep (120→45 Hz) + short decay; **snare** = noise burst + 180 Hz tone; **hat** = high-passed noise (short decay); clap = layered noise. Map MIDI-note→drum-kind (kick/snare/hat/open-hat) instead of tone frequency | none |
| **F8 — Limiter + headroom** (≈ F1) | R4 | `audio/src/mastering.rs`: soft-knee peak limiter, true peak ≤ −0.5 dBTP; per-layer gain staging (drums 0.9, bass 0.7, lead 0.6, pad 0.5); applied on the stereo bus | none |
| **F9 — Stereo render** (≈ F2) | — | wire the existing `Mixer` pan (layer-name hash → deterministic pan); stereo WAV (channels=2); keep mono default | none |
| **F10 — Genre-differentiated content** | R5 | `compiler/src/plan.rs`: per-genre **drum kits** (techno 4/4 kick+offbeat hat, house shuffle, ambient sparse hits), **bass movement**, **lead density**, **pad/drone layer** for ambient, **chord progressions** (i–VI–III–VII), per-genre velocity maps; `testkit` fixtures get genre-accurate patterns | none |

F1–F4 from the existing plan remain (per-segment render, `ss demo`, run-time gates, CI) and slot
after F5–F10 because the engine must sound right before we export it everywhere.

---

## 5. Genre differentiation spec (the "why does everything sound the same" fix)

| Dimension | melodic_techno | house | ambient |
|---|---|---|---|
| Drum kit | 4/4 kick, offbeat open hat, snare on 2&4, sparse clap | 4/4 kick, **shuffled** hats, clap on 2&4, rimshot fills | very sparse: kick on 1, soft hits, long tails |
| Bass | driving 8th-note root/fifth saw | root-octave groove, filtered | slow root drone, long sustain |
| Lead | square/saw arp, 8th notes, detuned | plucky short stabs, off-grid | sparse bell tones, long reverb-like tail |
| Pad/extra | chord stabs in breakdown | filtered chords | **evolving pad/drone layer** (Triangle + slow LFO) |
| Tempo/feel | 124, straight | 122, swung | 80, slow attack |
| Velocity map | accents on downbeat | shuffle weighting | low velocities, slow envelope |
| **Spectral fingerprint** | drums-dominant, high centroid | balanced, mid-heavy | pad-heavy, low centroid, low aliasing |

`GenreFingerprint` (in `analysis.rs`) encodes the expected band energy per genre; the audit gate
reports deviation → the agent loop can *repair toward* genre (diagnostic `GENRE_FINGERPRINT_DRIFT`).

---

## 6. Test matrix (additions to the existing plan's §6)

| # | Test | Crate | Iteration |
|---|---|---|---|
| 19 | `polyblep_saw_has_no_aliasing_above_16k` (spectral assert) | audio | A′ |
| 20 | `adsr_release_applies_and_no_tail_click` (last 100 samples decay to ~0) | audio | A′ |
| 21 | `render_notes_with_voices_uses_per_instrument_timbre` (lead ≠ bass bytes) | audio | A′ |
| 22 | `kick_has_pitch_sweep_snare_has_noise_energy` | audio | B′ |
| 23 | `limiter_clamps_and_no_clipped_samples` (replaces the clipping finding) | audio | A′ |
| 24 | `stereo_interleave_and_pan_separation` | audio | A′ |
| 25 | `techno_vs_ambient_timeline_differ_in_structure` (patterns not identical) | compiler | C′ |
| 26 | `genre_fingerprint_deviation_reported_for_wrong_timbre` | audio | D′ |
| 27 | `audit_spectrogram_png_is_valid_png` (magic bytes) | audio | D′ |
| 28 | `ss_analyze_prints_audit_log_and_writes_pngs` | cli | D′ |
| 29 | `postgen_gate_fails_on_clipping_aliasing_dc` | mcp/audio | E′ |
| 30 | `mvas_step_38_render_audit_gate` (render → audit → assert gate) | eval | E′ |
| 31 | traceability rows `SPEC-AUDIO-AUDIT` / `SPEC-AUDIO-VISUALIZATION` / `SPEC-AUDIO-GENRE-FINGERPRINT` | eval | E′ |

---

## 7. Iteration order (each ships, tests, reviews, commits, pushes)

| Iteration | Scope | Exit criteria (hear + verify) |
|---|---|---|
| **A′** | F5 synth rewrite + F8 limiter/headroom + F9 stereo + F1 limiter tests | A techno render: **no clipping (peak ≤ −0.5 dBTP), no clicks, lead audible (RMS within 6 dB of bass), 0 aliasing >16 kHz**; existing 670 tests still green |
| **B′** | F7 drum synthesis + F6 voice wiring | Kick/snare/hat are audibly distinct; drums no longer sound like bass; spectral assert tests pass |
| **C′** | F10 genre-differentiated planner + testkit fixtures | `ss demo` techno/house/ambient are **audibly distinct** (structure + fingerprint tests pass) |
| **D′** | §2 audit module (`rustfft` + `plotters`) + `ss analyze` | `ss analyze` prints audit log + writes PNGs; PNG validity test passes; receipts carry `render.audit` |
| **E′** | Post-generation gate (existing §4.2 extended with audit metrics) + MVAS step 38 + traceability rows | Every artifact verified on write; step 38 green; full workspace gate green |
| **F′** | Remaining existing-plan work: `--segment`, `ss demo` command, CI | full-scope plan's Definition of Done |

**Definition of done (this plan):** `ss demo` produces techno/house/ambient you can *tell apart by
ear*, with no distortion/clicks; `ss analyze` gives a per-artifact spectral debug log + PNGs;
post-generation gates catch any regression; workspace 700+ tests green, 0 clippy, fmt clean, all
pushed.

---

## 8. Immediate next step (recommended)

Start **iteration A′** — the synth-engine rewrite (F5) plus limiter (F8). It is the root of the
distortion and the same-y sound; every later iteration (drums, genres, visualization) depends on a
clean, voice-aware renderer. Expected: ~2 focused code sessions with the usual
implement → test → gate → review → push loop.
