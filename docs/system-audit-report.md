# Scorestrata System Audit Report

> Status of the system architecture as of the S1–S4 hardening iterations
> (synthesis upgrade, layer-wise spectrograph audit, genre expansion).
> This document records what the system **is** today, where it still falls
> short of production-grade music generation, and the concrete next moves.

---

## 1. Executive summary

Scorestrata is a deterministic, offline music-composition + rendering
harness: an agent (via MCP tools) or operator (via the `ss` CLI) describes
a track, the planner turns that into a layered arrangement, the compiler
produces a timeline IR, and the audio engine renders a WAV with per-layer
FX, automation, and a mastered stereo mix — all reproducible (same input →
byte-identical output).

Three hardening iterations landed since the last report:

| Iteration | What shipped | Status |
|---|---|---|
| **S1** (`d52399e`) | Layered synth voices (detuned dual-osc, sub, noise, drive, filter env, vibrato), improved drums (click + saturation, fuller snare), 2× oversampled drive, richer per-instrument timbres | ✅ pushed, gate green |
| **S2** (`e29e701`) | **Layer-wise spectrograph audit**: per-layer spectral reports + per-layer spectrogram PNGs on the timeline, `ss audit` layer table, new `audit_layer_spectral` MCP tool | ✅ pushed, gate green |
| **S3** (`3a8081b`) | **Genre expansion**: 9 new planner families (jazz/hip-hop/dnb/cinematic/pop/funk/blues/synthwave/lo-fi), 13 new detection keywords + defaults, 12 genre fingerprints, 20 arrangement templates, 9 production playbooks, per-family mix chains | ✅ pushed, gate green |

**Validation:** `cargo test --workspace` **847/847 passing**, `cargo clippy
--workspace --all-targets` **0 errors**, `cargo fmt --check` clean, all
commits pushed. Working tree clean.

---

## 2. Current system status (what is established)

### 2.1 Pipeline (all phases operational)

```
brief ──▶ config_gen ──▶ planner ──▶ compiler ──▶ timeline IR ──▶ audio engine ──▶ WAV
                │            │              │                 │
              genre      segments/      deterministic     synth voices + FX +
            detection    patterns/      events (§17)      automation + mastering
            + defaults   layers (G2)                       (G4/G5)
```

- **Domain model** (`scorestrata-core`): `Project` (arrangement, mixer),
  `Segment` (role, bars), `Layer` (instrument, register, automation, FX,
  mix), `Pattern` + instances, `MixerState`, `SpectralReference`.
- **Planner** (`scorestrata-compiler`): **13 genre families** now produce
  distinct pattern sets (drums/bass/lead/pads) and layer compositions.
- **Compiler**: deterministic timeline IR with events sorted by tick,
  automation lanes (CC cutoff/volume), note-off resolution.
- **Audio engine** (`scorestrata-audio`): polyBLEP oscillators, layered
  voices, GM drum synth, insert FX chains (HPF/EQ/compressor/sends),
  sidechain, master bus (stereo width + gain), lookahead limiter (−1 dBTP
  ceiling), 48 kHz offline render.
- **Analysis**: LUFS, true peak, clipping, spectral centroid/rolloff,
  6-band energies, aliasing >16 kHz, **per-layer** spectral audit with
  spectrogram PNGs, genre-fingerprint deviation gate.
- **MCP** (`scorestrata-mcp`): ~60 tools registered (governance, project,
  composition, patch, validation, render, debug, learning), resources
  (capabilities, contracts, playbooks, arrangement templates, calibration
  profiles), dispatch + capabilities manifest.
- **CLI** (`scorestrata-cli`): `ss new/plan/compile/render/audit/
  timeline/arrangement plan/stems/segments` + per-layer audit output.
- **Eval** (`scorestrata-eval`): golden harness, MVAS simulation,
  production-pipeline DoD simulation, traceability ledger.

### 2.2 Genre coverage (S3)

- **Planner families (13):** techno, house, ambient, rock, jazz, hip-hop,
  dnb, cinematic, pop, funk, blues, synthwave, lo-fi + generic baseline.
