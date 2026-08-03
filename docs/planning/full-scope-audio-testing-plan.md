# Full-Scope Audio Testing & Per-Phase Generation Plan

**Status:** Proposed (planning)
**Owner:** scorestrata engineering loop
**Basis:** live end-to-end verification run (2026-08-03) + scaffolding audit
**Related:** `docs/architecture/architecture.md` §11 (audio), §16 (test pyramid / golden harness), §17 (reproducibility)

---

## 0. Why this plan exists

The system was exercised end-to-end for the first time as a *listening* experience:
`ss new` → `ss compile` → `ss validate` → `ss render` produced **three real, playable WAV
tracks** (techno 30.97 s @ -12.5 LUFS, ambient 96 s @ -11.0 LUFS, house 31.48 s @ -12.5 LUFS)
plus 4 stems, all valid RIFF/WAVE PCM 16-bit 48 kHz. The full workspace is green
(670/670 tests, 0 clippy, fmt clean).

That run exposed concrete quality and capability gaps. This plan turns those gaps into
shippable iterations, with **audio delivered per pipeline phase** and **validation at both
build time and run time (post-generation)**.

---

## 1. Scaffolding audit (what exists / what is missing)

### 1.1 Verified working today

| Capability | Where | Evidence |
|---|---|---|
| Brief → deterministic config | `scorestrata_mcp::config_gen::generate_config` | `ss new` prints genre/BPM/key/bars |
| Arrangement planning | `scorestrata_compiler::plan::plan_project` | 4 segments planned per project |
| Compile to timeline | `scorestrata_compiler::compile_project` | 608 events, 30,720 ticks |
| Validation (L2/L3/L4) | `scorestrata_validator::*` | score 0.85 / pass, 3 warnings |
| Offline synthesis | `scorestrata_audio::synth` (osc + filter + ADSR), `sampler`, `mixer` | audible WAVs |
| WAV writing | `scorestrata_audio::wav::write_wav` | 16-bit PCM mono |
| Loudness analysis | `scorestrata_audio::analysis` (LUFS/dBTP/crest) | -11 to -12.5 LUFS reported |
| Stems rendering | `render_stems` | 4 stems + master |
| Manifest + checksum | `scorestrata_storage::render_manifest` | recorded per render (§17.2) |
| MCP dispatch of any tool | `ss tool <name> '<json>'` | full catalog reachable from CLI |

### 1.2 Gaps found by the audit

1. **No mastering stage → clipping.** True peak measured **+1.8 dBTP (techno)** and
   **+3.7 dBTP (ambient)** — over 0 dBTP with no limiter. The render is 16-bit mono and
   the mix overs. (L4 diagnostics already flag the symptom; nothing fixes it.)
2. **Stereo mixer exists but is not wired.** `mixer.rs` implements constant-power pan and
   `mix() -> (left, right)`, but `render_notes` collapses to mono
   (“The WAV writer currently writes mono”). `write_wav` is mono-only.
3. **No per-phase (per-segment) audio path in the CLI.** `render_segment` exists as an MCP
   tool (§MCP), but `ss render` has no `--segment` flag, so you cannot hear a single phase
   (Intro / Verse / Build / Chorus) in isolation.
4. **No `ss demo` / sample-project export.** The canonical testkit fixtures
   (`melodic_techno_project`, `ambient_project`, `house_project`, `eight_bar_loop`) exist
   only as Rust code; there are no project JSON bundles to render in one command.
5. **Run-time post-generation validation is not automated.** `RenderManifest::verify_artifact`
   and the analysis suite exist, but no gate runs them against the just-written artifact
   (checksum match, true peak ≤ 0 dBTP, LUFS target, duration match).
6. **No CI workflow.** `.github/workflows` does not exist; build-time validation is
   local-only (`cargo test/clippy/fmt`).

### 1.3 Verdict

The **pipeline scaffolding is in place and functional**. What is missing is
(a) audio *quality* (limiter, stereo), (b) audio *accessibility* (per-phase + demo
generation), and (c) audio *verification* (automated build-time + run-time gates).
Everything below builds on existing seams — no architecture change required.

---

## 2. Deliverable: audio of each phase generation

Every pipeline phase that can produce sound gets a playable artifact, written under
`demo/<project>/render/`:

```
demo/<project>/render/
  segments/            # per-phase audio (§2.2)
    Intro.wav          # phase: arrangement segment 1
    Verse.wav
    Build.wav
    Chorus.wav
  stems/               # per-layer audio (§2.3)
    Drums.wav  Bass.wav  Lead.wav  Lead_Octave.wav
  master.wav           # full timeline render (§2.4)
  manifest.json        # one RenderManifest per artifact (§17.2)
```

### 2.1 Phase map (what “phase” means)

