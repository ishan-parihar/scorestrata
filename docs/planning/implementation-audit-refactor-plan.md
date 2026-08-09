# Scorestrata Implementation Audit & Refactor Plan

> **Purpose:** review how well `docs/planning/re-architecture-upgrade-plan.md` (Phases A–F) has been
> implemented, surface every bug and implementation gap found by code inspection, and give an
> **ordered refactor plan for the final touches** before the full testing/validation pass.
>
> **Method:** read-only audit (no source changes made in this pass). Evidence: `git log/status`,
> `cargo clippy`, `cargo test`, targeted greps across all crates.
>
> **Companion docs:** `docs/planning/re-architecture-upgrade-plan.md` (the plan, "R"),
> `docs/system-audit-report.md` (pre-upgrade audit).
>
> **Audit date:** 2026-08-09 · **HEAD:** `4e05214` (Phase D committed)
>
> **✅ EXECUTED 2026-08-10** — every Step 1–6 work item below is closed. Final state:
> **917+ workspace tests green · clippy 0 · fmt clean · golden byte-identity green**;
> commits `5e0aa8b`, `60e4a23`, `1a85d89`, `4888a5a`, plus this session's traceability/docs
> commit. Phase A–F entries are in the traceability ledger (`SPEC-REARCH-A…F`); the plan's
> §10 now carries the formal implementation-status + amendment notes.

---

## 0. Executive summary

| Area | Status |
|---|---|
| Phase A (sound bank) | ✅ Implemented + committed + pushed |
| Phase B (bus graph + convolution reverb) | ✅ Implemented + committed + pushed |
| Phase C (performance: humanize/groove/articulation) | ✅ Implemented + committed + pushed |
| Phase D (mastering chain) | ✅ Implemented + committed + pushed |
| Phase E (reference-driven debug loop) | 🟡 Implemented but **uncommitted** + **breaks the build** |
| Phase F (polyphony, anti-masking, docs, traceability) | ❌ Not started |
| **`cargo test --workspace`** | 🔴 **Fails to compile** (2× E0027 in CLI tests) |
| **`cargo clippy -D warnings`** | 🔴 **10 errors**, all in new `reference.rs` |
| Golden byte-identity tests | ✅ Present (R1) |
| Backward compat (`#[serde(default)]`) | ✅ 48 sites (R3) |
| MCP tool surface | ✅ All 12 plan tools delivered + dispatch + capabilities |
| CLI surface (plan §4.2) | 🟡 Partial (`audit --reference/--grid` only) |

**Bottom line:** the four "audio-engine" phases (A–D) are strong, deterministic, R1-safe, and
green when taken alone (889 tests passing at Phase D). Phase E's core module + tools are solid
and even include the exit-criteria simulation, but the branch is **not gate-clean**:
1. It introduced a **compile break** in the CLI test suite (E0027 — the new `--reference`/`--grid`
   fields were added to the `Analyze`/`Audit` commands but not to the test patterns).
2. `reference.rs` carries **10 clippy errors** (5× `expect()`, 2 unfulfilled lint expectations,
   `const fn`, needless cast, `mul_add`).
3. Several plan items were **never implemented** (sample-rate modes + `rubato`, LUFS exit test,
   render caching, `physical.rs` Karplus-Strong, polyphony budget, anti-masking wiring,
   traceability ledger update).

Phase F is entirely outstanding.

---

## 1. Build health (ground truth)

### 1.1 🔴 `cargo test --workspace` does not compile

```
error[E0027]: pattern does not mention fields `reference`, `grid`
  --> crates/scorestrata-cli/src/main.rs:2737:13
     Commands::Analyze { path } => assert_eq!(path, "my_song"),

error[E0027]: pattern does not mention fields `reference`, `grid`
  --> crates/scorestrata-cli/src/main.rs:2757:13
     Commands::Audit { path } => assert_eq!(path, "my_song"),
```

**Cause:** Phase E added `reference: Option<String>` and `grid: bool` to the `Analyze` and
`Audit` CLI commands (§4.2) but the CLI argument-parser tests still match `{ path }` exactly.
**Fix:** `Commands::Analyze { path, .. }` / `Commands::Audit { path, .. }` (or add the fields).

