# Scorestrata Architecture Blueprint

Rust-first, compiler-minded, validation-oriented, MCP-exposed, and agent-friendly. Scorestrata is a **closed-loop agentic system** where AI agents can:

- understand the current state,
- diagnose problems,
- propose safe patches,
- test those patches,
- validate musical and audio quality,
- learn from outcomes,
- roll back safely,
- and progressively build both the music and the Scorestrata platform itself with minimal friction.

The architecture should therefore be treated as **five coupled planes**:

1. **Contract & Governance Plane** — capability discovery, schema registry, tool contracts, policy engine, budgets, receipts, error taxonomy.
2. **Autonomy Plane** — agent kernel, workflow state machine, task DAG, candidate branches, experiments, memory, lessons, escalation, multi-agent coordination.
3. **Creation Plane** — the composition compiler, audio engine, validator, storage, export, debug lenses.
4. **Engineering Plane** — executable specs, task graph, golden tests, eval harness, CI gates, self-building loop, traceability matrix.
5. **Evidence & Learning Plane** — event ledger, snapshots, evidence warehouse, metric lineage, provenance manifests, replay journal, lessons, calibration profiles, preference signals, telemetry.

> Scorestrata should not merely be a DAW with an API.
> It should be a machine-operable, self-auditing, self-testing composition compiler with a first-class agentic control plane.
>
> **Scorestrata is a contract-first, event-sourced, multi-objective composition compiler for AI agents, with deterministic replay, diagnostic validation, policy-gated mutation, calibrated taste models, and an executable engineering plane.**

---

# 1. Core Architectural Thesis

## thesis

Scorestrata should be:

> A deterministic, auditable, multi-objective composition compiler for AI agents, with an integrated validation/repair loop and a self-building engineering harness.

This means every feature must satisfy ten agentic design laws:

1. **Every state must be inspectable.**
2. **Every action must produce a receipt.**
3. **Every mutation must be reversible.**
4. **Every failure must produce actionable diagnostics.**
5. **Every improvement must be measurable.**
6. **Every action must be replayable.**
7. **Every action must be policy-aware.**
8. **Every metric must be evidence-backed.**
9. **Every outcome must be learnable.**
10. **Every capability must be contractually discoverable.**

If a feature cannot be inspected, tested, rolled back, diagnosed, scored, replayed, policy-checked, evidence-backed, learned from, or discovered, it is not agent-ready.

---

# 2. System Architecture

Scorestrata should be split into five explicit planes:

```text
+------------------------------------------------------------------+
|                        External Actors                           |
|      Human Producer / AI Agent / Orchestrator / CI Agent         |
+-------------------------------+----------------------------------+
                                |
                                | CLI / MCP / SDK / CI
                                v
+------------------------------------------------------------------+
|                 Contract & Governance Plane                      |
|  - capability discovery                                          |
|  - schema registry                                               |
|  - tool contracts                                                |
|  - policy engine                                                 |
|  - budgets / quotas                                              |
|  - permissions / safety classes                                  |
|  - versioning / compatibility                                    |
|  - receipts / error taxonomy                                     |
+-------------------------------+----------------------------------+
                                |
                                v
+------------------------------------------------------------------+
|                        Autonomy Plane                            |
|  - agent kernel                                                  |
|  - workflow state machine                                        |
|  - task DAG                                                      |
|  - candidate branches                                            |
|  - experiment runner                                             |
|  - memory / lessons                                              |
|  - escalation                                                    |
|  - multi-agent coordination                                      |
+-------------------------------+----------------------------------+
                                |
              +-----------------+-----------------+
              |                                   |
              v                                   v
+-----------------------------+     +-----------------------------+
|       Creation Plane        |     |      Engineering Plane      |
|                             |     |                             |
| - brief / intent            |     | - executable specs          |
| - config                    |     | - task graph                |
| - arrangement               |     | - golden tests              |
| - patterns                  |     | - contract tests            |
| - compiler passes           |     | - eval harness              |
| - validator                 |     | - benchmarks                |
| - audio engine              |     | - docs generation           |
| - mixer / FX                |     | - traceability matrix       |
| - export                    |     | - CI gates                  |
| - debug lenses              |     | - self-build loop           |
+--------------+--------------+     +--------------+--------------+
               |                                   |
               +-----------------+-----------------+
                                 |
                                 v
+------------------------------------------------------------------+
|                  Evidence & Learning Plane                       |
|  - event ledger                                                  |
|  - snapshots                                                     |
|  - evidence warehouse                                            |
|  - metric lineage                                                |
|  - provenance manifests                                          |
|  - replay journal                                                |
|  - lessons                                                       |
|  - calibration profiles                                          |
|  - preference signals                                            |
|  - telemetry / cost accounting                                   |
+------------------------------------------------------------------+
```

The **Autonomy Plane** is the operational brain. The **Contract & Governance Plane** ensures discoverability and safety. The **Evidence & Learning Plane** ensures reproducibility and improvement. This five-plane model ensures agentic efficacy does not collapse at any layer.

---

# 3. The Contract & Governance Plane

Agents fail when tools are discovered by trial and error, when schemas are implicit, when policies are hidden, and when costs are unknown.

## 3.1 Capability Discovery

Agents should be able to ask:

```text
system://capabilities
```

Response:

```json
{
  "system": "scorestrata",
  "version": "0.3.0",
  "schema_version": "0.3",
  "features": [
    "symbolic_compiler",
    "offline_render",
    "validator_v1",
    "agent_kernel",
    "snapshots",
    "candidate_branches",
    "eval_harness"
  ],
  "tools": [
    "create_project",
    "generate_config",
    "apply_patch",
    "run_validation",
    "render_project"
  ],
  "resources": [
    "project://<id>/summary",
    "project://<id>/validation/latest",
    "project://<id>/audit/summary"
  ],
  "budgets": {
    "supports_budgets": true,
    "max_render_seconds": 3600
  }
}
```

This allows agents to adapt to available functionality.

## 3.2 Contract and Schema Registry

Every tool, resource, command, receipt, diagnostic, patch, and artifact schema should be:

- versioned,
- machine-readable,
- discoverable,
- testable,
- compatible under explicit rules.

It should include:

- JSON Schema / typed schemas,
- Rust type generation,
- client SDK generation,
- MCP tool manifests,
- stability levels,
- deprecation metadata,
- compatibility constraints,
- cost and safety metadata.

### Example tool contract metadata

```json
{
  "name": "apply_patch",
  "version": "0.3.0",
  "stability": "alpha",
  "side_effects": "mutating",
  "supports_dry_run": true,
  "requires_snapshot": true,
  "budget_class": "medium",
  "safety_class": "write",
  "idempotency": "key-supported",
  "returns_receipt": true,
  "scopes": ["segment", "layer", "project"]
}
```

This reduces agent friction dramatically. Agents can:

- discover valid actions,
- avoid unsafe calls,
- understand costs,
- generate tests,
- and adapt to version changes.

## 3.3 Policy-as-Code and Safety Classes

Scorestrata needs a policy layer between the agent and mutating tools.

### Safety classes

- `read`
- `write`
- `render`
- `destructive`
- `governance`
- `engineering`

### Policy categories

| Category | Examples |
|---|---|
| Structural | Must have drums, bass, harmony, melody layers |
| Musical | Key compliance, max leap size, min chord-tone ratio |
| Arrangement | Chorus must be denser than verse |
| Mix | True peak must not exceed -1 dBTP |
| Render | Sample rate must be 48 kHz |
| Safety | No deletion of snapshots |
| Budget | No more than 8 candidate renders per segment |
| Governance | Core schema changes require human approval |

Policies should be declarative, versioned, and simulated in dry-run.

Example:

```toml
[policy.default]
allow_destructive_commands = false
require_snapshot_before_patch = true
max_out_of_scale_ratio = 0.05
min_chorus_energy_delta = 0.15
max_true_peak_dbtp = -1.0
require_midi_export_on_finalize = true
```

### Policy decision tracking

Every policy check should be recorded:

```json
{
  "policy_check_id": "pol_01J...",
  "command_id": "cmd_01J...",
  "allowed": true,
  "rules_evaluated": [
    {"rule": "require_snapshot_before_patch", "result": "passed"},
    {"rule": "max_out_of_scale_ratio", "result": "passed", "value": 0.02, "limit": 0.05}
  ],
  "override": null,
  "audit_trail": true
}
```

---

# 4. The Autonomy Plane

## 4.1 Agent Kernel

The Agent Kernel is the operational brain of Scorestrata.

It is responsible for:

- selecting the next action,
- managing project state,
- enforcing budgets,
- running compile/validate/render loops,
- generating candidates,
- selecting best candidates,
- applying patches,
- creating snapshots,
- detecting stagnation,
- escalating to humans when needed.

It should not be a vague "agent loop." It should be a deterministic workflow executor.

### Core responsibilities

```text
Agent Kernel
  - read project summary
  - read validation state
  - read budgets
  - choose scope: config / segment / layer / transition / mix
  - choose strategy: generate / patch / vary / mix / repair
  - propose candidates
  - dry-run candidates
  - compile scoped candidate
  - validate scoped candidate
  - score candidate
  - select best candidate
  - apply patch
  - snapshot
  - record lesson
  - repeat until done or budget exhausted
```

### Required kernel concepts

| Concept | Purpose |
|---|---|
| Workflow state machine | Defines what stage the project is in |
| Task DAG | Defines dependencies between composition or engineering tasks |
| Budget manager | Limits iterations, CPU, render time, storage, tool calls |
| Candidate branch | Isolates experimental mutations |
| Snapshot graph | Enables rollback and comparison |
| Stop conditions | Prevents infinite loops |
| Escalation policy | Determines when human review is required |
| Lesson store | Records what worked and what failed |
| Experiment runner | Manages multi-candidate experiments |
| Replay-aware planning | Uses replay journal to avoid repeating failed strategies |