| Phase | Pipeline stage | Audio deliverable |
|---|---|---|
| 0 | Brief → Config | none (deterministic config; no sound yet) |
| 1 | Arrangement | one WAV per segment (`segments/<SegmentName>.wav`) |
| 2 | Patterns/Layers | one WAV per layer (`stems/<LayerName>.wav`) |
| 3 | Timeline | full master (`master.wav`) |
| 4 | Mastering | post-limiter master (true peak ≤ -0.5 dBTP) |

### 2.2 Phase-1 audio (segments)

- Expose `--segment <id|name>` on `ss render` (the MCP `render_segment` capability,
  currently CLI-hidden).
- Segment-only render compiles the project, filters timeline events to the segment,
  renders, writes `segments/<Name>.wav`, and records a §17.2 manifest scoped to that
  segment (artifact type `segment`).

### 2.3 Phase-2 audio (stems)

- Already implemented (`--mode stems`); extend to always emit the per-stem manifests and
  post-generation validation (§4).

### 2.4 Phase-3 audio (master)

- Already implemented (`--mode master`); mastered variant after Fix F1.

### 2.5 `ss demo` command

- One command renders the built-in genre fixtures (techno / ambient / house / 8-bar loop)
  across all phases into `demo/<genre>/render/…`, printing a listening index
  (paths + LUFS + dBTP + checksum) and running §4 validation on every artifact.
- Requires exporting the testkit fixtures as project JSON bundles (new
  `scorestrata-testkit::write_sample_projects(dir)` or checked-in sample bundles).

---

## 3. Bug fixes (in priority order)

### F1 — Deterministic peak limiter (mastering stage) — *the clipping fix*
**Where:** new `scorestrata_audio::mastering` (or `limiter.rs`), applied at the end of
`render_notes` / `render_stems` master mix.
**Spec:**
- Soft-knee lookahead-free peak limiter; true peak ≤ **-0.5 dBTP** ceiling.
- Fully deterministic (fixed-point-style arithmetic on f32, no `f32` nondeterminism —
  mirror existing determinism guarantees), so render checksums stay stable.
- Applies per-sample to the (future) stereo bus; exposes `LimiterConfig { ceiling_db, knee_db }`.
- L4 audio validation (`audio_check`) gains: **post-mastering true peak ≤ 0 dBTP** is a
  hard pass criterion; a diagnostic is emitted if the unmastered mix overs.
**Tests:** limiter clamps a sine at full scale to ≤ ceiling; unity-gain pass-through below
knee; determinism (same input → same bytes); crest factor improves; existing checksum
stability on mastered pipeline.

### F2 — Stereo render path (wire the existing mixer)
**Where:** `render.rs` (`render_notes`, `render_stems`, `render_layer_buffer`), `wav.rs`,
`analysis.rs`.
**Spec:**
- Keep mono as the default for compatibility, add `RenderConfig { stereo: bool }` /
  `--stereo` flag; when stereo: mix via `mixer::Mixer` (pan from layer name hash or
  per-layer pan default), write a **stereo** WAV (extend `write_wav` to interleave L/R),
  and run the stereo-aware analysis.
- `RenderManifest.channels` already exists (§17.2) — record 1 vs 2 honestly.
**Tests:** stereo interleave correctness; L/R pan isolation (panned-left channel has
negligible right energy); mono path unchanged (checksum byte-identical to today); stereo
analysis LUFS/dBTP consistency with mono downmix.

### F3 — `ss render --segment <id|name>` (per-phase audio)
**Where:** CLI `main.rs` (`handle_render`, `Cli::Render`).
**Spec:** accept optional `--segment`; filter timeline events to the segment (reuse the
MCP `render_segment` filtering), write `segments/<Name>.wav`, record segment manifest.
**Tests:** CLI parse tests (like the existing `parse_render_command_*`), plus an
integration assertion that segment render duration ≤ full master duration and that
concatenated segment event counts equal the full timeline count.

### F4 — `ss demo` (hear everything in one command)
**Where:** CLI `main.rs` new subcommand; `scorestrata-testkit` sample-project export.
**Spec:** §2.5 above. Runs §4 validation on every artifact and fails the command if any
gate fails (exit code 1 with a report), so “did the demos stay good” is scriptable.
**Tests:** `ss demo` in a temp dir produces expected file set; every WAV validates against
its manifest; demo regenerates deterministically (same checksums across two runs).

---

## 4. Validation — build-time and run-time (post-generation)

### 4.1 Build-time (compiled into the test suite)

