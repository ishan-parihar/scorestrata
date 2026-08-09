# Voice-Sample Infrastructure Audit & Production-Grade Baseline Plan

> **Purpose:** answer "where are the high-quality voice samples?", audit what the
> engine actually ships today, and chart the legitimate path to a
> production-grade instrument baseline.
>
> **Method:** binary-level audit of every embedded WAV (`sox` + Python DSP),
> inspection of the generator (`bin/gen_instrument_bank.rs`), a crates.io survey
> of the sample-playback ecosystem, and a `tg` CLI sweep of the two producer
> channels the user pointed at.
>
> **Date:** 2026-08-10 · **HEAD:** `6490306`
>
> **Status update (Phase I, `ac8ace8`):** the synthetic bank audited below was
> **retired** — `embedded_bank.rs`, `gen_instrument_bank.rs` and the 19 WAVs are
> deleted, and the recorded `SoundFont` bank (`sf2_bank` + `assets/instruments_sf2/`,
> FluidR3_GM via rustysynth) is now the only melodic sample path. Findings F1–F5
> remain as the historical record of *why* the synthetic bank was replaced.

---

## 1. Where the "voice samples" live (and what they actually are)

There is **one** sample asset in the whole repo:

```
crates/scorestrata-audio/assets/instruments/  (19 WAVs, 6.3 MB, embedded via include_bytes!)
```

Every one of the 19 files is **synthetic** — produced at build time by
`src/bin/gen_instrument_bank.rs`:

| Family | Roots (MIDI) | Layers | Duration | Technique |
|---|---|---|---|---|
| guitar | 36, 45, 52, 59, 66 | soft/hard | 1.70 s | Karplus–Strong delay line + body resonance |
| bass | 24, 29, 36, 43 | soft/hard | 2.40 s | Karplus–Strong delay line + body resonance |
| epiano | 48, 60, 72 | single | 3.00 s | additive FM-ish partials + sustain loop |

**There are no recorded instruments in the system.** What plays in the rock demo
is a deterministic plucked-string model — closer to a cheap guitar synth than a
sample library. That is exactly why layers still read as "synth" rather than
"rock".

## 2. Quality audit of the current bank

Per-file DSP (24-bit decode, peak/RMS/DC/max-jump):

| File | Peak | RMS dB | DC | Tail dB | Max jump |
|---|---|---|---|---|---|
| guitar_*_soft | 0.85 | **−22.3…−23.0** | +0.0005 | −38…−39 | 0.48–0.67 |
| guitar_*_hard | 0.85 | **−27.3…−28.0** | +0.0005 | −43…−44 | 0.48–0.67 |
| bass_*_soft | 0.85 | **−26.5…−27.2** | +0.0003 | −42…−43 | 0.42–0.67 |
| bass_*_hard | 0.85 | **−30.8…−31.4** | +0.0004 | −47…−48 | 0.42–0.67 |
| epiano_* | 0.85 | **−8.8…−8.9** | ~0.0000 | −16.3 | 0.18–0.68 |

### Findings

1. **`F1` — velocity layers are inverted (real bug).** Hard layers are ~5 dB
   *quieter* than soft layers. A harder pick must be louder and brighter; here
   the generator's `normalize()` pegs every file to the same 0.85 peak and the
   only audible difference left is decay time (hard = longer). The `pluck`
   amplitude argument (0.85 hard vs 0.45 soft) is completely defeated by the
   final normalization. **Result: velocity response is backwards.**

2. **`F2` — 15–22 dB level mismatch inside the bank.** e-piano sits at −8.8 dB
   RMS; strings at −22…−31 dB. Any layer that uses the e-piano will dominate
   the mix by 15+ dB before the mixer even sees it. Layers were never level-matched.

3. **`F3` — no per-note character.** One synthetic one-shot per root × layer,
   no round-robin alternates on the *timbre* axis (RR seeds vary gain only), no
   pick noise/body mic separation. Repeated same-pitch hits machine-gun by
   construction.

4. **`F4` — harmonic content is thin.** KS with a one-zero filter is a clean
   tonal model; it lacks string inharmonicity, fret buzz, pick scrape, cab
   coloration. This is the *reason* "no layer is like rock".

5. **`F5` — DC offset +0.0003…0.0006** on strings (body-resonance sine). Small,
   but nonzero — a mastering chain that DC-blocks will thump on every pluck.

### What is good
- Determinism is bulletproof (SplitMix64-seeded, byte-exact — R1/R2 held).
- Zero external assets, embedded at compile time (R7 held).
- 24-bit, 48 kHz, mono, correct loop points on e-piano, no clipping.

## 3. The crates.io sample infrastructure we are missing

The engine already has a competent zone/velocity/loop player
(`multisample.rs`). What it lacks is **an instrument source**. Survey:

| Crate | Ver | License | Role |
|---|---|---|---|
| **rustysynth** | 1.3.6 | **MIT** | Full SoundFont (.sf2) MIDI synthesizer, pure Rust, offline buffer rendering |
| soundfont | 0.1.0 | MIT-ish | .sf2 parser (extract sample data + zones) |
| nidhi | 1.1.0 | **GPL-3.0** | SFZ/SF2 sample engine — GPL, unusable for embedding |
| fluidlite | 0.2.1 | LGPL | C bindings — FFI + LGPL, avoid |
| rameau_soundfont | 0.1.0 | — | .sf2/.sf3 loader (nascent) |