### Decision Policy Layer

The kernel should not choose actions arbitrarily. It should use:

- diagnostic severity,
- expected score delta,
- patch cost,
- patch success probability,
- genre calibration,
- lesson confidence,
- budget remaining,
- no-improvement count.

### Example decision rule

```text
if hard_failure exists:
    choose structural repair
elif metric_regression detected:
    choose rollback or localized repair
elif budget.remaining < 20%:
    choose low-cost high-confidence patch
elif no_improvement_count >= threshold:
    escalate
else:
    choose highest expected_value patch
```

### Candidate Tournament Logic

Candidates should not be selected by one scalar alone. Use:

1. hard gate pass/fail,
2. policy compliance,
3. multi-objective scorecard,
4. Pareto comparison,
5. genre-weighted preference,
6. cost-adjusted value,
7. novelty penalty,
8. lesson-backed confidence.

This prevents metric hacking.

### Stagnation Detection

Track:

- no improvement streak,
- oscillating metric behavior,
- repeated failed patches,
- rising cost per improvement,
- regression after apparent progress,
- validator/human disagreement.

Responses:

- switch strategy,
- widen candidate diversity,
- reduce scope,
- request human review,
- quarantine project state.

---

## 4.2 Project State Machine

```text
BriefReceived
  -> ConfigDrafted
  -> ConfigValidationFailed
  -> ConfigApproved
  -> ArrangementPlanned
  -> ArrangementValidationFailed
  -> SkeletonCreated
  -> SegmentDrafting
  -> SegmentCandidatesGenerated
  -> SegmentValidationFailed
  -> SegmentValidated
  -> TransitionValidationFailed
  -> TransitionValidated
  -> MixDrafting
  -> MixValidationFailed
  -> Compiling
  -> Validating
  -> Experimenting
  -> Calibrating
  -> Rendering
  -> RenderFailed
  -> FinalValidationPassed
  -> RenderFinalized
  -> AwaitingHumanDecision
  -> NeedsPolicyReview
  -> NeedsHumanReview
  -> Replaying
  -> Quarantined
  -> Archived
```
---

## 4.3 Budgets and Quotas

Every agent session should have a budget envelope.

Example:

```json
{
  "budget_id": "bgt_01J...",
  "project_id": "proj_123",
  "max_iterations": 12,
  "max_tool_calls": 250,
  "max_render_seconds": 600,
  "max_cpu_seconds": 1800,
  "max_disk_mb": 2048,
  "max_candidate_branches": 8,
  "max_no_improvement_rounds": 3,
  "max_human_escalations": 2,
  "expires_at": "2026-08-01T18:00:00Z"
}
```

The kernel must enforce these.

Without budgets, agentic systems become expensive, unstable, or circular.

---

## 4.4 Multi-Agent Architecture

Scorestrata should support multiple specialized agents.

### Suggested agent roles

| Agent Role | Responsibility | Can Mutate? | Can Finalize? | Can Escalate? |
|---|---|---:|---:|---:|
| Producer Agent | Owns brief, final selection, taste alignment | limited | yes | yes |
| Planner Agent | Config, arrangement, structure, section goals | yes | no | yes |
| Composer Agent | Patterns, melody, harmony, rhythm | yes | no | yes |
| Sound Designer Agent | Instruments, presets, timbre | yes | no | yes |
| Mix Engineer Agent | Gain, EQ, compression, reverb, automation | yes | no | yes |
| Critic Agent | Validation, scoring, diagnostics | no | no | yes |
| Debugger Agent | Root-cause analysis, repair patches | limited | no | yes |
| Archivist Agent | Snapshots, lessons, provenance, reports | metadata only | no | no |
| Engineer Agent | Implements Scorestrata code/features/tests | code branch only | no | yes |

These can be separate LLM agents, separate prompts, or separate tool policies.

### Coordination model

Use a blackboard/task-board model.

```text
Task Board
  - ready tasks
  - claimed tasks
  - blocked tasks
  - review tasks
  - completed tasks

Blackboard
  - current diagnosis
  - active candidates
  - top issues
  - lesson hints
  - escalation flags
  - budget state
```

Each agent claims tasks using leases.

Example:

```json
{
  "task_id": "task_chorus_lead",
  "claimed_by": "agent:composer",
  "lease_expires_at": "2026-08-01T12:15:00Z",
  "scope": {"segment_id": "chorus_1", "layer_id": "lead"},
  "allowed_tools": ["write_pattern_notes", "apply_motivic_variation"],
  "allowed_mutations": ["pattern", "note"],
  "status": "in_progress"
}
```

This prevents conflicting mutations.

### Separate Critic from Mutator

The critic agent should not mutate project state. This preserves evaluation integrity.

### Merge Policy

When candidates compete:

- loser branches are archived,
- winner is merged into main branch,
- comparison evidence is stored,
- lesson is recorded,
- scorecard history is preserved.

---

# 5. Command, Receipt, and Patch Architecture

For agentic efficacy, every mutating operation should be a command with a receipt.

## 5.1 Command envelope

```json
{
  "command_id": "cmd_01J...",
  "idempotency_key": "agent-7-loop-3-quantize-lead",
  "type": "apply_patch",
  "actor": "agent:composer",
  "project_id": "proj_123",
  "branch_id": "branch_main",
  "dry_run": false,
  "scope": {
    "segment_id": "chorus_1",
    "layer_id": "lead"
  },
  "params": {
    "patch": {
      "op": "quantize_layer_to_scale",
      "strength": 1.0,
      "preserve_rhythm": true
    }
  },
  "budget_id": "bgt_01J...",
  "created_at": "2026-08-01T12:00:00Z"
}
```

## 5.2 Command receipt

Every command returns a receipt.

```json
{
  "command_id": "cmd_01J...",
  "status": "succeeded",
  "dry_run": false,
  "snapshot_id": "snap_0042",
  "branch_id": "branch_main",
  "score_delta": {
    "harmony.scale_compliance": "+0.14"
  },
  "policy_decisions": [],
  "events": [
    "pattern_updated",
    "timeline_invalidated",
    "validation_cache_invalidated"
  ],
  "artifacts": [],
  "evidence_uris": [
    "project://proj_123/segments/chorus_1/layers/lead/notes"
  ],
  "focus_uris": [
    "project://proj_123/validation/latest"
  ],
  "next_actions": [
    "run_validation",
    "get_audit_report",
    "render_segment_preview"
  ],
  "cost": {
    "cpu_ms": 82,
    "tool_calls": 1,
    "render_seconds": 0
  },
  "warnings": [],
  "replay_cursor": "evt_01J..."
}
```

This is crucial.

Agents should never ask, "Did that work?"
They should receive a structured receipt with:

- `evidence_uris` — direct pointers to evidence,
- `policy_decisions` — what policies were evaluated,
- `replay_cursor` — for deterministic replay,
- `score_delta` — what changed in the scorecard.

---

## 5.3 Dry-run mode

Every mutating tool should support:

```json
{
  "dry_run": true
}
```

Dry-run returns:

- expected events,
- expected diagnostics,
- expected score delta,
- expected invalidations,
- expected policy violations,
- no state mutation.

This allows safe exploration.

---

## 5.4 Domain-specific patch operations

Do not let agents freely edit raw JSON unless absolutely necessary.

Instead, expose high-level patch operations:

```json
{ "op": "quantize_layer_to_scale" }
{ "op": "add_drum_fill" }
{ "op": "add_transition_riser" }
{ "op": "increase_segment_density" }
{ "op": "harmonize_melody_to_chords" }
{ "op": "transpose_pattern" }
{ "op": "apply_motivic_variation" }
{ "op": "add_sidechain_pump" }
{ "op": "reduce_low_mid_mud" }
{ "op": "normalize_loudness_target" }
```

This reduces hallucination and improves determinism.

Raw note writing should still exist, but high-level operations are safer.

---

# 6. The Command → Evidence → Decision Pipeline

The core operational loop of Scorestrata should be explicit everywhere.

Every meaningful action should flow through this pipeline:

```text
Intent
  -> Command
  -> Policy Check
  -> Dry Run
  -> Scoped Compile
  -> Scoped Validate
  -> Evidence Generation
  -> Score Delta
  -> Candidate Evaluation
  -> Decision
  -> Apply / Reject
  -> Snapshot
  -> Receipt
  -> Ledger
  -> Lesson
```

This should become the canonical architecture pattern.

---

# 7. Compiler Architecture

## 7.1 IR pipeline

```text
Brief
  -> Brief IR
  -> Config IR
  -> Constraint IR
  -> Arrangement IR
  -> Pattern IR
  -> Timeline IR
  -> Mixer IR
  -> DSP IR
  -> Render Manifest
  -> Audio Artifacts
  -> Analysis IR
  -> Scorecard IR
  -> Diagnostic IR
  -> Patch IR
```

Each IR should be:

- versioned,
- serializable,
- hashable,
- inspectable,
- diffable.

---

## 7.2 Compiler passes

The compiler should not merely expand patterns. It should run passes.

### Structural passes

- resolve config,
- resolve palette,
- resolve layers,
- expand arrangement,
- assign segment boundaries,
- validate duration policy.

### Theory passes

- scale compliance,
- chord-tone alignment,
- bass/chord alignment,
- harmonic rhythm check,
- voice-leading lint.

### Rhythm passes

- groove consistency,
- drum pattern sanity,
- fill placement,
- syncopation range,
- density targets.

### Arrangement passes

- energy curve check,
- section contrast check,
- transition marker check,
- repetition/variation balance.

### Mix passes

- routing resolution,
- send/bus resolution,
- automation curve resolution,
- loudness target check,
- sidechain routing check.

### Render passes

- build DSP graph,
- build render manifest,
- cache lookup,
- stem scheduling,
- master render.

### Compiler pass observability

Each pass should emit:

- input hash,
- output hash,
- diagnostics,
- warnings,
- duration,
- invalidations,
- policy checks.

This makes compiler behavior debuggable.

---

## 7.3 Incremental compilation

Agents will iterate constantly. So the compiler must recompute only what changed:

- pattern,
- segment,
- layer,
- track,
- automation,
- mixer state,
- DSP graph subset.

Fast loops are a core architectural feature, not a performance nicety.

---

## 7.4 Diff compilation

Agents often need to know:

- what changed between candidates,
- which pass introduced this event,
- which patch caused this metric delta.

So support:

- candidate A vs candidate B compilation diffs,
- IR diffs,
- timeline event diffs,
- mixer state diffs,
- automation diffs.

---

## 7.5 Compiler diagnostics

Every compiler diagnostic should be actionable.

Bad:

```text
Invalid harmony.
```

Good:

```json
{
  "code": "HARMONY_SCALE_VIOLATION",
  "severity": "warning",
  "scope": {
    "segment_id": "chorus_1",
    "layer_id": "lead",
    "bar": 18
  },
  "message": "Lead contains C# in C minor on a strong beat.",
  "focus_uri": "project://proj_123/segments/chorus_1/layers/lead/notes?bar=18",
  "suggested_actions": [
    {
      "op": "quantize_layer_to_scale",
      "params": {
        "segment_id": "chorus_1",
        "layer_id": "lead",
        "strength": 1.0
      }
    }
  ]
}
```

This is the level of feedback agents can use.

---

# 8. Domain Model

## 8.1 Musical Entities

```text
Brief
Intent
ConstraintSet
Goal
StyleProfile
GenreProfile
Motif
MotifInstance
EnergyCurve
TransitionGraph
ProjectConfig
Palette
InstrumentDef
Segment
Layer
Track
Pattern
PatternInstance
Note
Step
Automation
FxChain
MixerState
```

## 8.2 Operational Entities

```text
Command
CommandReceipt
Patch
DryRunResult
Snapshot
Branch
Candidate
Experiment
Budget
PolicyProfile
PolicyDecision
RenderJob
Lease
TaskClaim
HumanReviewPacket
```

## 8.3 Evidence Entities

```text
Diagnostic
MetricSample
Evidence
MetricLineage
Scorecard
ValidationReport
AuditReport
ProvenanceManifest
RenderManifest
Artifact
ReplayJournal
```

## 8.4 Learning Entities

```text
Lesson
RepairTemplate
CalibrationProfile
ReferenceTrackProfile
PreferenceSignal
FailureTaxonomyEntry
```

## 8.5 Engineering Entities

```text
AcceptanceSpec
Task
EvalRun
GoldenTest
ContractTest
BenchmarkResult
TraceabilityEntry
ADR
```

---

## 8.6 Project entity

```rust
Project {
  id: ProjectId,
  name: String,
  created_at: Timestamp,
  brief: Brief,
  config: ProjectConfig,
  arrangement: Arrangement,
  tracks: Vec<Track>,
  patterns: Vec<Pattern>,
  mixer: MixerState,
  branches: Vec<Branch>,
  snapshots: Vec<Snapshot>,
  ledger: EventLedger,
  policy: PolicyProfile,
  budget: Budget,
  memory: ProjectMemory,
  eval_state: EvalState,
  provenance: ProvenanceManifest,
  status: ProjectStatus,
}
```

---

## 8.7 Brief

```rust
Brief {
  raw_text: String,
  genre: Option<String>,
  mood: Vec<String>,
  references: Vec<Reference>,
  duration_hint: Option<DurationHint>,
  delivery_target: DeliveryTarget,
  constraints: Vec<BriefConstraint>,
  parsed_intent: Intent,
  confidence: f32,
}
```

Example:

```json
{
  "raw_text": "Dark melodic techno with emotional chorus and driving drums",
  "genre": "melodic techno",
  "mood": ["dark", "driving", "emotional"],
  "delivery_target": "streaming",
  "constraints": [
    "must be danceable",
    "must have strong chorus lift"
  ]
}
```

---

## 8.8 ConstraintSet

This is essential for agentic composition.

```rust
ConstraintSet {
  hard: Vec<HardConstraint>,
  soft: Vec<SoftConstraint>,
}
```

Examples:

```json
{
  "hard": [
    "key == C minor",
    "bpm >= 120 && bpm <= 126",
    "time_signature == 4/4",
    "duration_bars == 64"
  ],
  "soft": [
    "chorus density > verse density",
    "lead chord-tone ratio > 0.65",
    "kick on every quarter note in chorus"
  ]
}
```

---

## 8.9 Motif and MotifInstance

```rust
Motif {
  id: MotifId,
  pitch_contour: Vec<i32>,
  rhythm_contour: Vec<u32>,
  interval_vector: Vec<i32>,
  tags: Vec<String>,
}
```

```rust
MotifInstance {
  id: MotifInstanceId,
  motif_id: MotifId,
  segment_id: SegmentId,
  layer_id: LayerId,
  transpose: i32,
  variation: VariationKind,
  similarity_score: f32,
}
```

This allows the validator to measure coherence:

- motif recurrence,
- variation amount,
- section-to-section relationship,
- melodic identity.

---

## 8.10 EnergyCurve

```rust
EnergyCurve {
  points: Vec<EnergyPoint>,
}
```

```rust
EnergyPoint {
  segment_id: SegmentId,
  target_energy: f32,
  predicted_energy: f32,
  measured_energy: Option<f32>,
}
```

Energy should be derived from:

- layer count,
- rhythmic density,
- velocity,
- spectral brightness,
- loudness,
- drum intensity,
- FX activity.

---

## 8.11 Candidate and Experiment

Agents should not mutate only one linear draft.

```rust
Candidate {
  id: CandidateId,
  branch_id: BranchId,
  parent_snapshot_id: SnapshotId,
  patch_set: Vec<Patch>,
  scorecard: Scorecard,
  render_artifact_ids: Vec<ArtifactId>,
  status: CandidateStatus,
}
```

```rust
Experiment {
  id: ExperimentId,
  hypothesis: String,
  candidates: Vec<CandidateId>,
  winner_candidate_id: Option<CandidateId>,
  lesson: Option<Lesson>,
}
```

This turns composition into a measurable search process.

---

## 8.12 Lesson

This is critical for learning.

```rust
Lesson {
  id: LessonId,
  scope: LessonScope,
  trigger: DiagnosticCode,
  successful_patch: Option<Patch>,
  failed_patches: Vec<Patch>,
  evidence_uris: Vec<String>,
  genre: Option<String>,
  confidence: f32,
}
```

Example:

```json
{
  "trigger": "TRANSITION_ENERGY_DELTA_LOW",
  "successful_patch": {
    "op": "add_transition_package",
    "params": {
      "fill": true,
      "riser": true,
      "impact": true
    }
  },
  "genre": "melodic techno",
  "confidence": 0.83
}
```

Over time, Scorestrata becomes better not only by rendering music, but by accumulating repair knowledge.

---

## 8.13 Memory Tiers

Memory should be structured into four tiers:

### 1. Working memory
- current summary,
- current diagnosis,
- active candidates,
- current budget state.

### 2. Episodic memory
- command history,
- experiment history,
- failure history,
- rollback history.

### 3. Semantic memory
- lessons,
- genre profiles,
- failure patterns,
- patch templates,
- metric explanations.

### 4. Procedural memory
- playbooks,
- repair strategies,
- composition routines,
- mixing routines,
- engineering workflows.

Agents improve faster when they can retrieve: what worked, in what genre, under what diagnostic, at what cost, with what confidence.

---

# 9. Storage Architecture

## 9.1 Project bundle

```text
my_song.ssproj/
  manifest.json
  config.toml
  policy.toml
  brief.json
  arrangement.json
  mixer.json
  ledger/
    events.jsonl
  state/
    current.json
  patterns/
    drums_01.json
    bass_01.json
  segments/
    intro.json
    chorus.json
  snapshots/
    snap_0001.json
    snap_0002.json
  branches/
    main.json
    candidate_001.json
  experiments/
  memory/
    lessons.json
    repair_templates.json
  calibration/
    genre_profile.json
    reference_profiles/
  evidence/
    metrics/
    diagnostics/
    comparisons/
  replay/
    journal.jsonl
  telemetry/
    latest.json
  eval/
    latest.json
  cache/
    objects/
```

---

## 9.2 Event ledger

Every mutation should append an event.

Example:

```json
{
  "event_id": "evt_01J...",
  "command_id": "cmd_01J...",
  "type": "pattern_updated",
  "project_id": "proj_123",
  "branch_id": "main",
  "snapshot_id": "snap_0042",
  "payload": {
    "pattern_id": "chorus_lead_A",
    "changes": [
      "quantized_notes",
      "updated_metadata"
    ]
  },
  "timestamp": "2026-08-01T12:00:01Z"
}
```

Benefits:

- rollback,
- audit,
- debugging,
- reproducibility,
- branch comparison,
- agent traceability.

---

## 9.3 Content-addressed cache

Use hashes for expensive artifacts.

Cache keys should include:

```text
config hash
snapshot hash
asset hashes
validator version
compiler version
audio engine version
render settings
policy profile
seed
```

Example cache entries:

```text
cache/objects/timeline/<hash>.json
cache/objects/validation/<hash>.json
cache/objects/render/<hash>.wav
cache/objects/stem/<hash>.wav
cache/objects/analysis/<hash>.json
```

This makes iterative agent loops much faster.

---

## 9.4 Deterministic Replay and Time Travel

Every project state should be reconstructible from:

- initial snapshot,
- event ledger,
- command history,
- asset hashes,
- engine versions,
- seeds,
- policy profile.

### Replay capabilities

- command replay,
- event reconstruction,
- snapshot bisect,
- deterministic simulation mode,
- "replay until failure",
- "replay with substituted policy",
- "replay with alternate seed".