| Layer | What | Location |
|---|---|---|
| Unit | limiter clamp / pass-through / determinism; stereo interleave; segment filter | `scorestrata-audio` `scorestrata-mcp` tests |
| Contract | §17.2 manifest schema (13 fields) for master/segment/stems artifacts | `scorestrata-storage` tests |
| Property | render determinism across repeated runs (hash stability) | `scorestrata-audio` proptest-style test |
| Integration | `ss demo` file-set + re-render determinism | `scorestrata-cli` tests |
| Simulation | **MVAS step 38**: compose → per-segment phase audio → stems → mastered master → post-generation validation (checksum match, true peak ≤ 0 dBTP, LUFS target, duration) → manifest recorded for every artifact | `scorestrata-eval/tests/mvas_simulation.rs` |
| Traceability | `SPEC-AUDIO-MASTERING` and `SPEC-PER-PHASE-RENDER` rows | `scorestrata-eval/src/traceability.rs` |
| Gate | `cargo build` + `cargo test` + `cargo clippy --all-targets` + `cargo fmt --check` all clean | workspace-wide |

### 4.2 Run-time post-generation validation (fires on every render)

A `validate_render()` gate runs after any artifact is written and feeds the receipt:

1. **Checksum** — `RenderManifest::verify_artifact(bytes) == true` (declared sha256 matches
   the bytes actually written; catches mid-write corruption).
2. **True peak** — mastered output ≤ 0 dBTP (post-F1 always ≤ -0.5 dBTP).
3. **Loudness target** — integrated LUFS in a genre-profile band (e.g. -20…-8 LUFS);
   surfaced as a warning when outside.
4. **Duration** — measured duration matches `manifest.duration_seconds` within a sample
   tolerance.

Receipt gains `render.verified=true` and `render.validation` summary; a failing gate
produces a `RENDER_POSTVALIDATION_FAILED` diagnostic (§12 conventions) and a `next_action`
of `get_render_manifest`. All four checks are unit-testable in isolation.

### 4.3 CI (optional follow-up)

Add `.github/workflows/ci.yml` running the §4.1 gate on push/PR so build-time validation
is enforced remotely (out of local-repo scope; flagged for explicit approval).

---

## 5. Implementation order (each iteration ships + pushes)

| Iteration | Scope | Exit criteria |
|---|---|---|
| **A** | F1 limiter + F2 stereo + unit/contract tests + §17.2 channels wiring | audio tests green; mastered master true peak ≤ -0.5 dBTP; stereo flag renders valid stereo WAV; workspace 670+ tests green |
| **B** | F3 `--segment` + F4 `ss demo` + sample-project export + CLI/integration tests | `ss demo` produces the §2 file tree; per-segment WAVs valid; deterministic re-runs |
| **C** | §4.2 post-generation gate wired into `render_project`/CLI + MVAS step 38 + traceability rows | every artifact verified on write; step 38 green; traceability matrix updated |

Each iteration: implement → targeted tests → full workspace gate
(build/test/clippy/fmt) → deepseek code review → commit + push.

---

## 6. Test matrix (new tests enumerated)

| # | Test | Crate | Iteration |
|---|---|---|---|
| 1 | `limiter_clamps_full_scale_sine_to_ceiling` | audio | A |
| 2 | `limiter_passes_through_below_knee` | audio | A |
| 3 | `limiter_is_deterministic_same_input_same_bytes` | audio | A |
| 4 | `stereo_interleave_matches_lr_buffers` | audio | A |
| 5 | `stereo_pan_isolation_left_vs_right` | audio | A |
| 6 | `mono_path_unchanged_byte_identical` | audio | A |
| 7 | `stereo_analysis_consistent_with_downmix` | audio | A |
| 8 | `manifest_records_stereo_channels` | storage | A |
| 9 | `render_segment_events_sum_to_timeline` | mcp | B |
| 10 | `cli_render_segment_parse` | cli | B |
| 11 | `demo_produces_expected_file_set` | cli/eval | B |
| 12 | `demo_redeterministic_checksums` | cli/eval | B |
| 13 | `postgen_checksum_matches_manifest` | mcp/storage | C |
| 14 | `postgen_true_peak_within_ceiling` | mcp/audio | C |
| 15 | `postgen_lufs_within_genre_band` | mcp/audio | C |
| 16 | `postgen_duration_matches_manifest` | mcp/storage | C |
| 17 | `mvas_step_38_phase_audio_validation` | eval | C |
| 18 | traceability rows for SPEC-AUDIO-MASTERING / SPEC-PER-PHASE-RENDER | eval | C |

---

## 7. Definition of done

- You can run **one command** (`ss demo`) and hear every phase of every genre.
- The audio does not clip (true peak ≤ -0.5 dBTP after mastering) and can be stereo.
- Every generated artifact is verified at write time against its manifest (checksum,
  loudness, duration) and verified again in the test suite.
- Workspace gate: 688+ tests (670 current + ~18 new), 0 clippy, fmt clean, all pushed.
