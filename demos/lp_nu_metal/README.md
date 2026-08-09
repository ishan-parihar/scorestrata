# lp_nu_metal — Linkin Park–style nu-metal gold-standard demo

A 60-bar, 105 BPM, A minor, 4/4 nu-metal track (~2:17) authored as a
**gold-standard structural reference**: it recreates the blueprint of
Linkin Park's production — the **i–VI–III–VII** progression, quiet
verse / wall-of-sound chorus dynamics, palm-muted chug guitars,
root/octave bass, backbeat-to-syncopated drums, and an e-piano intro
hook — and renders through the **recorded SF2 bank** (`melodic_bank:
"sf2"` → FluidR3_GM Distortion Guitar / Picked Bass / E.Piano) plus the
rock drum kit (`sound_bank: "rock"`).

Regenerate the project (deterministic — uuid5 ids, byte-identical on
every run):

```bash
python3 scripts/gen_lp_golden.py
```

> **Canonical demo:** this committed project (`demos/lp_nu_metal/`) is the
> canonical gold-standard. The older `lp_demo/` at the repo root is gitignored
> scratch from the Phase G stress test — this demo supersedes it.

Render / export:

```bash
ss compile   demos/lp_nu_metal
ss render    demos/lp_nu_metal               # master → render/master.wav
ss render    demos/lp_nu_metal --mode stems  # per-layer stems → render/stems/
ss export-midi demos/lp_nu_metal --output demos/lp_nu_metal/lp_nu_metal.mid
```

---

## Timeline — segments × layers

| Bar | Segment | Role | Drums | Bass | Guitar | Lead | Epiano |
|-----|---------|------|-------|------|--------|------|--------|
| 0–3 | Intro | intro | enters b2 (soft) | enters b2 | — | — | **hook arp** |
| 4–11 | Verse | verse | backbeat | root 8ths | **palm-mute chug** | — | sparse |
| 12–15 | Build | build | 16th-hat ramp | root 8ths | **sustained power chords** | **rising line** | — |
| 16–23 | Chorus | chorus | **syncopated** | **root/octave** | **wall-of-sound power chords** | **hook melody** | — |
| 24–31 | Verse | verse | backbeat | root 8ths | chug | — | sparse |
| 32–35 | Build | build | ramp | 8ths | sustained | rising | — |
| 36–43 | Chorus | chorus | syncopated | octave | power chords | hook | — |
| 44–51 | Bridge | bridge | — | **sustained halves** | sparse | **long notes** | **arp** |
| 52–59 | Finale | chorus | syncopated | octave | power chords | hook | — (4-bar fade) |

Instrument routing: `Drums` = `drum_sampler` (rock kit) · `Bass` =
`bass_synth` → SF2 `bass` · `Guitar` = `distorted_guitar` → SF2 `guitar` ·
`Lead` = `lead_synth` (subtractive voice) · `Epiano` = `epiano_bell` →
SF2 `epiano`.

## The i–VI–III–VII progression

One 1-bar pattern per layer, transposed per bar over the 4-chord loop
(A minor pattern base; semitone offsets `[0, −4, +3, −2]`):

| Bar | Chord | Guitar root | Bass root | Transpose |
|-----|-------|-------------|-----------|-----------|
| 0 | Am (i) | A2 (45) | A1 (33) | 0 |
| 1 | F (VI) | F2 (41) | F1 (29) | −4 |
| 2 | C (III) | C3 (48) | C2 (36) | +3 |
| 3 | G (VII) | G2 (43) | G1 (31) | −2 |

## Per-layer note map (1-bar patterns, PPQ = 480)

**Drums** — 16-step GM tier patterns (kick vel ≥ 105, snare 95–104, hats
< 95):

- `Drums_Backbeat` — kick 1&3, snare 2&4, 8th-note hats (verse/finale).
- `Drums_Verse` — same, −4 dB quieter (intro entry).
- `Drums_Build` — kick/snare + 16th-hat velocity ramp 62→90 (build tension).
- `Drums_Chorus` — syncopated kick (1, 2-and, 3, 4-and), backbeat snare,
  16th-hat drive (chorus/finale).

**Bass** (register 1–3):

- `Bass_Verse` — root 8ths on A1, alternating 100/88 velocity.
- `Bass_Chorus` — root 8ths with octave (A2) pickups on beats 2-and/4.
- `Bass_Bridge` — sustained half notes (the "In the End" bridge moment).

**Guitar** (register 2–3, power chords A2+E3):

- `Guitar_Verse` — single-note A2 8th chugs, 120-tick gates (palm-mute).
- `Guitar_Chorus` — root+fifth power chords, quarter hits on 1&3 +
  8th chugs between (the wall of sound).
- `Guitar_Build` — sustained half-note power chords.
- `Guitar_Bridge` — sparse root pickups.

**Lead** (register 4–5, A minor pentatonic):

- `Lead_Hook` — A4–C5–E5–D5–C5–A4 hook (the "Numb" arpeggio shape).
- `Lead_BuildRise` — 8-note rising run A4→F5 with a **cutoff-sweep
  automation lane** (CC 74, 0.35→1.0 across the build).
- `Lead_Bridge` — long sustained notes (E5, C5, A4).

**Epiano** (register 3–5):

- `Epiano_Arp` — 8th-note root-octave arpeggio A3–E4–A4 (the intro hook).

## Mix & master

Per-layer faders/pans: Drums 0 dB · Bass −1.5 · Guitar −2.0 (pan −0.2) ·
Lead −3.0 (pan +0.25) · Epiano −7.0 (pan +0.3). Humanize (seed 105) for
the "played" feel. Master chain: multiband (mono < 200 Hz) → saturation
→ 24-bit dither → −14 LUFS target. Measured section dynamics (mastered):
Intro −16.4 → Verse −9.9 → Build −8.6 → Chorus −7.1 → Bridge −10.9 → Finale −7.2 dBFS RMS.

## Validation results (2026-08-10)

- Master: 137.4 s, −14.0 LUFS, −6.1 dBTP, crest 7.2 dB, sha256 `908dc0cc`.
- Stems: Guitar −7.3 · Bass −10.6 · Lead −10.2 · Epiano −17.6 · Drums −22.1 LUFS.
- Finale fade-out: **volume automation** (CC 7) on all four finale layers
  ramps the last 4 bars to 20% — audible in the mastered mix (−11.2 →
  −13.4 dBFS), unlike a velocity taper which the master chain compensates.
- MIDI export: `lp_nu_metal.mid` (16 KB) — load in any DAW (FL Studio,
  Reaper, LMMS) to compare the arrangement against the reference songs in
  `docs/planning/music-reference-catalog.md`.
- Pinned by `golden_lp_nu_metal_demo_passes_full_gates` in
  `crates/scorestrata-eval/tests/golden_harness_e2e.rs` — compile, full L6
  validation, and SF2 render gates run in CI.