Architectural rule: if an agent cannot replay a failure exactly, it cannot reliably learn from it.

---

## 9.5 Evidence Warehouse

Every metric should have:

```text
score
severity
scope
evidence URI
explanation
confidence
calibration context
suggested actions
lineage
```

The evidence warehouse should include:

- metric samples,
- diagnostic evidence,
- note-level pointers,
- bar/beat focus URIs,
- audio analysis excerpts,
- spectrogram references,
- render checksums,
- compiler pass traces,
- validator explanations,
- candidate comparisons,
- predicted vs actual deltas.

Agents should never have to infer "why" from opaque numbers. Evidence turns validation into repair.

---

## 9.6 Knowledge Graph / Ontology Layer

Model relationships such as:

- `Diagnostic` → affects → `Metric`,
- `Metric` → evidence in → `Segment`,
- `Patch` → fixes → `FailureClass`,
- `Lesson` → derived from → `Experiment`,
- `Task` → implements → `Spec`,
- `Spec` → verified by → `GoldenTest`,
- `Candidate` → child of → `Snapshot`,
- `PreferenceSignal` → calibrates → `GenreProfile`.

A graph layer makes the system more queryable and more learnable. Agents can ask:

- "Which patches most often fix weak transitions in melodic techno?",
- "Which metric regresses most when density increases?",
- "Which tasks block the validator milestone?"

---

# 10. MCP Architecture

The MCP section should be a full agent control surface.

## 10.1 MCP design principles

### Principle 1: Tools mutate, resources inspect

Every mutating tool returns a structured receipt; every resource returns inspectable state.

### Principle 2: Summaries first

Return summary-level data by default, then allow drill-down via scoped lenses.

### Principle 3: Dry-run support

Every mutating tool supports dry-run mode that returns expected effects without state mutation.

### Principle 4: Receipt-based mutation

Every mutating tool returns a receipt with status, score delta, evidence URIs, policy decisions, and next actions.

### Principle 5: Budget-awareness

Every session respects budget envelopes. Tools report cost in their receipts.

### Principle 6: Async jobs

Long-running operations return job handles. Agents poll for completion.

### Principle 7: Drill-down lenses

Summaries first, then scoped inspection via segment, layer, metric, and transition lenses.

### Principle 8: Diff and trace resources

Compare snapshots, candidates, and commands. Trace any command through the event ledger.

### Principle 9: Capability discovery

Agents discover available tools and features dynamically via `system://capabilities`.

### Principle 10: Pagination and token limits

Large resources paginate to avoid context overflow.

### Principle 11: Contract metadata

Every tool exposes its safety class, cost class, and side effects via the schema registry.

---

## 10.2 Agent Onboarding Packet

Every new agent should receive a formal onboarding packet containing:

- system map,
- capability manifest,
- schema catalog,
- tool catalog,
- playbooks,
- failure taxonomy,
- budget envelope,
- success criteria,
- golden examples,
- sample project,
- escalation rules,
- "do not do" list.

Most agent failure is not intelligence failure. It is orientation failure.

---

## 10.3 System resources

```text
system://capabilities
system://contracts
system://onboarding
system://playbooks
system://failure-taxonomy
system://policy-profiles
system://budget-templates
system://calibration-profiles
```

---

## 10.4 Project resources

```text
project://<id>/summary
project://<id>/budget
project://<id>/policy
project://<id>/state
project://<id>/provenance
project://<id>/ledger
project://<id>/branches
project://<id>/candidates
project://<id>/memory/lessons
project://<id>/memory/repair-templates
project://<id>/eval/latest
project://<id>/evidence/latest
project://<id>/telemetry/latest
project://<id>/diff/snapshots/<a>/<b>
project://<id>/diff/candidates/<a>/<b>
project://<id>/trace/command/<command_id>
project://<id>/replay/cursor/<cursor>
project://<id>/lens/segment/<segment_id>
project://<id>/lens/layer/<layer_id>
project://<id>/lens/metric/<metric_id>
project://<id>/lens/transition/<from>/<to>
project://<id>/render/jobs
project://<id>/render/jobs/<job_id>
project://<id>/human-review/latest
```

---

## 10.4 Engineering resources

```text
engineering://specs
engineering://specs/<spec_id>
engineering://tasks/next
engineering://tasks/<task_id>
engineering://eval/latest
engineering://golden/status
engineering://traceability/<feature_id>
```

---

## 10.5 MCP tools

### Governance tools

```text
get_capabilities
get_contracts
get_policy_profile
check_budget
```

### Project tools

```text
create_project
generate_config_from_brief
validate_config
approve_config
get_project_summary
get_onboarding_packet
```

### Composition tools

```text
plan_arrangement
create_pattern
write_pattern_notes
write_pattern_steps
apply_motivic_variation
place_pattern_in_segment
set_layer_instrument
set_segment_goals
set_energy_curve
```

### Patch tools

```text
apply_patch
dry_run_patch
quantize_layer_to_scale
harmonize_melody
add_drum_fill
add_transition_package
increase_segment_density
reduce_masking
normalize_loudness_target
```

### Validation tools

```text
run_validation
get_validation_report
get_audit_summary
explain_metric
get_root_causes
get_metric_evidence
suggest_fixes
compare_candidates
```

### Render tools

```text
render_project
render_segment
render_stems
render_candidate_preview
get_render_job
cancel_render_job
```

### Debug tools

```text
get_command_trace
get_snapshot_diff
get_candidate_diff
isolate_layer
solo_segment
explain_metric
get_failure_report
create_human_review_packet
```

### Learning tools

```text
get_lessons
record_lesson
get_repair_templates
update_calibration_profile
```

### Engineering tools

```text
get_next_task
claim_task
submit_task_result
run_acceptance_tests
get_eval_report
update_docs
create_adr
```

---

## 10.6 MCP tool result envelope

Every tool should return a consistent envelope.

```json
{
  "status": "succeeded",
  "summary": "Quantized lead layer to C minor in chorus_1.",
  "score_delta": {
    "harmony.scale_compliance": "+0.14"
  },
  "artifacts": [],
  "evidence_uris": [
    "project://proj_123/segments/chorus_1/layers/lead/notes"
  ],
  "focus_uris": [
    "project://proj_123/validation/latest"
  ],
  "next_actions": [
    "run_validation",
    "get_audit_summary"
  ],
  "cost": {
    "cpu_ms": 74,
    "render_seconds": 0
  },
  "warnings": []
}
```

This dramatically improves agent efficiency.

---

# 11. Validator Architecture

The validator is one of the strongest parts. It should be expanded into a diagnostic, calibrated, explainable quality engine.

## 11.1 Validator roles

The validator should act as:

1. **Linter** — detects rule violations.
2. **Tester** — runs pass/fail gates.
3. **Critic** — produces soft aesthetic scores.
4. **Debugger** — attributes root causes.
5. **Advisor** — suggests patches.
6. **Calibrator** — compares against genre/reference profiles.
7. **Regression Detector** — detects metric regressions after patches.

---

## 11.2 Validation levels

### Level 1: Config validation

Checks:

- schema validity,
- missing layers,
- invalid BPM,
- invalid key,
- invalid duration policy,
- missing assets,
- policy violations.

### Level 2: Symbolic validation

Checks:

- scale compliance,
- chord-tone ratio,
- progression validity,
- bass alignment,
- rhythmic density,
- drum sanity,
- motif coherence,
- section contrast,
- transition markers.

### Level 3: Timeline validation

Checks:

- unresolved pattern instances,
- overlapping invalid events,
- empty required segments,
- automation range errors,
- absolute timing errors.

### Level 4: Audio validation

Checks:

- loudness,
- true peak,
- clipping,
- crest factor,
- stereo correlation,
- spectral balance,
- transient health,
- masking.

### Level 5: Brief compliance

Checks:

- genre fit,
- mood fit,
- duration target,
- delivery target,
- section structure,
- reference-profile similarity.

### Level 6: Regression validation

Checks:

- did this patch worsen another metric?
- did chorus energy drop?
- did mix loudness drift?
- did motif coherence break?
- did render checksum unexpectedly change?

---

## 11.3 Diagnostic packets

Not just reports. Each diagnostic packet should contain:

- metric affected,
- severity,
- scope,
- evidence,
- root causes,
- suggested patches,
- expected improvement,
- confidence,
- cost estimate,
- policy implications.

### Example

```json
{
  "code": "TRANSITION_ENERGY_DELTA_LOW",
  "metric": "arrangement.transition_strength",
  "severity": "warning",
  "scope": {
    "from_segment_id": "verse_2",
    "to_segment_id": "chorus_2"
  },
  "evidence_uris": [
    "project://proj_123/energy-curve",
    "project://proj_123/segments/verse_2/density",
    "project://proj_123/segments/chorus_2/density"
  ],
  "root_causes": [
    "insufficient drum density increase",
    "no riser or fill marker",
    "harmonic rhythm unchanged"
  ],
  "suggested_patches": [
    {
      "op": "add_transition_package",
      "params": {
        "fill": true,
        "riser": true,
        "impact": true
      },
      "expected_delta": "+0.18",
      "confidence": 0.81
    }
  ]
}
```

---

## 11.4 Root-cause attribution

A metric score alone is not enough.

Example:

```json
{
  "metric": "harmony.scale_compliance",
  "score": 0.68,
  "root_causes": [
    {
      "cause": "non_scale_notes_on_strong_beats",
      "severity": "high",
      "scope": {
        "segment_id": "chorus_1",
        "layer_id": "lead",
        "bars": [18, 22]
      },
      "evidence_uri": "project://proj_123/segments/chorus_1/layers/lead/notes?bars=18,22",
      "suggested_patch": {
        "op": "quantize_layer_to_scale",
        "params": {
          "segment_id": "chorus_1",
          "layer_id": "lead",
          "strength": 1.0
        }
      }
    }
  ]
}
```