### 1.2 🔴 `cargo clippy --workspace --all-targets -- -D warnings` — 10 errors (all `reference.rs`)

| Line | Lint | Fix |
|---|---|---|
| 290:9 | `suboptimal_flops` (`mul_add`) | `num = e.mul_add((lo + s) as f32, num);` |
| 295:24 | `unnecessary_cast` (f32→f32) | drop the `as f32` |
| 312:1 | `missing_const_for_fn` | `pub const fn band_labels()` |
| 17:5 | unfulfilled `cast_possible_truncation` expectation | remove from `#![expect(...)]` list |
| 18:5 | unfulfilled `cast_sign_loss` expectation | remove from `#![expect(...)]` list |
| 329:23, 344:23, 362:27, 393:22, 422:22 | `expect_used` (workspace lint `expect_used = "warn"`) | replace `.expect()` with proper `?` / handled errors |

Note: the module-level `#![expect(...)]` lists `cast_possible_truncation` and `cast_sign_loss`
which never fire — the *cast_precision_loss* expectation is the one actually used (the compiler
is smart about `i64 as f32` here because the casts are on exactly-representable values).

---

## 2. Phase-by-phase implementation review

### Phase A — Sound bank & sampled percussion ✅ (commit `a066d31`)

**Delivered**
- `scorestrata-audio/src/samplebank.rs` — 5 kits (`rock`, `house`, `hiphop`, `techno`, `trap`),
  `KIT_NAMES`, `kit()`, `kit_voice_names()`, `kit_character()`, `kit_processing_summary()`.
- MCP `list_sound_bank` / `set_sound_bank` tools + `sample_bank_catalog` resource + dispatch +
  capabilities (R5 additive).
- Determinism tests (`kits_are_deterministic`, `kits_have_distinct_characters`).
- `RenderConfig.sound_bank: Option<String>` — `None` keeps the legacy synth-drum path (R1).

**Gaps / deviations**
- **Deviation (documented, acceptable):** the "CC0 embedded WAV banks" from plan §3.5/§R7 were
  implemented as **procedurally synthesized kits** (per-voice DSP tuning), not
  `include_bytes!`-embedded one-shot WAVs. The plan's §R7 "no external runtime deps" is still
  honored, and determinism is arguably *better* (no asset licensing). Worth a plan amendment note.
- **❌ Missing: `physical.rs` (Karplus-Strong plucked string / membrane drum)** — plan §3.3 lists
  it; the audio crate has no `physical.rs`. Phase A's thesis bullet ("samples + physical models")
  is only half delivered. Either implement it or formally descope in the plan.

### Phase B — Convolution reverb + bus graph ✅ (commit `78ff5a4`)

**Delivered**
- `bus.rs` — group busses, send/return routing, `MixBus` model, mono-fold invariance preserved.
- `convolve.rs` — partitioned convolution + `embedded_ir()` + pre-delay + wet control;
  `irs_are_distinct_and_deterministic` test.
- `sidechain_amount: Option<f32>` (continuous send amount, plan §3.4) in `FxChain` (domain.rs:704).
- MCP `set_bus_graph` / `get_bus_graph` + `bus_graph_resource` + `render_stereo_stems`.
- R1 test: `bus_graph_disabled_is_byte_identical_to_flat_fold` (render.rs:1889).

**Gaps / deviations**
- **Deviation:** IRs are procedurally generated (synthetic decay/noise), not EchoThief/OpenAIR/
  Samplicity CC0 captures as plan §3.5 lists. Functionally sound; licensing risk = zero.
  Recommended plan amendment (same note as Phase A).

### Phase C — Performance: humanize + groove + articulation ✅ (commit `0c76c59`)

**Delivered**
- `scorestrata-compiler/src/performance.rs` — SplitMix64 seeded PRNG, `layer_stream_seed`
  (per-layer streams, Ableton "Random" semantics), groove pass (swing16 → 66 % off-beat shift
  @1.0 — test `swing16_shifts_off_beat_16ths_by_66_percent_at_full_amount`), humanize
  (velocity/timing/gate jitter), `apply_performance_passes`.