- **Detection + defaults:** techno/melodic-techno, house, ambient,
  cinematic, trance, dubstep, lo-fi, drum-and-bass, nu-metal, rock,
  jazz, hip-hop, pop, funk, blues, synthwave, classical, reggae, country,
  folk, latin, gospel, electronic, world.
- **Fingerprints (12):** techno, house, ambient, jazz, hip-hop, rock,
  pop, funk, blues, synthwave, lo-fi, cinematic, dnb, classical.
- **Arrangement templates (20)** and **production playbooks (13)**.

### 2.3 What the demo proves today

Live runs on the current binary:
- **Nu-metal/rock** (`ss-rock3`, 104 BPM, E minor, 32 bars): 76 s render,
  −1.0 dBTP, no clipping, backbeat drums + palm-muted power-chord guitar +
  driving bass + vocal-range lead.
- **Jazz** (112 BPM, C major): swing ride, walking bass, horn line, pads —
  per-layer audit shows clean separation (lead 2.1 kHz, pads 613 Hz).
- **Trap** (92 BPM, E minor): boom-bap grid, 808-style bass, −1.0 dBTP,
  genre deviation 0.25 (moderate — close to the hip-hop profile).

---

## 3. Architectural gaps and limitations (honest audit)

### 3.1 Sound quality: "synthetic" timbre is only partially solved

**What improved (S1):** single static oscillator → layered voices
(detune, sub, noise, drive, filter envelope, vibrato); oversampled drive
removes foldback; drums got transients and saturation.

**What remains:**
1. **No samples.** The engine is 100 % synthesis. Production drums,
   real guitars, strings, and vocals are sample/IR-based in every DAW
   (FL Studio, Ableton). Synth-only kits read as "electronic" regardless
   of genre. **Next: sample-based percussion + IR convolution reverb.**
2. **No multi-sampled melodic instruments.** A "guitar" is a driven saw,
   a "bass" a detuned saw — timbre is constant across pitch, velocity,
   and articulation. Production-grade instruments vary across
   note/velocity zones. **Next: multi-layer multisamples or physical
   models for at least drums, guitar, bass.**
3. **No humanization/performance variation.** Every repeat of a pattern
   is byte-identical. Real performances have velocity/timing micro-
   variation. (Currently determinism is a *feature* for reproducibility;
   the plan should make humanization a deterministic-seeded *option*.)
4. **Limited articulation control.** No portamento, pitch bend, gliss,
   accent-layered hits, rolls/grooves beyond static steps.
5. **Mix is functional, not crafted.** Static per-layer gain staging +
   a master limiter. No true parallel compression, no bus groups, no
   sidechain *amount* automation beyond on/off, no multiband dynamics,
   no saturation on the master bus, no stereo *imaging* beyond mid/side
   width.

**Score: 4/10 on production-grade timbre.** Melodically and structurally
it is a real composition; sonically it is a high-quality chiptune.

### 3.2 Analysis/debug: layer-wise audit landed, but gaps remain

**What improved (S2):** `ss audit` now renders per-layer stems and prints
a per-layer spectral table + per-layer spectrogram PNGs; new MCP tool
`audit_layer_spectral`; shared `timeline_render_inputs` so master/stems/
audit paths convert identically.

**What remains:**
1. **No per-layer time-domain annotation** (which bar has the snare,
   where the kick pattern changes) on the spectrogram — the PNGs show
   heat but no bar/beat grid overlay.
2. **No reference-comparison.** `SpectralReference` exists as a type and
   deviation metric, but the audit does not yet compare *rendered* bands
   against a *user-provided reference track* spectrally — only against
   static genre fingerprints.