Now the agent can repair instead of guess.

---

## 11.5 Calibration-aware scoring

A metric should not mean the same thing in every genre.

Examples:

- high spectral brightness is good in techno,
- high spectral brightness may be wrong in ambient,
- high drum density is essential in house,
- low drum density may be correct in cinematic.

So every score should be interpreted through:

- genre profile,
- reference profile,
- project intent,
- delivery target,
- human overrides.

---

## 11.6 Calibration profiles

Metrics must be calibrated by genre.

A techno track and an ambient track should not be judged identically.

```toml
[genre_profile.melodic_techno]
bpm_range = [120, 126]
kick_pattern = "four_on_floor"
bass_syncopation = "medium_high"
chorus_energy_delta_min = 0.15
drum_density_min = 0.75
spectral_brightness_target = 0.62
loudness_target_lufs = -10.0

[genre_profile.ambient]
bpm_range = [70, 100]
kick_pattern = "sparse_or_none"
bass_syncopation = "low"
chorus_energy_delta_min = 0.05
drum_density_min = 0.10
spectral_brightness_target = 0.48
loudness_target_lufs = -16.0
```

This prevents the validator from becoming a single generic taste model.

---

## 11.7 Regression detection

The validator should detect not only "bad states," but also:

- metric regressions after patches,
- cross-layer side effects,
- mix drift,
- arrangement flattening,
- motif coherence loss,
- loudness target drift.

This is essential for iterative autonomy.

---

## 11.8 Taste Calibration and Preference Learning

Beyond genre profiles, this should become a full calibration system:

- reference corpus profiles,
- genre target distributions,
- metric weighting by genre,
- human preference capture,
- A/B decision logging,
- calibration drift detection,
- per-project taste overrides,
- confidence-weighted scoring.

### Preference signals

- human chose Candidate A over B,
- human rejected a patch,
- human edited a result,
- human approved final render,
- human overrode a metric weight.

Without calibration, the system optimizes toward generic "correctness," not taste.

---

# 12. Debug Plane

For autonomous agents, debugging must be first-class.

## 12.1 Debug capabilities

Agents should be able to:

- trace a command,
- diff two snapshots,
- diff two candidates,
- isolate a layer,
- solo a segment,
- mute layers,
- render A/B candidates,
- inspect metric evidence,
- inspect compiler pass logs,
- inspect DSP graph,
- inspect render manifest,
- inspect cache hits/misses,
- produce a human-review packet.

---

## 12.2 Debug resources

```text
project://<id>/trace/command/<command_id>
project://<id>/diff/snapshots/<a>/<b>
project://<id>/diff/candidates/<a>/<b>
project://<id>/isolate/layer/<layer_id>
project://<id>/solo/segment/<segment_id>
project://<id>/explain/metric/<metric_id>
project://<id>/compiler/log
project://<id>/render/manifest
project://<id>/dsp/graph
```

---

## 12.3 Human escalation packet

When an agent is stuck, it should produce a packet:

```text
human_review/
  summary.md
  latest_validation.json
  candidate_a.wav
  candidate_b.wav
  diff.json
  attempted_patches.json
  failed_metrics.json
  recommended_decisions.json
```

This makes human intervention efficient.

---

# 13. Agent Loop Architecture

## 13.1 L0 — Orientation Loop

This allows an agent to become operational quickly.

### Steps

- discover capabilities,
- read onboarding packet,
- inspect schemas,
- inspect budget envelope,
- inspect policy profile,
- inspect project summary,
- inspect top issues,
- select playbook.

### Exit criteria

Agent knows: what tools exist, what is safe, what is expensive, what state the project is in, and what the next best action is.

This dramatically reduces early agent friction.

---

## 13.2 L1 — Micro-Repair Loop

### Purpose

Fix localized issues quickly.

### Examples

- scale violation,
- weak drum fill,
- low chord-tone ratio,
- missing transition marker,
- loudness violation.

### Loop

```text
inspect metric
  -> locate evidence
  -> propose patch
  -> dry-run patch
  -> scoped compile
  -> scoped validate
  -> accept or reject
```

### Key requirement

This loop must be very fast. If micro-repair is slow, autonomy collapses.

---

## 13.3 L2 — Segment Development Loop

### Purpose

Build or improve a segment.

### Examples

- intro,
- verse,
- build,
- chorus,
- breakdown,
- outro.

### Loop

```text
define segment goal
  -> generate layer drafts in priority order
  -> compile segment
  -> validate segment
  -> generate N candidates
  -> score candidates
  -> select best
  -> validate transitions in/out
  -> snapshot
```

### Key requirement

Candidate branching must be native here.

---

## 13.4 L3 — Project Refinement Loop

### Purpose

Improve the whole track.

### Loop

```text
full compile
  -> full validation
  -> identify weakest segment/layer/metric
  -> run repair loop
  -> global mix pass
  -> render preview
  -> final validation
  -> finalize or escalate
```

### Key requirement

This loop must detect cross-section regressions. Example: improving chorus density should not destroy transition clarity; increasing loudness should not ruin transient health.

---

## 13.5 L4 — Learning Loop

### Purpose

Convert experience into competence.

### Loop

```text
collect outcome
  -> compare predicted vs actual score delta
  -> store lesson
  -> update patch-template ranking
  -> update genre profile confidence
  -> update failure-pattern memory
```

### Key requirement

Learning must be typed and retrievable. Not just "store a note." Store structured lessons with genre scope, trigger diagnostic, successful/failed patches, evidence URIs, and confidence.

---

## 13.6 L5 — Engineering Loop

### Purpose

Allow agents to improve Scorestrata itself.

### Loop

```text
claim task
  -> read executable spec
  -> create branch
  -> implement
  -> run unit tests
  -> run integration tests
  -> run golden tests
  -> run eval harness
  -> update docs
  -> open PR
  -> CI passes
  -> policy/human approval
  -> merge
  -> close task
```

### Key requirement

Engineering autonomy must be gated by verification.

---

## 13.7 Pseudocode

```text
initialize budget
create snapshot

while not done and budget.remaining:
    summary = get_project_summary()
    diagnosis = get_top_diagnosis(summary)

    if diagnosis.has_hard_failure:
        candidates = generate_structural_repairs(diagnosis)
    else:
        candidates = generate_improvement_patches(diagnosis)

    evaluated = []

    for candidate in candidates:
        dry = dry_run_patch(candidate)
        if dry.policy_ok:
            branch = create_candidate_branch(candidate)
            compile(branch, scope=diagnosis.scope)
            validate(branch, scope=diagnosis.scope)
            score = score_candidate(branch)
            evaluated.append((candidate, score))

    best = tournament_select(evaluated)
        # hard gate pass/fail
        # policy compliance
        # multi-objective scorecard
        # Pareto comparison
        # genre-weighted preference
        # cost-adjusted value
        # novelty penalty
        # lesson-backed confidence

    if best.improves_threshold:
        apply_patch(best)
        create_snapshot()
        record_lesson(diagnosis, best)
    else:
        increase_no_improvement_counter()

    if no_improvement_counter > limit:
        create_human_review_packet()
        break

    if overall_score >= target and hard_failures == 0:
        break

finalize_or_escalate()
```

---

# 14. Engineering Plane: Letting Agents Build Scorestrata Itself

Scorestrata must include an **Engineering Plane** so that AI agents not only use Scorestrata, but help build it.

---

## 14.1 Engineering Plane purpose

It allows agents to:

- read requirements,
- claim tasks,
- implement code,
- run tests,
- run golden evals,
- debug failures,
- open pull requests,
- update docs,
- verify acceptance criteria,
- progress through a roadmap autonomously.

---

## 14.2 Executable specs

Every feature should begin as an executable spec.

Example:

```yaml
id: SPEC-VALIDATOR-SCALE-001
title: Scale compliance detects out-of-scale notes
component: validator
priority: high
acceptance:
  - Given key C minor
  - And a lead pattern containing C# on beat 1
  - When symbolic validation runs
  - Then harmony.scale_compliance < 0.8
  - And diagnostic points to the offending note ID
tests:
  - cargo test -p scorestrata-validator scale_compliance
  - just golden validator/scale_compliance
docs:
  - docs/validation-metrics.md
```

This gives agents a clear definition of done.

---

## 14.3 Task graph

Use a DAG of engineering tasks.

```text
CORE-001 Define core IDs
CORE-002 Define error taxonomy
CORE-003 Define ProjectConfig schema
THEORY-001 Implement scales
THEORY-002 Implement chords
COMPILER-001 Expand arrangement
COMPILER-002 Generate timeline IR
VALIDATOR-001 Implement scale compliance
AUDIO-001 Implement WAV writer
MCP-001 Implement resource server
EVAL-001 Add golden project harness
```

Dependencies:

```text
CORE-001 -> CORE-003
THEORY-001 -> VALIDATOR-001
COMPILER-002 -> VALIDATOR-001
AUDIO-001 -> RENDER-001
MCP-001 -> AGENT-001
```

Agents can query:

```text
engineering://tasks/next
```

Response:

```json
{
  "task_id": "VALIDATOR-001",
  "title": "Implement scale compliance",
  "status": "ready",
  "dependencies_met": true,
  "acceptance_spec_uri": "engineering://specs/VALIDATOR-001",
  "commands": [
    "cargo test -p scorestrata-validator scale_compliance",
    "just golden validator/scale_compliance"
  ]
}
```

---

## 14.4 Definition of Done for every Scorestrata feature

Every feature should not be considered complete unless it has:

1. Rust implementation.
2. Public API docs.
3. JSON schema if data-facing.
4. Unit tests.
5. Property tests where relevant.
6. Golden example.
7. MCP tool/resource if agent-facing.
8. Error mapping.
9. CLI command if user-facing.
10. Benchmark if performance-critical.
11. Traceability entry.
12. Updated architecture doc or ADR.