**Recommendation: `rustysynth` (MIT) as the production voice, or the `soundfont`
crate to bake real instrument samples into our existing zone engine.** SoundFont
files *are* multi-sampled instruments (key zones, velocity layers, sustain
loops) — the exact model `multisample.rs` already implements.

### Free, legally-usable SoundFonts (verified licensing)
| Font | Size | License | Fit |
|---|---|---|---|
| **FluidR3_GM** (Frank Wen) | ~140 MB (MIT) | MIT | the standard; guitars/basses present but GM-level |
| **GeneralUser GS** (S. C. Collins) | ~30 MB | free, no redistribution as standalone product | best "recorded" feel for a small file |
| Musyng Kite | ~990 MB | free | large orchestral/GM |
| Arachno | ~35 MB | non-commercial only | skip |

A SoundFont of even 30 MB gives us *real* recorded electric guitar, bass,
drums, keys across 128 GM programs with proper velocity layers — an enormous
jump from 6.3 MB of synthesized plucks, at MIT-grade licensing.

## 4. The two Telegram channels (audit result)

Both channels were synced and searched (`tg sync -- -1002412094962`,
`-1001160529263`, ~2164 messages in PersimmonFL):

- **Producer Library / FL Studio / Ableton** (`-1002412094962`): mostly
  *Spinnin' / Sample Magic / Oliver* **house & pop sample packs** (WAV loops &
  one-shots, torrent links) plus synth presets. Little to nothing rock/nu-metal.
- **PersimmonFL** (`-1001160529263`): **torrents of commercial products** —
  Ample Sound guitar VSTs, Kontakt 8, Native Instruments Guitar Rig 6, TH-U,
  Drum Daddy, etc.

**Assessment:** these channels redistribute *commercial, licensed* sample packs
and VST plugins (Ample Sound, NI, Spinnin', Splice) via torrent. Two hard
problems:

1. **License.** Those are retail products (Kontakt 8, Ample Guitar, Spinnin'
   Sounds). Downloading them from a torrent channel is piracy — they are not
   "free downloads" in any legally usable sense, and a production product built
   on them cannot ship.
2. **Technical.** VSTs are plugin executables — our engine is a headless Rust
   renderer with no plugin host, so 90% of that channel (Kontakt/Ample/Guitar
   Rig) is unusable *even if* licensed. Only raw WAV packs would matter, and the
   ones there are the same commercial packs with distribution rights stripped.

**Conclusion:** the Telegram path does not get us to a production baseline.
The SoundFont path does — same outcome (real recorded multi-sampled
instruments), MIT licensing, pure Rust, no torrents.

## 5. Recommended plan (ordered)

| Step | Work | Gate | Status |
|---|---|---|---|
| P1 | Fix `F1`: level-correct velocity layers in the generator (pre-normalize by layer, hard = +6 dB, brighter) | `audit_samples.py` shows hard ≥ soft | ✅ `4b49a87` |
| P2 | Fix `F2`: normalize all layers to a common RMS (−18 dB ref) in the generator | all files within ±3 dB | ✅ `4b49a87` |
| P3 | Fix `F5`: DC-block the generator output (one-pole HPF at 20 Hz) | |dc| < 1e-5 | ✅ `4b49a87` |
| P4 | Add `rustysynth` (MIT); wire a GM patch → layer mapping; render FluidR3_GM one-shots at build time | demo renders, checksum changes, clippy/test gate green | ✅ `147778e` |
| P5 | Regenerate + commit; re-run LP demo; compare LUFS/spectrogram vs. current bank | report in `docs/` | ✅ below |

## 6. Result — Phase H shipped

### The recorded bank (`147778e`)

- **`extract_sf2`** — a build-time tool that renders FluidR3_GM GM programs
  (30 Distortion Guitar, 34 Picked Bass, 4 E.Piano) through **rustysynth
  (MIT, pure Rust)** into 21 dry 48 kHz mono one-shot WAVs. Rendering (not
  raw-sample copy) bakes in the font's own envelopes, filters and velocity
  layers — the one-shot is what the recorded instrument *sounds like*.
- **`sf2_bank`** — zone manifest + `include_bytes!` over the rendered
  one-shots, with a deterministic sustain-loop detector for the e-piano.
  Same instrument names/zones as the synthetic bank, so the two are
  drop-in interchangeable behind `melodic_bank: "embedded" | "sf2"`.
- The 100 MB SoundFont itself is **not** embedded (R7 posture kept: only
  ~8.6 MB of rendered one-shots are committed).

### LP-demo stem comparison (synthetic → recorded)

| Layer | Synthetic | SF2 bank | Δ |
|---|---|---|---|
| Guitar | −10.4 LUFS | **−9.0** | +1.4 (loudest melodic element) |
| Bass | −18.7 LUFS | **−10.8** | **+7.9** (recorded picked bass present) |
| Drums | −19.7 | −19.7 | kit untouched |
| Lead | −9.2 | −9.2 | synth lead unchanged |

Master render: **−14.0 LUFS, −5.1 dBTP**, checksum `193704f5` (was
`51ae96cb` — the recorded bank is verifiably in the mix). Bass jumped
~8 dB because the recorded GM-34 picked bass carries real body and
attack that the synthetic pluck could not.

### Gates

- 936 workspace tests pass (6 new: bank load, determinism, register
  coverage, SF2≠synthetic proof, 8 s e-piano sustain, loop detector).
- `clippy --workspace --all-targets -- -D warnings` clean, `fmt` clean.
- Commits: `4b49a87` (level fixes), `147778e` (SF2 bank).