3. **No automated "fix loop".** The audit reports, but does not yet *propose
   or apply* the patch (e.g., "Bass layer 22 % too much low-mid → EQ −3 dB
   at 350 Hz"). The `suggest_fixes` tool surfaces validator patches, but
   not audio-engine patches from the layer audit.
4. **CLI audit is render-coupled.** `ss audit` re-renders stems each run
   (correct, but slow on long tracks); no caching keyed on input hash.

**Score: 6/10 as an agent debugging surface.** It answers "which layer is
wrong" — not yet "what exactly to change" or "does it now match the ref".

### 3.3 Agent UX / pipeline control

**What exists:** playbooks (guided workflows per genre), arrangement
templates, octave-register contracts (G2), pattern repetition primitives,
automation lanes, FX/mix tools, layer isolate/solo, timeline view,
incremental compilation.

**What remains:**
1. **No multi-take/A-B candidate workflow surfaced to agents** beyond
   `render_candidate_preview` (exists) — no simple "render A, render B,
   diff the layer audits" loop in the playbooks.
2. **No "audition" affordance** — the agent cannot *hear* (no playback
   tool); it relies on numbers. Playbook steps should end with
   "audit_layer_spectral → interpret → patch" instead of "render".
3. **Playbooks are static text.** They list tool names but do not *chain*
   calls with the actual parameter values from the current project.

**Score: 7/10.** Intuitive for a producer-agent that reads; missing the
iterative audition→patch reflex.

### 3.4 Other structural limits

1. **Mono stems only.** Stems are mono folds; stereo panning is baked
   into the master. No per-layer stereo stems or multichannel export.
2. **48 kHz only.** No 44.1/96 kHz or dithering modes; the limiter has no
   dither — fine for demo, limiting for mastering grade.
3. **No tempo/key changes mid-track.** Single BPM/key per project; no
   modulations, rubato, or time-signature changes within a track.
4. **No polyphony budget per voice** — notes are summed, so a dense
   chorus could mask; no anti-masking analysis (a `ReduceMasking` tool
   exists as a stub surface).
5. **Determinism vs. naturalism tension** — see §3.1.3.

---

## 4. Roadmap — next moves (priority order)

### P1 — Production-grade timbre (biggest lever on the complaint)
1. **Sample-based percussion.** Replace/augment GM drum synthesis with a
   small embedded drum-sample bank (kick/snare/hat/perc per genre family),
   pitched via the existing `drum_kind_for_pitch` routing. Big win for
   rock/hip-hop/pop realism.
2. **IR convolution reverb.** Add a tiny set of embedded impulse responses
   (room, plate, hall) and a `ConvolutionReverb` FX node. Pads/leads/drums
   instantly stop sounding "dry synthetic".
3. **Deterministic humanization.** A seeded micro-timing/velocity
   variation pass behind a `humanize` flag (seed = input hash, so
   reproducibility is preserved per seed).
4. **Master-bus polish:** soft-clip/saturation stage + true-peak-safe
   dithering at 16/24-bit.

### P2 — Iterative agent debug loop (the "producer reflex")
5. **Audit → patch coupling.** Extend `audit_layer_spectral` to emit
   *suggested patches* (band excess → EQ cut, aliasing → oversample/drive
   down, silence → register check) and add an `apply_audit_patch` tool or
   fold into `apply_patch`.
6. **Spectrogram bar/beat grid.** Overlay the timeline's bar/beat
   boundaries on per-layer spectrograms (time axis already maps to
   samples → ticks at known BPM).
7. **Reference-track spectral comparison.** Wire `SpectralReference` into
   `ss audit --reference <wav>`: render, then compare per-layer band
   energies to the reference's, reporting deviation per layer.

### P3 — Production workflow completeness
8. **Stereo stems + multichannel export.**
9. **Tempo automation / key-change support** in config + planner.
10. **Audition loop in playbooks:** rewrite playbook steps to end with
    `audit_layer_spectral` + interpretation + `apply_patch` rather than a
    bare render, making the agent behave like a producer mixing to a
    reference.

### P4 — Scale & robustness
11. **Render caching** keyed on input hash (incremental re-render of
    changed segments only).
12. **Polyphony/anti-masking analysis** wired into the validator (make
    `ReduceMasking` real).

---

## 5. How to reproduce the current state

```bash
# Full gate
cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --check

# Live demo: rock, jazz, trap
ss new /tmp/ss-rock --brief "linkin park style nu metal rock, 32 bars"
ss render /tmp/ss-rock && ss audit /tmp/ss-rock     # now includes LAYER AUDIT table + PNGs

ss new /tmp/ss-jazz --brief "jazz swing quartet, 32 bars" && ss render /tmp/ss-jazz
ss new /tmp/ss-trap --brief "trap beat, heavy 808, 24 bars" && ss render /tmp/ss-trap

# MCP layer audit tool
ss tool audit_layer_spectral --params '{"project_id":"x","project":{...}}'
```