This makes autonomous engineering possible.

---

## 14.5 Traceability Matrix

Every feature should link:

```text
spec -> task -> code -> tests -> golden -> MCP tool -> docs -> eval result
```

This is what makes autonomous engineering possible.

---

## 14.6 Engineering Sandboxing

If agents help build Scorestrata itself, you need stronger safety:

- isolated build sandbox,
- restricted network access,
- signed commits,
- branch protection,
- required CI gates,
- eval regression thresholds,
- human approval for core schema changes,
- secret scanning,
- dependency auditing,
- mutation testing for critical logic.

Self-building autonomy requires strong containment.

---

## 14.7 Engineering loop

```text
claim task
  -> read spec
  -> create branch
  -> implement
  -> run unit tests
  -> run integration tests
  -> run golden tests
  -> run eval harness
  -> update docs
  -> create PR
  -> CI passes
  -> human or policy approves
  -> merge
  -> close task
```

If failure occurs:

```text
capture failure
  -> classify failure
  -> generate repair patch
  -> rerun tests
  -> if stuck, escalate
```

---

# 15. Evidence & Learning Plane

## 15.1 Evidence

Every metric must have:

```text
score
severity
scope
evidence URI
explanation
confidence
calibration context
suggested actions
lineage
```

---

## 15.2 Provenance

Every artifact must be reproducible from:

```text
brief
config
policy
seed
assets
asset hashes
snapshot
compiler version
validator version
audio engine version
render settings
DSP graph version
```

---

## 15.3 Replay

Every project state should be reconstructible from:

- initial snapshot,
- event ledger,
- command history,
- asset hashes,
- engine versions,
- seeds,
- policy profile.

---

## 15.4 Calibration

Metrics must be calibrated by genre and refined by human preference.

---

## 15.5 Telemetry

Track:

- tool call counts,
- CPU seconds,
- render seconds,
- disk usage,
- cache hit rate,
- iteration count,
- no-improvement streaks,
- escalation frequency,
- rollback frequency,
- metric regression rate,
- cost per successful patch,
- cost per finalized project.

You cannot improve what you cannot measure. This is true for music quality and for agent efficiency.

---

# 16. Testing and Evaluation Architecture

## 16.1 Testing pyramid

```text
Golden Eval Tests
Mutation Tests
Contract Tests
Integration Tests
Property Tests
Unit Tests
```

---

## 16.2 Unit tests

For:

- scales,
- intervals,
- chords,
- time conversion,
- pattern utilities,
- FX math,
- normalization.

---

## 16.3 Property tests

Use `proptest` or similar.

Examples:

- converting bar/beat/tick to absolute tick and back is stable,
- transposing notes preserves rhythmic positions,
- scale quantization never produces out-of-scale notes,
- pattern instantiation preserves duration,
- compiler output event count is deterministic for same seed.

---

## 16.4 Snapshot tests

Use `insta` or similar.

Examples:

- timeline IR snapshot,
- validation report snapshot,
- MIDI export snapshot,
- audit summary snapshot.

---

## 16.5 Golden tests

Golden tests are essential.

Example golden project:

```text
eval/golden/melodic_techno_64_bar/
  config.toml
  expected_timeline.json
  expected_validation.json
  expected_master_checksum.json
  expected_midi.mid
```

Golden tests detect regressions in:

- compiler,
- validator,
- audio engine,
- export,
- scoring.

---

## 16.6 Contract tests

For MCP tools and CLI.

Examples:

- `create_project` returns valid schema,
- `apply_patch` returns receipt,
- `run_validation` returns report,
- `render_project` returns artifact manifest,
- invalid commands return structured errors.

---

## 16.7 Fuzzing

Fuzz:

- JSON parsing,
- TOML parsing,
- MIDI parsing,
- WAV parsing,
- sample loading,
- automation curve interpolation.

---

## 16.8 Eval harness

The eval harness measures both **Scorestrata quality** and **agent performance**.

### System eval metrics

- average validation score,
- hard failure rate,
- render determinism,
- golden pass rate,
- CPU time,
- render time,
- cache hit rate,
- regression rate,
- metric stability,
- schema/contract pass rate.

### Agent eval metrics

- task success rate,
- iterations to target,
- cost per successful patch,
- rollback rate,
- escalation rate,
- no-improvement streak frequency,
- context efficiency,
- tool error rate,
- time to first successful repair,
- time to finalizable project.

### Engineering eval metrics

- spec-to-merge time,
- CI pass rate,
- flaky test rate,
- golden regression rate,
- docs drift,
- test coverage,
- mutation score,
- PR rejection rate.

### Example eval structure

```text
eval/
  suite/
    melodic_techno_01/
    ambient_01/
    house_01/
    cinematic_01/
  metrics/
    baseline.json
    latest.json
  reports/
    regression.md
```

---

# 17. Determinism and Reproducibility

## 17.1 Reproducibility inputs

Every artifact must be reproducible from:

```text
brief
config
policy
seed
assets
asset hashes
snapshot
compiler version
validator version
audio engine version
render settings
DSP graph version
```

---

## 17.2 Render manifest

Every render should produce a manifest.

```json
{
  "artifact_id": "art_01J...",
  "type": "master_wav",
  "path": "renders/master.wav",
  "sha256": "abc123...",
  "sample_rate": 48000,
  "channels": 2,
  "duration_seconds": 158.4,
  "source_snapshot_id": "snap_0042",
  "command_id": "cmd_01J...",
  "compiler_version": "0.3.0",
  "audio_engine_version": "0.3.0",
  "seed": 1337,
  "asset_hashes": {
    "samples/kicks/techno_kick_01.wav": "def456..."
  }
}
```

---

## 17.3 DSP determinism rules

To keep renders reproducible:

- fixed sample rate,
- fixed block size,
- deterministic RNG,
- deterministic event ordering,
- deterministic summing order,
- no dependency on wall-clock time,
- no nondeterministic thread scheduling in audio graph,
- pinned asset versions,
- versioned DSP nodes.

For golden tests, use either:

- exact checksums on a reference platform, or
- tolerated numeric deltas across platforms.

---

# 18. Repository Layout

## 18.1 Recommended initial workspace

Start with fewer, stronger crates:

```text
scorestrata/
  Cargo.toml
  README.md
  justfile
  docs/
    architecture.md
    domain-model.md
    validation-metrics.md
    mcp-api.md
    agent-playbooks.md
    engineering-plane.md
    adr/
  crates/
    scorestrata-core/
    scorestrata-theory/
    scorestrata-compiler/
    scorestrata-validator/
    scorestrata-audio/
    scorestrata-storage/
    scorestrata-agent/
    scorestrata-mcp/
    scorestrata-cli/
    scorestrata-eval/
    scorestrata-testkit/
  assets/
    samples/
    presets/
  eval/
    golden/
    suite/
    reference/
  examples/
    projects/
    configs/
  tests/
    integration/
    contract/
    fuzz/
  dev/
    tasks/
    specs/
    playbooks/
```

Later, split as needed (only when boundaries stabilize):

```text
scorestrata-contracts
scorestrata-policy
scorestrata-replay
scorestrata-evidence
scorestrata-memory
scorestrata-assets
scorestrata-telemetry
scorestrata-sdk
scorestrata-sandbox
scorestrata-taskgraph
scorestrata-instruments
scorestrata-fx
scorestrata-export
scorestrata-debug
```

---

## 18.2 Crate responsibilities

### `scorestrata-core`

Contains:

- IDs,
- error taxonomy,
- project model,
- config,
- pattern,
- segment,
- receipts,
- snapshots,
- schema utilities.

### `scorestrata-theory`

Contains:

- scales,
- intervals,
- chords,
- roman analysis,
- voice-leading helpers,
- motif similarity.

### `scorestrata-compiler`

Contains:

- arrangement expansion,
- pattern instantiation,
- timeline IR,
- mixer IR,
- DSP IR,
- compiler passes,
- lints.

### `scorestrata-validator`

Contains:

- symbolic metrics,
- audio metrics,
- scoring profiles,
- diagnostics,
- root-cause attribution,
- calibration profiles.

### `scorestrata-audio`

Contains:

- instruments,
- FX,
- mixer,
- offline render,
- stems,
- loudness analysis.

### `scorestrata-storage`

Contains:

- project bundle,
- ledger,
- snapshots,
- cache,
- artifact manifests,
- evidence index,
- replay journal,
- SQLite index.

### `scorestrata-agent`

Contains:

- agent kernel,
- workflow state machine,
- budgets,
- candidate selection,
- candidate tournament,
- patch application,
- memory,
- escalation,
- multi-agent coordination.

### `scorestrata-mcp`

Contains:

- MCP server,
- tool adapters,
- resources,
- prompts,
- schema validation,
- async jobs,
- capability discovery.

### `scorestrata-cli`

Contains:

- CLI commands,
- human-facing tables,
- progress bars,
- agent commands.

### `scorestrata-eval`

Contains:

- golden harness,
- suite runner,
- regression reports,
- benchmarks,
- calibration evaluation.

### `scorestrata-testkit`

Contains:

- fixtures,
- fake projects,
- sample patterns,
- test utilities,
- snapshot helpers.

---

# 19. CLI Design

## 19.1 Core commands

```bash
ss new my_song --brief "dark melodic techno"
ss config generate my_song
ss config validate my_song
ss arrangement plan my_song
ss compile my_song
ss validate my_song --scope full
ss validate my_song --scope segment --segment chorus_1
ss audit my_song --segment chorus_1 --layer lead
ss render my_song --mode master
ss render my_song --mode stems
ss export midi my_song
ss mcp serve --stdio
```

## 19.2 Agent commands