- Articulation resolved in `compile.rs::articulation_resolution` (Accent/Staccato/Legato/
  Portamento→CC/Pedal) — matches plan §3.2 pass list, applied while building timeline events.
- MCP `set_humanize` / `set_articulation` + `humanization_view` resource.
- Determinism tests: `same_seed_reproduces_identical_humanize`, `different_seed_changes_the_performance`,
  `disabled_spec_is_a_noop` (R1/R2).

**Gaps**
- None material. (The pass is documented as running at event-build time rather than as a separate
  compiler pass — a pragmatic ordering choice, consistent with the plan's fixed order note.)

### Phase D — Mastering chain ✅ (commit `4e05214`)

**Delivered**
- `mastering.rs` — LR-24 multiband (3 bands) with per-band M-S width, 2×-oversampled soft-clip
  saturation, lookahead limiter, TPDF dither; R1 test
  `master_chain_disabled_is_byte_identical_to_legacy_path` (render.rs:1915).
- `wav.rs` — 16/24-bit PCM export with `write_wav_stereo_16_bit_is_byte_identical_to_legacy_path`.
- MCP `set_master_chain` + `master_chain_view` resource + dispatch + capabilities.
- `RenderConfig.master_chain`, `RenderConfig.bits_per_sample`.

**Gaps — plan §3.3 / §4.2 items NOT delivered**
- **❌ `RenderConfig.sample_rate` (44.1/48/96 kHz) via `rubato` is missing.** The plan's §3.3
  "RenderConfig gains: `sample_rate` (44.1/48/96), `bit_depth` (16/24/32)" — only `bits_per_sample`
  (16/24) exists; `render_notes_with_config` still takes a hardcoded `sample_rate` param and
  `rubato` is **not** in any Cargo.toml (only `rustfft`). 96 kHz mode and the SRC path were never
  built.
- **❌ Phase D exit criterion "−14 LUFS ±0.5 @ −1 dBTP ceiling test" has no test.** `analysis.rs`
  computes `integrated_lufs` and there are *sanity* assertions (`> -70.0`), but **no mastering-level
  test asserting −14 LUFS ±0.5 / true-peak ≤ −1.0 dBTP** anywhere in `mastering.rs`/`render.rs`.
- **❌ Phase D exit criterion "no aliasing >16 kHz on driven guitar with saturation" has no test**
  (only `polyblep_saw_reduces_audible_band_aliasing` in synth, which is not the saturation path).
- **❌ 32-bit float export** (`bit_depth` 32) not implemented (16/24 only).

### Phase E — Reference-driven debug loop 🟡 (uncommitted; breaks the build)

**Delivered (working tree only)**
- `reference.rs` — `ReferenceProfile`, 6-band energy comparison, centroid/LUFS deviation,
  groove extraction with sample-accurate onset refinement; 5 unit tests green.
- MCP tools: `load_reference_track`, `compare_to_reference`, `apply_audit_patch`,
  `extract_groove_from_reference` (+ `import_reference_profile` pre-existing at tools.rs:3620)
  — all wired in `dispatch.rs` (lines 548–577) + `capabilities.rs` (4 new entries).
- Exit-criteria simulation test: `audit_patch_loop_reduces_seeded_deviation_by_half` (tools.rs:13200)
  + `compare_to_reference_reports_master_and_layer_deviations` (tools.rs:13115).
- CLI `Analyze`/`Audit` gained `--reference <wav>` and `--grid`.

**Gaps / bugs**
- 🔴 **Compile break** (E0027, §1.1) — CLI tests not updated for the new fields.
- 🔴 **10 clippy errors** in `reference.rs` (§1.2).
- **❌ Render caching keyed on input hash is missing.** Phase E exit criteria explicitly says
  "Render caching keyed on input hash (audit without full re-render)". Today only
  `render.cache_key` is *reported* in the receipt (tools.rs:7638, from
  `render_manifest::cache_key()`); there is **no cache that skips a re-render** in the audio crate
  (`grep cache render.rs` → nothing).
- **❌ `reference_profile` resource (plan §4.1) missing** — resources.rs has `bus_graph_resource`,
  `humanization_resource`, `master_chain_resource`, `sample_bank_catalog_resource`, but **no
  `reference_profile_resource`**.

### Phase F — Scale, polyphony, hardening ❌ (not started)

- **❌ Polyphony budget** — no polyphony/budget logic in the validator (grep `polyphony` →
  nothing outside docs).
- **❌ Anti-masking "make `ReduceMasking` real"** — `reduce_masking` exists only as an MCP tool
  (tools.rs:1447); the validator has **no `ReduceMasking`/anti-masking analysis wired in**.
- **❌ Traceability ledger update** — `scorestrata-eval/src/traceability.rs::canonical_matrix()`
  still contains only pre-upgrade entries (the only new-ish entry is `SPEC-AUDIO-RENDER`); **no
  Phase A–F entries** for the new modules/tools/resources.
- **❌ Full doc pass / README + playbook rewrite** — not done (no README at repo root; docs are
  `architecture/` + `planning/` only).
- **❌ Eval `production_pipeline_simulation.rs` not updated** — still references the old
  `SpectralReference` domain type; the new `compare_to_reference` / `apply_audit_patch` /
  `extract_groove_from_reference` tools are not exercised by the eval harness (success-metric #5).
- 🟡 Tempo/key-change support (plan marks optional) — not done; acceptable to descope.

---

## 3. Bug ledger (ordered)

| # | Sev | Where | Bug | Fix |
|---|---|---|---|---|
| B1 | 🔴 Blocker | `cli/src/main.rs:2737,2757` | E0027: test patterns don't mention `reference`/`grid` | `{ path, .. }` |
| B2 | 🔴 Blocker | `audio/src/reference.rs:329–422` | 5× `expect()` under `expect_used = "warn"` + `-D warnings` | proper `Result` propagation |
| B3 | 🟠 High | `audio/src/reference.rs:17–18` | unfulfilled `#![expect]` entries (`cast_possible_truncation`, `cast_sign_loss`) | prune list |
| B4 | 🟢 Low | `audio/src/reference.rs:290,295,312` | `mul_add` / needless cast / `const fn` | mechanical clippy fixes |
| B5 | 🟠 High | Phase D | missing −14 LUFS / −1 dBTP exit test | add mastering-level test |
| B6 | 🟠 High | Phase D | missing 96 kHz / `rubato` SRC (plan §3.3) | implement or descope in plan |
| B7 | 🟠 High | Phase E | render caching not implemented (exit criteria) | implement input-hash cache |
| B8 | 🟡 Med | Phase E | `reference_profile` resource missing (plan §4.1) | add resource |

---

## 4. Refactor plan — ordered work items

Each item = one commit, gated by `cargo test --workspace` + clippy 0 + fmt clean (plan §6 R6).

### Step 1 — Unblock the build (B1, B2, B3, B4) · `Phase E fix-up`
1. Fix the two E0027 test patterns in `cli/src/main.rs` (`{ path, .. }`).
2. Fix `reference.rs`: prune the `#![expect]` list, apply `mul_add`/cast/`const fn`, and
   replace the 5 `.expect()` calls with real error propagation (return `Err(AudioError::...)`).
3. Gate: `cargo test --workspace` (expect ≈ 900+), clippy 0, fmt clean.
4. Commit + push "feat(E): reference-driven debug loop — load/compare/patch/groove tools + CLI".

### Step 2 — Close Phase D exit-criteria gaps (B5, B6)
1. Add a mastering-level **LUFS + true-peak test**: render a driven mix through the chain and
   assert `integrated_lufs ≈ −14 ±0.5` and `true_peak ≤ −1.0 dBTP`.
2. Add a **saturation anti-aliasing test** (drive a tone, assert >16 kHz band energy stays below
   threshold) — the plan's Phase D exit criteria.
3. Decide 96 kHz: implement `RenderConfig.sample_rate` + `rubato` (add dependency, test 44.1/96
   vs 48 for determinism), **or** amend the plan to formally descope SRC to 48 kHz-only.
4. Gate + commit + push.

### Step 3 — Phase E exit-criteria completion (B7, B8)
1. Implement a **render cache keyed on the timeline input hash** (reuse
   `render_manifest::cache_key()`), so `compare_to_reference` / `audit` can skip a full re-render
   when inputs are unchanged; wire into the Phase E tools + add a cache-hit test.
2. Add the `reference_profile` resource (URI `system://reference-profile/<id>`) mirroring the
   other view resources + capability entry + test.
3. Gate + commit + push.

### Step 4 — Phase F: polyphony budget + anti-masking (validator)
1. Implement a **polyphony budget check** in the validator (peak simultaneous voices per layer /
   project, config threshold) wired into `scope::full` (or the density checker).
2. Make **`ReduceMasking` real**: validator-level anti-masking analysis (register overlap +
   spectrum-based masking pairs) feeding the existing `reduce_masking` MCP tool.
3. Add validator tests + update `production_pipeline_simulation.rs` to exercise the new path.
4. Gate + commit + push.

### Step 5 — Phase F: traceability + docs + eval
1. Add **Phase A–F entries** to `traceability::canonical_matrix()` (new modules, tools,
   resources, tests) so success-metric #5 (agent reflex DoD simulation) is covered.
2. Update `production_pipeline_simulation.rs` to drive the **new** Phase E tools
   (`set_humanize → render → audit → compare_to_reference → apply_audit_patch → re-render`) with
   a decreasing-deviation assertion.
3. Docs pass: note the A/B asset deviations (procedural kits/IRs), document new CLI flags,
   write README + playbook.
4. Gate: full workspace tests + clippy + fmt + `cargo run -- ss doctor`.
5. Commit + push.

### Step 6 — Full validation & simulation gate (success metrics §9)
1. `cargo test --workspace` — assert count **≥ 848** and monotonically growing.
2. Golden byte-identity: all three R1 tests green.
3. Reference-match simulation: `audit_patch_loop_reduces_seeded_deviation_by_half` +
   eval harness deviation-reduction run.
4. Clippy 0 / fmt clean / CI workflow (`ci.yml`) green.
5. Final demo: rock / jazz / trap project through the entire new stack (A→E).

---

## 5. Deviations from plan (proposed amendments)

| Plan said | Implemented | Recommendation |
|---|---|---|
| §3.5 CC0 embedded sample WAVs | Procedural kit synthesis | Keep; document as deterministic asset-free approach |
| §3.5 CC0 convolution IRs | Procedural IR synthesis | Keep; document |
| §3.3 `physical.rs` (Karplus-Strong) | Not implemented | Implement in Phase F+ or formally descope |
| §3.3 `sample_rate` 44.1/48/96 via `rubato` | 48 kHz only | Implement or descope |
| §3.3 `bit_depth` 16/24/32 | 16/24 only | Descope 32-bit float (unused by WAV targets) |

---

## 6. Appendix — evidence index

- MCP tools all present: `list_sound_bank` (2374), `set_sound_bank` (2421), `set_humanize` (2536),
  `set_master_chain` (2725), `set_articulation` (2833), `set_bus_graph` (4752), `get_bus_graph`
  (4869), `render_stereo_stems` (8631), `load_reference_track` (8922), `compare_to_reference`
  (9031), `apply_audit_patch` (9224), `extract_groove_from_reference` (9379) — all in
  `tools.rs`, all dispatched (dispatch.rs:333–577).
- New resources: `bus_graph_resource` (1670), `humanization_resource` (1755),
  `master_chain_resource` (1816), `sample_bank_catalog_resource` (1851).
- R1 byte-identity tests: render.rs:1889, render.rs:1915, wav.rs:386.
- R3: 48× `#[serde(default)]` in domain.rs.
- Test function counts (incl. helpers): performance.rs 18, mastering.rs 26, reference.rs 12,
  convolve.rs 13, bus.rs 6.
- Commits: A `a066d31`, B `78ff5a4`, C `0c76c59`, D `4e05214`; E uncommitted (6 files modified,
  +918 lines: error.rs, lib.rs, main.rs, capabilities.rs, dispatch.rs, tools.rs; new reference.rs).
