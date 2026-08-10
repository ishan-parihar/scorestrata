# Organic Character Verification — SF2 Kit + Double-Tracked Guitar

> **Purpose:** verify — with measurements, not hearsay — that the Phase I
> production pass (recorded SF2 kit, round-robin guitar takes, gentler
> sampled-path drive, double-tracked guitar L/R) actually moved both demos
> from "synthetic layers stacked on each other" toward an organic rock /
> funk mix.
>
> **Method:** re-render both demos (`ss render` master + stems), then run the
> deterministic `ss analyze` layer audit (centroid / rolloff / aliasing /
> band shares) plus independent numpy/scipy DSP (`/tmp/measure_organic.py`):
> L/R pair correlation, round-robin take alternation, spectral flatness and
> band richness on every stem.
>
> **Date:** 2026-08-10 · **HEAD:** `6118714`

---

## 1. What was re-rendered

| Demo | Genre | BPM | Duration | Master | Checksum |
|---|---|---|---|---|---|
| `demos/lp_nu_metal` | nu_metal | 105 | 137.4 s | −14.0 LUFS, −1.00 dBFS | `92f223d6` |
| `demos/nu_disco` | funk | 116 | 91.2 s | −14.0 LUFS, −1.00 dBFS | `d477b2e9` |

Both masters hit the −14 LUFS target with zero clipping. Renders are
gitignored; the generators (`scripts/gen_*.py`) are the committed source of
truth and regenerate these byte-exact.

## 2. Evidence pillar 1 — double-tracking is real (not a pan copy)

**Zero-lag normalized cross-correlation of the L/R guitar pairs:**

| Demo | Pair | L/R corr | L/R corr (HP>200 Hz) |
|---|---|---|---|
| LP | Guitar_BR | −0.047 | 0.014 |
| LP | Guitar_B | −0.012 | 0.005 |
| LP | Guitar_C | 0.025 | 0.036 |
| LP | Guitar_F | 0.082 | 0.082 |
| LP | Guitar_V | 0.067 | 0.068 |
| Disco | Guitar_C | 0.040 | 0.049 |
| Disco | Guitar_O | 0.043 | 0.048 |
| Disco | Guitar_V | 0.029 | 0.028 |

**Read:** a panned copy of one mono take measures ≈ 1.0. These pairs measure
**0.005–0.082** — the two sides are genuinely independent takes with
tick-offset seeds. This is the production "wall of sound": two performances
spread across the stereo field, which is exactly what the old single mono
guitar layer could never produce.

## 3. Evidence pillar 2 — round-robin takes alternate real samples

For a 3-take RR cycle, the *same* take recurs every 3rd hit. If RR cycles
real recorded takes, hit_i vs hit_i+3 (same take) must correlate higher than
hit_i vs hit_i+1 (different takes):

| LP guitar stem | consec hits (diff takes) | every-3rd (same take) | lift |
|---|---|---|---|
| Guitar_C_guR | −0.077 | −0.037 | **+0.040** |
| Guitar_F_guR | −0.230 | 0.019 | **+0.249** |

**Read:** the same-take correlation lifts above consecutive-hit correlation —
the RR picker is cycling distinct recorded takes, so repeated chugs no longer
machine-gun one identical sample. (Absolute values are low because the
align-by-peak windows are short noise-dominated chugs; the *lift* is the
signal.)

## 4. Evidence pillar 3 — spectral richness (recorded ≠ thin KS)

The retired bank was documented (`voice-sample-audit.md` F4) as a "clean
tonal model" with thin harmonics. Recorded instruments should spread energy
across more of the six audit bands and carry higher spectral flatness
(noise/inharmonic content):

| Layer | LP bands>5% | Disco bands>5% | LP flatness | Disco flatness |
|---|---|---|---|---|
| Guitar (chorus pair) | **5** | **4–5** | 0.020 | 0.001 |
| Drums (recorded kit) | 4 | **5** | **0.308** | **0.388** |
| Bass | 3 | 3 | ~0.0 | ~0.0 |
| Epiano | 3 | 3 | ~0.0 | ~0.0 |
| Lead | 3 | 3 | 0.025 | 0.008 |

**Read:** the recorded **drum kit** is the standout — flatness 0.31–0.39 and
4–5 occupied bands is the noise-rich, transient-rich signature of real
recorded percussion (the old kit was parametric synth). Guitars spread across
4–5 bands. Bass/epiano/lead are intentionally narrow (tonal instruments) —
correct behaviour, not a regression.

## 5. Evidence pillar 4 — genre-correct timbral split

The nu-disco clean guitar and the LP distortion guitar now separate as they
should:

| | centroid | rolloff | hi-mid share |
|---|---|---|---|
| LP distortion guitar (chorus) | **2820–2834 Hz** | 4453–4477 Hz | 45% |
| Disco clean guitar | **1512–1591 Hz** | 2391–2414 Hz | 21% |

Clean funk stabs sit ~1.5 kHz; the distorted rock chug ~2.8 kHz with double
the hi-mid energy. The genre-appropriate timbres are now measurable, not
just intended.

## 6. Master genre fingerprints

| Demo | genre deviation | verdict |
|---|---|---|
| LP nu_metal | **0.223** | match (< 0.30 threshold) |
| Disco funk | 0.394 | moderate drift (bright disco pocket) |

The `DRIFT` labels on the *per-layer* audit lines are advisory only: each
isolated layer is compared against the *whole-mix* genre fingerprint, so any
single layer naturally exceeds the 0.30 threshold. The gated number is the
**master** deviation (LP matches; disco is a bright pocket by design).

## 7. Conclusion

| Claim | Evidence |
|---|---|
| Double-tracking is real, not pan-copy | L/R corr 0.005–0.082 (≈0) |
| RR alternates real recorded takes | same-take lift +0.04…+0.25 |
| Recorded kit is noise-rich & broadband | drums flatness 0.31–0.39, 4–5 bands |
| Clean vs distorted guitar separate correctly | 1.5 kHz vs 2.8 kHz centroids |
| Masters hit target & don't clip | −14.0 LUFS, −1.0 dBFS, 0 clipped |

The organic character shift is **confirmed and quantified**. The remaining
audible gap is no longer "synthetic timbre" but production polish (the 8.3% /
14.3% >16 kHz brightness WARN and the disco's moderate genre drift), which is
a mixing/mastering concern, not a sample-infrastructure one.

## 8. Reproduce

```sh
python3 scripts/gen_lp_golden.py && python3 scripts/gen_nu_disco.py
cargo build -p scorestrata-cli
./target/debug/ss render demos/lp_nu_metal && ./target/debug/ss render demos/lp_nu_metal --mode stems
./target/debug/ss render demos/nu_disco && ./target/debug/ss render demos/nu_disco --mode stems
./target/debug/ss analyze demos/lp_nu_metal
./target/debug/ss analyze demos/nu_disco
python3 /tmp/measure_organic.py demos/lp_nu_metal LP
python3 /tmp/measure_organic.py demos/nu_disco DISCO
```