```bash
ss agent step --project my_song
ss agent run --project my_song --max-iterations 8
ss agent repair --project my_song --scope segment --segment chorus_1
ss agent suggest-fixes --project my_song
ss agent compare --project my_song --candidate A --candidate B
```

## 19.3 Debug commands

```bash
ss doctor
ss trace command <command_id>
ss diff snapshots <a> <b>
ss diff candidates <a> <b>
ss explain metric <metric_id>
ss isolate layer <layer_id>
ss human-packet create --project my_song
ss replay --project my_song --from-snapshot <snapshot_id>
ss calibrate --project my_song
ss budget show --project my_song
ss policy show --project my_song
ss capabilities
```

## 19.4 Engineering commands

```bash
ss task next
ss task claim <task_id>
ss task validate <task_id>
ss eval run
ss eval compare
ss golden update
ss bench run
ss docs check
```

---

# 20. Roadmap

---

## Phase -1 — Contracts and Governance

Goal: make the system definable, discoverable, and testable.

### Tasks

- define core IDs,
- define schemas,
- define command/receipt contracts,
- define diagnostic contracts,
- define tool contracts,
- define capability discovery,
- define error taxonomy,
- generate JSON schemas,
- add contract tests.

### Exit criteria

- schemas are machine-readable,
- contracts are versioned,
- tools are discoverable,
- errors are classified.

---

## Phase 0 — Skeleton and Project Bundle

Goal: make the system operable at a basic level.

### Tasks

- create project bundle,
- implement storage primitives,
- implement ledger,
- implement snapshots,
- implement CLI doctor,
- implement sample project.

### Exit criteria

- can create project,
- can serialize/deserialize,
- can inspect state,
- can rollback.

---

## Phase 1 — Symbolic Compiler

Goal: make composition inspectable.

### Tasks

- implement musical time model,
- implement scales/chords,
- implement arrangement planner,
- implement pattern instances,
- implement timeline IR,
- implement MIDI export,
- implement compiler diagnostics,
- implement scoped compilation.

### Exit criteria

- deterministic timeline,
- inspectable events,
- exportable MIDI,
- scoped compilation works.

---

## Phase 2 — Validator v1

Goal: make quality measurable and actionable.

### Tasks

- implement scale compliance,
- implement chord-tone ratio,
- implement rhythmic density,
- implement drum sanity,
- implement segment length checks,
- implement transition heuristics,
- implement evidence URIs,
- implement suggested patches,
- implement scoring profile,
- implement diagnostics with focus URIs.

### Exit criteria

- validation report is actionable,
- agent can loop on validator output,
- diagnostics point to exact notes/bars/layers,
- hard gates are enforced.

---

## Phase 3 — MCP + Agent Kernel MVP

Goal: make the system externally operable.

### Tasks

- implement MCP resources,
- implement MCP tools,
- implement receipts,
- implement dry-run,
- implement budgets,
- implement snapshots,
- implement rollback,
- implement agent step/run commands.

### Exit criteria

- external agent can drive full symbolic workflow,
- agent can apply patches,
- agent can validate improvements,
- agent can rollback safely.

---

## Phase 4 — Evidence and Replay

Goal: make debugging and learning first-class.

### Tasks

- implement evidence warehouse,
- implement metric lineage,
- implement replay journal,
- implement trace command,
- implement diff snapshots,
- implement diff candidates,
- implement explain metric.

### Exit criteria

- any failure can be traced,
- any state can be replayed,
- any metric can be explained.

---

## Phase 5 — Basic Audio Engine

Goal: make the system audible.

### Tasks

- implement drum sampler,
- implement simple subtractive synth,
- implement bass synth,
- implement mixer,
- implement gain/pan/EQ/compressor/limiter/delay/reverb,
- implement offline WAV render,
- implement stem render,
- implement render manifest.

### Exit criteria

- can render master WAV,
- can render stems,
- render is deterministic,
- render cache works.

---

## Phase 6 — Audio Validator and Mix Metrics

Goal: make audio quality measurable.

### Tasks

- implement LUFS,
- implement true peak,
- implement clipping detection,
- implement crest factor,
- implement spectral balance,
- implement stereo correlation,
- implement mix diagnostics.

### Exit criteria

- agent can detect mix problems,
- agent can improve loudness/peak compliance,
- audio metrics are included in scorecard.

---

## Phase 7 — Candidate Branching and Learning

Goal: make exploration intelligent.

### Tasks

- implement candidate branches,
- implement candidate tournaments,
- implement patch templates,
- implement no-improvement detection,
- implement lesson storage,
- implement human escalation packet,
- implement memory tiers.

### Exit criteria

- agent can generate multiple candidates,
- agent can select best candidate,
- agent can learn from successful patches,
- agent can stop or escalate intelligently.

---

## Phase 8 — Engineering Plane

Goal: make Scorestrata self-buildable.

### Tasks

- implement task graph,
- implement executable specs,
- implement golden project harness,
- implement eval suite,
- implement regression reports,
- implement contract tests,
- implement docs checks,
- implement traceability matrix.

### Exit criteria

- agent can claim a task,
- agent can run acceptance tests,
- agent can verify feature completion,
- regressions are detected automatically.

---

## Phase 9 — Calibration, Preference, and Memory

Goal: make quality less generic.

### Tasks

- implement genre profiles,
- implement reference-track calibration,
- implement human preference capture,
- implement calibration drift detection,
- implement patch-template ranking,
- implement lesson confidence,
- implement brief-compliance scoring.

### Exit criteria

- validator adapts to genre,
- agent improves faster on repeated genre tasks,
- memory influences patch selection,
- human taste influences scoring.

---

## Minimum Viable Agentic Slice (MVAS)

Before the full MVP, prove the closed loop on a tiny vertical slice.

### Scope

An agent can:

1. receive a brief,
2. generate a valid config,
3. create an **8-bar loop**,
4. compile to timeline IR,
5. export MIDI,
6. run symbolic validation,
7. receive diagnostics,
8. apply at least 2 successful patches,
9. prove score improvement,
10. snapshot and rollback,
11. produce receipts and evidence,
12. stay within budget.

### Why 8 bars?

Because it is enough to test harmony, rhythm, layering, transition logic, validation, repair, receipts, and snapshots — but small enough to iterate fast.

### Then expand to full MVP

Once the 8-bar loop is strong, expand to:

- 16-bar structure,
- 3 segments,
- candidate branching,
- preview render,
- audio metrics,
- lessons,
- escalation.

Then later:

- 64-bar arrangement,
- stems,
- full audio validation,
- engineering plane,
- calibration,
- multi-agent coordination.

This is the fastest path to real autonomy.

---

# 21. Success Criteria

## MVAS success criteria

An autonomous agent can:

1. receive a brief for an 8-bar loop,
2. generate a valid config,
3. create 2–4 layers,
4. compile to timeline IR,
5. export MIDI,
6. run symbolic validation,
7. receive actionable diagnostics,
8. apply at least 2 successful patches,
9. prove score improvement,
10. snapshot and rollback,
11. stay within budget.

## MVP success criteria

An autonomous agent can:

1. receive a brief,
2. generate a valid config,
3. plan a 64-bar arrangement,
4. create 4–6 layers,
5. compose patterns for 3–5 segments,
6. compile to timeline IR,
7. export MIDI,
8. run symbolic validation,
9. apply at least 3 successful patches,
10. render a WAV master,
11. render stems,
12. pass hard gates,
13. improve overall score across iterations,
14. produce an audit report,
15. produce a provenance manifest,
16. stay within budget,
17. rollback from a failed candidate,
18. escalate if no improvement occurs.

That is a real agentic v1.

---

# 22. Key Architectural Patterns to Adopt

## 22.1 Summary-first context

Never return full project state by default.

Return:

```json
{
  "status": "SegmentValidationFailed",
  "overall_score": 0.68,
  "hard_failures": [],
  "top_issues": [],
  "focus_uris": [],
  "next_actions": []
}
```

Then allow drill-down.

---

## 22.2 Scoped validation

Support:

```text
--scope config
--scope arrangement
--scope segment
--scope layer
--scope transition
--scope mix
--scope full
--scope diff
```

This keeps loops fast.

---

## 22.3 Incremental compilation

Only recompile affected:

- patterns,
- segments,
- layers,
- tracks,
- automation,
- DSP nodes.

---

## 22.4 Cache everything expensive

Cache:

- timeline IR,
- validation reports,
- audio analysis,
- stem renders,
- master renders,
- metric scores.

---

## 22.5 Every metric must have evidence

No metric should exist without:

```text
score
severity
scope
evidence URI
explanation
confidence
suggested action
```

---

## 22.6 Every patch must be testable

Before applying:

```text
dry-run -> scoped compile -> scoped validate -> score delta -> policy check
```

---

## 22.7 Every failure must be classified

Use a failure taxonomy:

```text
CONFIG_INVALID
SCHEMA_VIOLATION
ASSET_MISSING
THEORY_SCALE_VIOLATION
THEORY_CHORD_MISALIGNMENT
RHYTHM_DENSITY_INVALID
TRANSITION_WEAK
MIX_TRUE_PEAK_VIOLATION
RENDER_FAILURE
TIMEOUT
BUDGET_EXCEEDED
POLICY_VIOLATION
INTERNAL_COMPILER_ERROR
```

Agents can then choose repair strategies.

---

# 23. Definition of "Agent-Ready"

A feature is agent-ready only if it satisfies all of the following:

1. It is discoverable via capabilities/contracts.
2. It has a schema.
3. It has a receipt.
4. It supports dry-run if mutating.
5. It produces evidence if evaluative.
6. It is reversible if state-changing.
7. It is replayable.
8. It is scoped.
9. It is budget-aware.
10. It is policy-aware.
11. It produces actionable diagnostics on failure.
12. It has tests.
13. It has docs.
14. It has traceability.

If not, it is not done.

---

# 24. MCP Prompt Pack

## Composition prompts

