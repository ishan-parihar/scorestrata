# Music Reference Catalog — production structures & open-source score files

> **Purpose:** note-level structural blueprints of production-grade music
> (Linkin Park nu-metal, Daft Punk–style nu-disco) that scorestrata can
> recreate for validation, plus a catalog of **open-source / free score
> files** (`.flp`, MIDI, Guitar Pro, LMMS `.mmpz`, Reaper `.rpp`) usable as
> gold-standard reference data in the R&D loop.
>
> Companion: `demos/lp_nu_metal/` — a 60-bar nu-metal gold-standard project
> built from the Linkin Park blueprint below.
>
> **Audit date:** 2026-08-10

---

## 1. Linkin Park — the nu-metal blueprint

All four canonical tracks share one harmonic engine: **i – VI – III – VII**
in a minor key, over 4/4, in **Drop C# / Drop D** low tunings, with the
verse/chorus "quiet → wall of sound" dynamic paradigm.

| Song | Tempo | Key | Progression (RN) | Structure |
|------|-------|-----|------------------|-----------|
| **Numb** | 110 BPM | F♯ minor | i–VI–III–VII | Intro 8 (synth/piano hook) · Verse 8 · Pre-Chorus 4 · Chorus 8 ×2 · Bridge 8 · Outro 8 |
| **In the End** | 105 BPM | D♯ minor | i–VI–III–VII | Intro 8 (piano hook) · Verse 16 (rap) · Pre 8 · Chorus 8 ×2 · Bridge 8 · Solo 8 · Outro 4–8 |
| **Faint** | 135 BPM | C♯ minor | i–VI–III–VII | Intro 8 (staccato strings/guitar) · Verse 8 · Pre 8 · Chorus 8 ×2 · Bridge 8 · Outro 8 |
| **Crawling** | 105 BPM | C♯ minor | i–VI–III–VII | Intro 4–8 · Verse 8 · Pre 8 · Chorus 8 ×2 · Bridge 8 · Outro 4–8 |

### The four production layers (timeline view)

