# nu_disco — Daft Punk *Get Lucky*–style nu-disco gold-standard demo

A 44-bar, 116 BPM, B Dorian, 4/4 nu-disco track (~1:31) expressing the
nu-disco blueprint (see `docs/planning/music-reference-catalog.md` §2)
through the recorded SF2 bank + house drum kit:

- **Dorian loop** Bm7 → D → F#m7 → E (i₇–III–v₇–IV), four 1-bar patterns
  transposed per bar (`[0, +3, −5, −7]`).
- **Note-based disco drums** (`sound_bank: "house"`): 4-on-floor kick,
  snare+clap backbeat, closed 8th hats with **open-hat off-beats** (the
  disco pocket) — full GM kit via note pitches (36/38/42/46/49).
- **Octave-bounce bass** — root/octave 8ths on B1/B2.
- **Off-beat guitar stabs** — power-chord hits on the "and" of each beat
  (the Nile Rodgers chuck).
- **Epiano chord stabs** on beats 1 & 3 with a passing octave.
- **B pentatonic lead riff** in the choruses.
- Structure: Intro → Verse → Break → Chorus → Verse → Chorus → Outro
  (volume-automation fade-out).

Regenerate (deterministic) / render / export:

```bash
python3 scripts/gen_nu_disco.py
ss compile demos/nu_disco
ss render  demos/nu_disco                # → render/master.wav
ss render  demos/nu_disco --mode stems   # → render/stems/
ss export-midi demos/nu_disco --output demos/nu_disco/nu_disco.mid
```

## Timeline

| Bar | Segment | Role | Drums | Bass | Guitar | Epiano | Lead |
|-----|---------|------|-------|------|--------|--------|------|
| 0–3 | Intro | intro | enters b2 | enters b2 | — | stabs | — |
| 4–11 | Verse | verse | disco | **octave bounce** | **off-beat stabs** | comps | — |
| 12–15 | Break | build | disco | — | — | stabs | — |
| 16–23 | Chorus | chorus | disco | bounce | stabs | comps | **riff** |
| 24–31 | Verse | verse | disco | bounce | stabs | comps | — |
| 32–39 | Chorus | chorus | disco | bounce | stabs | comps | riff |
| 40–43 | Outro | outro | disco | bounce | stabs | comps | — (volume fade) |

## Validation (2026-08-10)

- Master: 91.2 s, −14.0 LUFS, −4.0 dBTP, crest **9.3 dB** (wider than the
  rock demo's 7.2 — disco's dynamic range), sha256 `0a0369e5`.
- Outro fade audible in the mastered mix: −13.3 → −17.9 dBFS.
- MIDI export: `nu_disco.mid` — load in a DAW and compare against the
  nu-disco reference patterns in the music-reference catalog.