```text
compose_segment_from_goal
improve_segment_energy
create_chorus_lift
strengthen_drum_groove
generate_bass_movement
harmonize_lead
create_transition_package
add_ear_candy
finalize_arrangement
```

## Repair prompts

```text
fix_scale_compliance
fix_weak_transition
fix_low_chord_tone_ratio
fix_excessive_density
fix_loudness_violation
fix_true_peak_violation
fix_empty_segment
fix_unresolved_pattern_instances
```

## Debug prompts

```text
debug_validation_failure
explain_metric_regression
compare_candidate_branches
isolate_problem_layer
create_human_review_packet
```

## Engineering prompts

```text
implement_next_task
run_acceptance_tests
debug_failing_golden_test
update_schema_docs
create_adr_for_decision
prepare_pull_request
```

---

# 25. Risk Register

## Risk: Agent over-optimizes metrics

Mitigation:

- multi-objective scoring,
- genre calibration,
- reference tracks,
- human preference alignment,
- novelty/variety penalties,
- brief-compliance checks,
- Pareto comparison,
- lesson-backed confidence.

---

## Risk: Agent context overflow

Mitigation:

- summary-first resources,
- scoped lenses,
- pagination,
- compact note representations,
- focus URIs,
- next-action envelopes.

---

## Risk: Nondeterministic renders

Mitigation:

- render manifests,
- asset hashing,
- fixed DSP ordering,
- golden checksums,
- versioned engines,
- deterministic seeds.

---

## Risk: Agent mutates into broken state

Mitigation:

- snapshots,
- dry-run,
- policy checks,
- candidate branches,
- rollback,
- hard gates.

---

## Risk: Validator disagrees with human taste

Mitigation:

- genre profiles,
- calibration corpus,
- human override,
- human review packets,
- preference feedback loop,
- calibration drift detection.

---

## Risk: Engineering agents introduce regressions

Mitigation:

- executable specs,
- golden tests,
- contract tests,
- CI gates,
- human approval for core changes,
- eval regression thresholds,
- mutation testing.

---

## Risk: Architecture becomes too large

Mitigation:

- start with consolidated crates,
- defer GUI/plugins/realtime,
- enforce MVP gates,
- split crates only when boundaries stabilize.

---

## Risk: Contract drift

Mitigation:

- schema registry,
- contract tests,
- versioning,
- deprecation policy,
- generated clients.

---

## Risk: Agent disorientation

Mitigation:

- onboarding packet,
- capability discovery,
- summary-first resources,
- next-action hints,
- playbooks.

---

## Risk: Evidence poverty

Mitigation:

- evidence warehouse,
- metric lineage,
- focus URIs,
- explainable diagnostics.

---

## Risk: Replay failure

Mitigation:

- event sourcing,
- asset hashing,
- deterministic seeds,
- versioned engines,
- replay journal.

---

## Risk: Memory rot

Mitigation:

- confidence scoring,
- lesson expiration,
- genre-scoped lessons,
- outcome validation.

---

## Risk: Taste drift / metric hacking

Mitigation:

- multi-objective scoring,
- Pareto selection,
- genre calibration,
- human preference capture,
- novelty penalties.

---

# 26. Strong Architectural Recommendations

These are the most important refinements.

## Recommendation 1: Build audit-first, not audio-first

The first valuable version of Scorestrata is not a great-sounding DAW.

It is a system where an agent can:

- inspect a composition,
- understand why it fails,
- patch it,
- and prove improvement.

Audio can follow. Auditability must come first.

---

## Recommendation 2: Make the agent kernel first-class

Do not treat orchestration as an afterthought.

The agent kernel should be as important as the compiler or audio engine.

It owns:

- budgets,
- snapshots,
- candidate selection,
- repair loops,
- escalation,
- memory.

---

## Recommendation 3: Make every tool receipt-based

Agents need closed loops.

No mutating tool should return only "OK."

It must return:

- status,
- summary,
- snapshot,
- focus URIs,
- evidence URIs,
- next actions,
- cost,
- warnings,
- replay cursor.

---

## Recommendation 4: Make every mutation receipt-based and replayable

Every mutating tool should return a decision object, not just an acknowledgment.

Every state should be reconstructible from event history.

---

## Recommendation 5: Make validation diagnostic, not merely numeric

A score is not enough.

The validator must say:

- what failed,
- where,
- why,
- how severe,
- what patch might fix it,
- with what confidence,
- at what cost.

---

## Recommendation 6: Make every repair testable via dry-run and scoped validation

Before applying any patch, run:

```text
dry-run -> scoped compile -> scoped validate -> score delta -> policy check
```

---

## Recommendation 7: Make engineering autonomous through executable specs

If you want agents to help build Scorestrata, every task must have:

- acceptance criteria,
- tests,
- commands,
- docs,
- definition of done.

---

## Recommendation 8: Keep the core Rust, offline, deterministic, and MCP-first

Do not dilute the architecture with GUI, plugin hosting, realtime, or ML too early.

---

## Recommendation 9: Prove the loop on a tiny vertical slice before expanding

Build the MVAS (8-bar loop) first. Then expand to full songs.

---

## Recommendation 10: Make every capability contractually discoverable

Agents should never guess. They should discover.

---

# 27. Top 10 Architectural Priorities

If you want the highest-leverage refinements, do these first:

1. **Contract & Schema Registry**
2. **Receipt-based mutation everywhere**
3. **Evidence-rich diagnostics**
4. **Scoped compile / scoped validate**
5. **Deterministic replay**
6. **Budget and policy enforcement**
7. **Candidate branching**
8. **Memory / lessons**
9. **Golden eval harness**
10. **Minimum Viable Agentic Slice (8-bar loop)**

If you get these right, everything else becomes easier.

---

# 28. Recommended Immediate Next Steps

If starting implementation immediately, do these first.

## Step 1: Define the contract core

Create:

```text
scorestrata-core
scorestrata-contracts (or fold into core initially)
```

Define:

- `ProjectId`, `SegmentId`, `LayerId`, `PatternId`, etc.
- `Project`
- `ProjectConfig`
- `Pattern`
- `Segment`
- `Note`
- `Step`
- `Command`
- `CommandReceipt`
- `Diagnostic`
- `ValidationReport`
- `Scorecard`
- `Snapshot`
- `Artifact`
- `PolicyDecision`
- `Budget`
- `Evidence`

Generate JSON Schema for all.

---

## Step 2: Define the error taxonomy

Create a unified error model:

```rust
enum ScorestrataError {
    ConfigInvalid,
    SchemaViolation,
    AssetMissing,
    TheoryViolation,
    CompilerError,
    RenderError,
    ValidationError,
    PolicyViolation,
    BudgetExceeded,
    Timeout,
    Internal,
}
```

Each error should include:

- code,
- message,
- retryable flag,
- focus URI,
- suggested actions,
- causal context.

---

## Step 3: Build the symbolic compiler

Implement:

- bar/beat/tick model,
- PPQ conversion,
- arrangement expansion,
- pattern instantiation,
- timeline IR,
- MIDI export,
- scoped compile,
- compiler diagnostics.

---

## Step 4: Build validator v1

Implement:

- scale compliance,
- chord-tone ratio,
- rhythmic density,
- segment length check,
- layer presence check,
- transition heuristic,
- evidence URIs,
- suggested patches.

Make every diagnostic point to evidence.

---

## Step 5: Build MCP read/write surface

Expose:

```text
create_project
generate_config
plan_arrangement
create_pattern
write_pattern_notes
compile_project
run_validation
get_audit_summary
apply_patch
dry_run_patch
create_snapshot
rollback_to_snapshot
```

---

## Step 6: Build the evidence and replay layer

Implement:

- event ledger,
- snapshot graph,
- command trace,
- metric evidence,
- replay cursor,
- diff snapshots.

---

## Step 7: Build the eval harness

Create one golden project:

```text
eval/golden/melodic_techno_64_bar/
```

Ensure:

- config loads,
- timeline compiles,
- MIDI exports,
- validation report matches snapshot,
- CLI/MCP contract tests pass.

---

## Step 8: Build the agent repair loop

Implement:

```text
get_top_issue
suggest_fixes
dry_run_patch
apply_patch
validate_scoped
snapshot
rollback
```

Then run:

```bash
ss agent run --project my_song --max-iterations 5
```

---

# 29. Final Architecture Summary

The Scorestrata architecture should be described as:

```text
Scorestrata is a Rust-based, offline-first, deterministic composition compiler
and evidence engine for AI agents.

It transforms a brief into config, arrangement, patterns, timeline IR,
DSP graph, audio renders, and multi-objective validation reports.

It is governed by a Contract & Governance Plane with capabilities,
schemas, receipts, dry-run, budgets, policy checks, replay, and
drill-down evidence lenses.

Its Autonomy Plane enables agents to plan, compose, validate, repair,
compare, learn, and escalate safely.

Its Creation Plane handles the composition compiler, audio engine,
validator, storage, export, and debug lenses.

Its Engineering Plane enables agents to build Scorestrata itself through
executable specs, task graphs, golden tests, eval harnesses, and
CI-gated acceptance loops.

Its Evidence & Learning Plane provides deterministic replay,
metric lineage, calibration profiles, preference signals, and
cost telemetry.
```

The most important shift is:

> Do not build a DAW that agents can use.
> Build an agentic composition compiler that can also render audio.

That is the architecture that will give agents maximum efficacy with minimum friction.

---

# 30. Strongest Recommendations

If you want the architecture to truly maximize agent efficacy, adopt these six rules without compromise:

1. **Build audit-first, not audio-first.**
2. **Make every mutation receipt-based and replayable.**
3. **Make every metric evidence-backed and explainable.**
4. **Make every repair testable via dry-run and scoped validation.**
5. **Make every engineering task executable and verifiable.**
6. **Prove the loop on a tiny vertical slice before expanding.**