1. **Rhythm guitar (Drop C#/D, palm-muted chugs)** — the riff engine.
   Verses: single-note palm-muted 8th chugs on the low root. Choruses:
   wide power chords (root+fifth, doubled/quad-tracked hard L/R) hitting
   on 1 & 3 with 8th-note chugs between.
2. **Bass (sub + picked)** — locks to the kick; 8th-note root pulsing in
   verses, root + octave pickups and unison-with-guitar roots in choruses,
   long sustained roots in the bridge.
3. **Drums (breakbeat → live hybrid)** — verses: hip-hop breakbeat or
   backbeat with 8th hats; choruses: syncopated kick (1, 2-and, 3, 4-and),
   backbeat snare, driving 16th hats; bridge: dropped out or rim/ride.
4. **Keys/synth + lead** — the hook layer: square/saw synth or e-piano
   arpeggio (Numb's intro = arpeggiated chord tones), plus a vocal-range
   melodic lead in the choruses. Mr. Hahn's scratches/breakdowns act as
   structural punctuation.

**Golden-demo mapping** (`demos/lp_nu_metal`, 105 BPM, A minor):
Intro 4 (epiano arp) · Verse 8 (chug + backbeat + root bass) · Build 4
(sustained chords + rising lead + cutoff sweep) · Chorus 8 (power chords +
syncopated drums + hook) · Verse 8 · Build 4 · Chorus 8 · Bridge 8 (no
drums) · Finale 8 (fade).

---

## 2. Nu-disco (Daft Punk *Get Lucky*) — the blueprint

| Parameter | Pattern |
|-----------|---------|
| Tempo | 110–122 BPM (Get Lucky = 116) |
| Harmony | **Dorian**: Bm7 → D → F♯m7 → E (i₇–III–v₇–IV), loops the whole song |
| Bass | **Octave-disco bounce**: root–octave–root 8ths with anticipations |
| Guitar | Nile Rodgers 16th-note "chuck": off-beat triad stabs, muted scrapes |
| Drums | 4-on-floor kick, snare/clap on 2&4, closed 16th hats + open off-beat pedal |
| Dynamics | Filter sweeps, arrangement thinning, volume automation (no loudness walls) |

A nu-disco family would need: a 4-on-floor step pattern, an octave-bounce
bass shape (already supported — see `Bass_Chorus`), off-beat triad guitar
chucking, and a Dorian chord loop. The infrastructure (step patterns,
transpose cycles, SF2 e-piano/bass) is already in place.

---

## 3. Open-source / free score files for R&D

### 3.1 FL Studio projects (`.flp`) — the exact format you asked about

- **Format:** `.flp` is a reverse-engineered binary event stream (chunk
  headers, variable-length event ids, UTF-16LE strings in 11.5+). **No
  official spec**, but fully parseable by community libraries:
  - **`pyflp`** (Python, `@demberto`) — arrangements, playlist tracks,
    channel rack, automations, instruments, mixer inserts, notes.
  - **`libflp`** (C++, `@codecat`) — metadata/sorting across `.flp` dirs.
  - **`FLParser`** (C#, `@monadgroup`) — full `Project/Channel/Pattern/Note`
    object model.
  - **`ts-flp`** / **`flpdiff`** (TypeScript, `@SpendLessDaw` / `@dawhubapp`)
    — non-destructive patches, semantic git diffs.
- **Finding open `.flp` files:** GitHub hosts template collections (e.g.
  `clrke/flstudio-projects`, `numberoneboybiggiefj/FL-Studio-Producer-pack`);
  full retail-song recreations are rare because `.flp` files don't embed
  third-party VSTs or stems — they reference plugin names, so a recreation
  only loads if you own the same plugins. **Scorestrata's bridge is MIDI**:
  `ss export-midi` emits a multitrack `.mid` (per-layer tracks, GM drum
  map) that FL Studio imports directly — see `demos/lp_nu_metal/lp_nu_metal.mid`.

### 3.2 MIDI transcriptions (free archives)

Free fan-made multitrack MIDI of the LP songs is available at freemidi.org
/ bitmidi.com (search "Linkin Park Numb MIDI" etc.). Typical track layout:
piano/keys, bass, guitar(s), drums (GM map), lead vocal line. These are
**the fastest structural reference data**: import the `.mid` into
scorestrata via a future MIDI-importer, or use them to compare our
arrangement's layer/track layout in a DAW.

### 3.3 Guitar Pro tabs (note-level riffs)

`.gp5`/`.gp` tabs of Numb / In the End / Faint / Crawling show exact Drop
C# riffs and bass lines; free at Ultimate Guitar / Songsterr. Guitar Pro
tabs are XML-ish (`.gp` = zipped XML since v7) — parseable for riff-level
validation of our power-chord/chug patterns.

### 3.4 Open-source DAW formats (fully transparent)

- **LMMS `.mmpz`** (XML, compressed) — the most accessible open format:
  community rock/nu-metal demo projects + SoundFont configs. XML = trivial
  to parse with the tooling we already have.
- **Reaper `.rpp`** (plain-text S-expressions) — thousands of open
  mixing-template repos show the *production* structure (parallel drum
  compression, multi-amp guitar busses, vocal chains) even where the song
  audio itself isn't distributable.

### 3.5 License reality check

SoundFonts are the legal recorded-instrument path (FluidR3_GM, MIT —
already integrated via `extract_sf2`). Torrented retail VST/sample packs
from Telegram channels are **not legally usable**, and VSTs can't run in
our headless renderer anyway. MIDI transcriptions and Guitar Pro tabs of
songs are fan transcriptions — fine for study/structural comparison, not
for redistributing the compositions.

---

## 4. R&D next steps

1. **MIDI importer** — parse multitrack `.mid` (tracks → layers, GM drum
   map → steps, note/velocity/timing → patterns) so any transcribed song
   becomes a scorestrata project for validation.
2. **Nu-disco family** — Dorian loop, 4-on-floor step pattern,
   octave-bounce bass, off-beat guitar chuck (infrastructure exists).
3. **FLP reader** — a Rust or Python-side `pyflp` bridge to ingest
   community `.flp` templates and diff our arrangements against real DAW
   layouts.
