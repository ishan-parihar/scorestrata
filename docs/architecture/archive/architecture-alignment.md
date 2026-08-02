Below is a refined systems-architecture blueprint for **Scorestrata**, treated not as “a DAW with an API,” but as an **agentic, deterministic, self-auditing composition compiler** that AI agents can operate, test, debug, validate, learn from, and eventually help build.

I will focus specifically on:

1. what to **keep**,
2. what to **expand**,
3. what to **add**,
4. what to **remove / defer / contract**,
5. what is **missing** from the current document,
6. how to shape the whole system around a high-efficacy **agentic loop architecture**,
7. and how to make the platform progressively **self-buildable** with minimal friction.

---

# 1. Executive Architectural Verdict

Your core thesis is correct:

> **Scorestrata should not be a DAW that agents can use.  
> It should be an agentic composition compiler that can also render audio.**

That is the right north star.

However, the current architecture still underweights a few decisive layers that determine whether agents can operate with **maximum efficacy and minimum friction**:

- **contract governance**
- **deterministic replay**
- **evidence-grade diagnostics**
- **agent runtime isolation and coordination**
- **memory and calibration**
- **self-engineering guardrails**
- **observability / cost control**
- **a smaller, harder vertical slice to prove the loop early**

So the refined thesis should be:

> **Scorestrata is a contract-first, event-sourced, multi-objective composition compiler for AI agents, with deterministic replay, diagnostic validation, policy-gated mutation, calibrated taste models, and an executable engineering plane.**

That version is stronger because it makes the system:

- more **operable** by agents,
- more **testable** by machines,
- more **debuggable** under autonomy,
- more **self-improving** over time,
- and more **safe** as scope grows.

---

# 2. The Refined North-Star Architecture

I would revise the architecture from **three coupled planes** to **five explicit planes**:

1. **Contract & Governance Plane**
2. **Autonomy Plane**
3. **Creation Plane**
4. **Engineering Plane**
5. **Evidence & Learning Plane**

This is the biggest structural refinement.

Your original model had:

- Creation Plane
- Autonomy Plane
- Engineering Plane
- Artifact / Evidence Layer

I would promote **Evidence** into a full plane, and add a **Contract & Governance Plane** at the front.

## Why?

Because agentic efficacy collapses when any of these are weak:

- agents cannot discover what is possible,
- tools return ambiguous results,
- state changes are hard to replay,
- metrics lack evidence,
- policies are implicit,
- learning is not accumulated,
- engineering changes are not verifiable.

So the refined architecture becomes:

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

---

# 3. What to Keep

These are architecturally sound and should remain unchanged or nearly unchanged.

| Existing Segment | Decision | Reason |
|---|---|---|
| Rust-first core | **Keep** | Determinism, performance, strong typing, long-term maintainability |
| Offline rendering first | **Keep** | Avoids realtime DSP complexity before the agentic loop is proven |
| Symbolic/audio separation | **Keep** | Essential for auditability, validation, and agent reasoning |
| Compiler mentality | **Keep** | Correct path: brief → config → arrangement → timeline → audio → analysis |
| MCP as first-class interface | **Keep** | Critical for agent operability |
| Multi-objective validation | **Keep** | Musical quality cannot be reduced to one scalar without losing taste |
| Snapshot/rollback support | **Keep** | Required for safe exploration |
| Project directory bundle | **Keep** | Human-debuggable, versionable, portable |
| CLI + MCP parity at service layer | **Keep** | Humans and agents should use the same domain services |
| MVP instrument/FX limitation | **Keep** | Prevents scope explosion |
| Receipt-based mutation model | **Keep** | One of the strongest ideas in the document |
| Dry-run mode | **Keep** | Essential for safe agent experimentation |
| Diagnostic-first validator | **Keep** | Agents need causes, not just scores |
| Executable specs for engineering | **Keep** | Necessary if agents will help build Scorestrata itself |

---

# 4. What to Expand

These exist in your document, but need to be made deeper, stricter, and more machine-actionable.

## 4.1 Interface Layer → **Contract Gateway**

Your current “Interface Layer” should become a full **Contract & Governance Plane**.

### Expand it to include:
- capability discovery
- schema registry
- tool contracts
- resource contracts
- error contracts
- idempotency semantics
- dry-run semantics
- cost classes
- safety classes
- pagination and context budgets
- version compatibility rules
- deprecation policy
- generated clients / SDKs

### Why this matters
Agents fail when tools are discovered by trial and error.

A strong contract layer gives agents:

- predictable behavior,
- machine-checkable schemas,
- safe exploration,
- fewer hallucinated calls,
- easier self-testing.

---

## 4.2 Application / Orchestration → **Agent Runtime**

Your “Application/Orchestration” layer should become a real **Agent Runtime**, not just an orchestration shell.

### Expand it to include:
- workflow state machine
- task DAG executor
- experiment runner
- candidate tournament engine
- lease-based coordination
- blackboard / shared context
- budget enforcement
- no-improvement detection
- escalation policy
- memory retrieval
- patch-template ranking
- replay-aware planning

### Why this matters
The agent kernel is not a helper component.  
It is one of the primary products.

If the agent runtime is weak, even a great compiler and validator will be underused.

---

## 4.3 Domain Model → **Evidence-Rich Domain Model**

Your added entities are good, but the model needs more **evidence and governance primitives**.

### Expand it to include:
- `Evidence`
- `MetricSample`
- `MetricLineage`
- `PolicyDecision`
- `BudgetLedgerEntry`
- `ReplayJournal`
- `PreferenceSignal`
- `CalibrationProfile`
- `ToolContract`
- `FailureClass`
- `RepairTemplate`
- `AgentRole`
- `Lease`
- `TaskClaim`

### Why this matters
Agents do not just need musical objects.  
They need objects for **decision-making, accountability, and learning**.

---

## 4.4 Composition Compiler → **Pass-Based, Incremental, Replayable Compiler**

Your compiler section is already strong. Expand it into a fully formalized pass pipeline.

### Expand it to include:
- pass registry
- pass ordering rules
- incremental invalidation
- IR hashing
- compiler trace logs
- deterministic pass seeds
- diff compilation
- scoped compilation
- compiler lint severity profiles
- optimization passes
- compile-time policy checks

### Why this matters
Agents need:
- fast scoped recompiles,
- explainable failures,
- reproducible outputs,
- and the ability to compare “before vs after” precisely.

---

## 4.5 Validator → **Diagnostic, Calibrated, Explainable Quality Engine**

This is one of the most important expansions.

Your validator should not merely score.  
It should become:

- linter
- tester
- critic
- debugger
- advisor
- calibrator
- regression detector

### Expand it to include:
- root-cause attribution
- evidence URIs
- metric explanations
- genre calibration
- reference-profile comparison
- confidence scores
- suggested patch ranking
- metric regression detection
- human preference alignment
- metric drift monitoring

### Why this matters
A score without evidence creates guessing.  
A score with evidence creates repair.

---

## 4.6 Storage → **Evidence & Provenance Store**

Your project bundle and ledger are good. Expand storage into a full evidence system.

### Expand it to include:
- event ledger
- snapshot graph
- content-addressed object cache
- artifact manifests
- evidence index
- metric history
- replay journal
- lesson store
- calibration store
- SQLite or embedded index for queries
- retention and pruning policies
- branch metadata
- experiment metadata

### Why this matters
Agents need to query:
- what changed,
- why it changed,
- what improved,
- what regressed,
- what was tried before,
- and what should be tried next.

---

## 4.7 MCP Server → **Agent Control Surface**

Your MCP section is already better than most. Expand it further.

### Expand it to include:
- capabilities
- onboarding
- contracts
- budgets
- telemetry
- replay cursors
- evidence lenses
- task endpoints
- memory endpoints
- calibration endpoints
- escalation endpoints
- async jobs
- pagination
- tool safety classes
- cost classes

### Why this matters
MCP should not just expose tools.  
It should expose the **entire operational surface** of the system to agents.

---

## 4.8 CLI → **Operator and Debug Console**

Your CLI plan is good. Expand it into a full operator console.

### Expand it to include:
- `ss doctor`
- `ss trace`
- `ss diff`
- `ss explain`
- `ss replay`
- `ss calibrate`
- `ss task`
- `ss eval`
- `ss golden`
- `ss bench`
- `ss human-packet`
- `ss budget`
- `ss policy`
- `ss capabilities`

### Why this matters
Humans and agents should share the same debug vocabulary.

---

## 4.9 Testing → **Verification and Evaluation System**

Your testing section should expand from “tests” into a full **verification and evaluation discipline**.

### Expand it to include:
- unit tests
- property tests
- snapshot tests
- contract tests
- golden tests
- fuzzing
- mutation testing
- eval benchmarks
- agent task benchmarks
- regression thresholds
- calibration stability tests
- render determinism tests
- docs/schema consistency checks

### Why this matters
If agents will help build Scorestrata, testing cannot be an afterthought.  
It must be the primary mechanism of autonomy.

---

# 5. What to Add

These are major missing or under-specified aspects that materially improve agentic efficacy.

---

## 5.1 Add a **Contract & Schema Registry**

This is one of the most important additions.

### What it does
Every tool, resource, command, receipt, diagnostic, patch, and artifact schema should be:

- versioned,
- machine-readable,
- discoverable,
- testable,
- compatible under explicit rules.

### It should include:
- JSON Schema / typed schemas
- Rust type generation
- client SDK generation
- MCP tool manifests
- stability levels
- deprecation metadata
- compatibility constraints
- cost and safety metadata

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

### Why this matters
This reduces agent friction dramatically.

Agents can:
- discover valid actions,
- avoid unsafe calls,
- understand costs,
- generate tests,
- and adapt to version changes.

---

## 5.2 Add **Deterministic Replay and Time Travel**

Your architecture mentions snapshots and ledgers, but not a first-class replay model.

### Add:
- replay journal
- command replay
- event reconstruction
- snapshot bisect
- deterministic simulation mode
- “replay until failure”
- “replay with substituted policy”
- “replay with alternate seed”

### Why this matters
Autonomous systems need debugging by replay.

If an agent cannot replay a failure exactly, it cannot reliably learn from it.

### Architectural rule
Every project state should be reconstructible from:

- initial snapshot
- event ledger
- command history
- asset hashes
- engine versions
- seeds
- policy profile

---

## 5.3 Add an **Evidence Warehouse**

Your current model has reports and artifacts.  
That is not enough.

You need a dedicated evidence layer.

### Evidence should include:
- metric samples
- diagnostic evidence
- note-level pointers
- bar/beat focus URIs
- audio analysis excerpts
- spectrogram references
- render checksums
- compiler pass traces
- validator explanations
- candidate comparisons
- predicted vs actual deltas

### Every metric should have:
- score
- severity
- scope
- evidence URI
- explanation
- confidence
- calibration context
- suggested actions
- lineage

### Why this matters
Agents should never have to infer “why” from opaque numbers.

Evidence turns validation into repair.

---

## 5.4 Add **Agent SDK and Agent Runtime Contracts**

Your document describes agent roles, but not enough runtime structure.

### Add:
- agent role contracts
- tool permission profiles
- lease semantics
- task claims
- blackboard reads/writes
- memory access scopes
- critic independence rules
- mutation rights
- escalation rights
- budget ownership

### Suggested role separation
| Role | Can Mutate? | Can Finalize? | Can Escalate? |
|---|---:|---:|---:|
| Producer Agent | limited | yes | yes |
| Planner Agent | yes | no | yes |
| Composer Agent | yes | no | yes |
| Mix Engineer Agent | yes | no | yes |
| Critic Agent | no | no | yes |
| Debugger Agent | limited | no | yes |
| Archivist Agent | metadata only | no | no |
| Engineer Agent | code branch only | no | yes |

### Why this matters
Multi-agent systems fail without clear authority boundaries.

---

## 5.5 Add **Memory Tiers**

Your “Lesson” concept is good, but memory should be structured more deeply.

### Add four memory tiers:

#### 1. Working memory
- current summary
- current diagnosis
- active candidates
- current budget state

#### 2. Episodic memory
- command history
- experiment history
- failure history
- rollback history

#### 3. Semantic memory
- lessons
- genre profiles
- failure patterns
- patch templates
- metric explanations

#### 4. Procedural memory
- playbooks
- repair strategies
- composition routines
- mixing routines
- engineering workflows

### Why this matters
Agents improve faster when they can retrieve:
- what worked,
- in what genre,
- under what diagnostic,
- at what cost,
- with what confidence.

---

## 5.6 Add **Taste Calibration and Preference Learning**

Your genre profiles are a great start, but this should become a full calibration system.

### Add:
- reference corpus profiles
- genre target distributions
- metric weighting by genre
- human preference capture
- A/B decision logging
- calibration drift detection
- per-project taste overrides
- confidence-weighted scoring

### Preference signals could include:
- human chose Candidate A over B
- human rejected a patch
- human edited a result
- human approved final render
- human overrode a metric weight

### Why this matters
Without calibration, the system optimizes toward generic “correctness,” not taste.

---

## 5.7 Add **Asset Intelligence Pipeline**

Your asset pipeline is mentioned, but should be expanded.

### Add:
- sample normalization
- loudness pre-analysis
- spectral tagging
- transient classification
- genre tagging
- license metadata
- provenance manifests
- derived sample caching
- sample compatibility rules
- asset health checks

### Why this matters
Agents need trustworthy assets.

Bad assets create nondeterminism, mix failures, and wasted iterations.

---

## 5.8 Add **Policy-as-Code and Safety Classes**

Your policy engine is mentioned, but should be formalized.

### Add:
- declarative policy profiles
- versioned policy packs
- policy simulation in dry-run
- policy violation evidence
- policy override audit trail
- safety classes for every tool
- protected branches / snapshots
- destructive command quarantine
- human approval gates for governance changes

### Safety classes
- `read`
- `write`
- `render`
- `destructive`
- `governance`
- `engineering`

### Why this matters
Autonomy without policy becomes expensive and unsafe.

---

## 5.9 Add **Observability and Cost Telemetry**

Your budget model is good, but telemetry should be broader.

### Add:
- tool call counts
- CPU seconds
- render seconds
- disk usage
- cache hit rate
- iteration count
- no-improvement streaks
- escalation frequency
- rollback frequency
- metric regression rate
- token cost estimates (if reported by agent)
- cost per successful patch
- cost per finalized project

### Why this matters
You cannot improve what you cannot measure.

This is true for music quality and for agent efficiency.

---

## 5.10 Add a **Knowledge Graph / Ontology Layer**

This is not in your document, and it is very valuable.

### Model relationships such as:
- `Diagnostic` → affects → `Metric`
- `Metric` → evidence in → `Segment`
- `Patch` → fixes → `FailureClass`
- `Lesson` → derived from → `Experiment`
- `Task` → implements → `Spec`
- `Spec` → verified by → `GoldenTest`
- `Candidate` → child of → `Snapshot`
- `PreferenceSignal` → calibrates → `GenreProfile`

### Why this matters
A graph layer makes the system more queryable and more learnable.

Agents can ask:
- “Which patches most often fix weak transitions in melodic techno?”
- “Which metric regresses most when density increases?”
- “Which tasks block the validator milestone?”

---

## 5.11 Add **Engineering Sandboxing and Governance**

If agents help build Scorestrata itself, you need stronger safety.

### Add:
- isolated build sandbox
- restricted network access
- signed commits
- branch protection
- required CI gates
- eval regression thresholds
- human approval for core schema changes
- secret scanning
- dependency auditing
- mutation testing for critical logic

### Why this matters
Self-building autonomy requires strong containment.

---

## 5.12 Add **Agent Onboarding Packet**

This is mentioned briefly, but should be formalized.

### Every new agent should receive:
- system map
- capability manifest
- schema catalog
- tool catalog
- playbooks
- failure taxonomy
- budget envelope
- success criteria
- golden examples
- sample project
- escalation rules
- “do not do” list

### Why this matters
Most agent failure is not intelligence failure.  
It is orientation failure.

---

# 6. What to Remove / Defer / Contract

Your document already has a good defer list. I would strengthen it.

## 6.1 Remove or Defer

| Segment | Decision | Reason |
|---|---|---|
| GUI / Tauri | **Defer** | Large surface area, low priority for agentic MVP |
| Native plugin hosting VST/CLAP | **Defer** | High complexity, low autonomy value early |
| Realtime playback | **Defer** | Offline render is enough for validation and iteration |
| Cloud collaboration | **Defer** | Single-project autonomy comes first |
| Multi-user editing | **Defer** | Adds concurrency pain before core value is proven |
| Recording / audio input | **Defer** | This is a composition engine first |
| Marketplace / preset store | **Defer** | Product layer, not core architecture |
| Full piano-roll UX | **Defer** | Agent-first, not GUI-first |
| ML aesthetic model as primary judge | **Defer** | Use explainable metrics first |
| Advanced FM synthesis | **Defer** | Start with simple deterministic sound sources |
| Complex velocity layers | **Defer** | MVP needs determinism more than realism |

---

## 6.2 Contract

These should be deliberately narrowed.

| Segment | Contract To | Reason |
|---|---|---|
| Python sidecar | Optional research only | Production core should remain Rust |
| CLI + MCP parity | Parity at service layer, not necessarily every command surface | Avoid duplicate surface bloat |
| Prompt packs | Generate from specs and patch templates | Prevent static prompt rot |
| Initial crate count | Fewer crates until boundaries stabilize | Avoid premature modularization |
| Early audio validation | Start with symbolic + proxy audio metrics | Prove loop before full DSP maturity |
| Early arrangement scope | Start with 8-bar or 16-bar loops, not full 64-bar songs | Reduce validation surface and prove repair loop |
| Early multi-agent support | Start with one kernel and role policies, not full distributed agents | Reduce coordination complexity |

---

# 7. The Most Important Missing Architecture Elements

If I had to name the highest-leverage missing pieces, they are these:

1. **Contract & Schema Registry**
2. **Deterministic Replay**
3. **Evidence Warehouse**
4. **Memory Tiers**
5. **Taste Calibration System**
6. **Agent Runtime Contracts**
7. **Knowledge Graph / Ontology**
8. **Engineering Sandboxing**
9. **Observability / Cost Telemetry**
10. **A smaller vertical slice to prove the loop early**

These are not optional if your goal is truly high-efficacy autonomy.

---

# 8. Refined Architectural Principles

Your five agentic design laws are good. I would expand them to ten.

Every feature must be:

1. **Inspectable**
2. **Receipt-producing**
3. **Reversible**
4. **Diagnosable**
5. **Measurable**
6. **Replayable**
7. **Policy-aware**
8. **Evidence-backed**
9. **Learnable**
10. **Contractually discoverable**

If a feature cannot satisfy these, it is not agent-ready.

---

# 9. Refined System Model: The “Command → Evidence → Decision” Pipeline

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

# 10. Refined Agentic Loop Architecture

Your inner/middle/outer/meta loops are correct.  
I would refine them into a **nested autonomy stack**.

---

## 10.1 L0 — Orientation Loop

This is missing as an explicit loop.

### Purpose
Allow an agent to become operational quickly.

### Steps
- discover capabilities
- read onboarding packet
- inspect schemas
- inspect budget envelope
- inspect policy profile
- inspect project summary
- inspect top issues
- select playbook

### Exit criteria
Agent knows:
- what tools exist,
- what is safe,
- what is expensive,
- what state the project is in,
- and what the next best action is.

### Why this matters
This dramatically reduces early agent friction.

---

## 10.2 L1 — Micro-Repair Loop

Your inner loop.

### Purpose
Fix localized issues quickly.

### Examples
- scale violation
- weak drum fill
- low chord-tone ratio
- missing transition marker
- loudness violation

### Loop
```text
diagnose
  -> locate evidence
  -> propose patch
  -> dry-run
  -> scoped compile
  -> scoped validate
  -> accept/reject
```

### Key requirement
This loop must be very fast.

If micro-repair is slow, autonomy collapses.

---

## 10.3 L2 — Segment Development Loop

Your middle loop.

### Purpose
Build or improve a segment.

### Examples
- intro
- verse
- build
- chorus
- breakdown
- outro

### Loop
```text
define segment goal
  -> generate layer drafts
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

## 10.4 L3 — Project Refinement Loop

Your outer loop.

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
This loop must detect cross-section regressions.

Example:
- improving chorus density should not destroy transition clarity
- increasing loudness should not ruin transient health

---

## 10.5 L4 — Learning Loop

Your meta loop, but should be expanded.

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
Learning must be typed and retrievable.

Not just “store a note.”  
Store structured lessons.

---

## 10.6 L5 — Engineering Loop

This is your most ambitious loop.

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

# 11. Refined Agent Kernel Design

Your agent kernel section is strong. I would refine it further.

The kernel should be a **deterministic workflow executor**, not a vague “agent loop.”

## Core kernel responsibilities

- maintain project state machine
- select next action
- enforce budgets
- choose repair vs generation strategy
- create candidate branches
- run scoped compile/validate cycles
- evaluate score deltas
- detect stagnation
- trigger escalation
- record lessons
- preserve replayability

---

## 11.1 Add a Decision Policy Layer Inside the Kernel

The kernel should not choose actions arbitrarily.

It should use:

- diagnostic severity
- expected score delta
- patch cost
- patch success probability
- genre calibration
- lesson confidence
- budget remaining
- no-improvement count

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

---

## 11.2 Add Candidate Tournament Logic

Candidates should not be selected by one scalar alone.

Use:

1. hard gate pass/fail
2. policy compliance
3. multi-objective scorecard
4. Pareto comparison
5. genre-weighted preference
6. cost-adjusted value
7. novelty penalty
8. lesson-backed confidence

This prevents metric hacking.

---

## 11.3 Add Stagnation Detection

You mention no-improvement rounds. Expand this.

### Track:
- no improvement streak
- oscillating metric behavior
- repeated failed patches
- rising cost per improvement
- regression after apparent progress
- validator/human disagreement

### Responses:
- switch strategy
- widen candidate diversity
- reduce scope
- request human review
- quarantine project state

---

# 12. Refined State Machine

Your project state machine is good. I would add a few missing states.

Add:

- `Compiling`
- `Validating`
- `Rendering`
- `AwaitingHumanDecision`
- `Replaying`
- `Calibrating`
- `Experimenting`
- `Quarantined`
- `NeedsPolicyReview`

A more complete high-level state machine:

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

# 13. Refined Domain Model

Your entity list is already strong. I would refine it into categories.

## 13.1 Musical Entities
- Brief
- Intent
- ConstraintSet
- Goal
- StyleProfile
- GenreProfile
- Motif
- MotifInstance
- EnergyCurve
- TransitionGraph
- Segment
- Layer
- Pattern
- PatternInstance
- Note
- Step
- Automation

## 13.2 Operational Entities
- Command
- CommandReceipt
- Patch
- DryRunResult
- Snapshot
- Branch
- Candidate
- Experiment
- Budget
- PolicyProfile
- PolicyDecision
- Lease
- TaskClaim

## 13.3 Evidence Entities
- Diagnostic
- MetricSample
- Evidence
- Scorecard
- ValidationReport
- AuditReport
- ProvenanceManifest
- RenderManifest
- ReplayJournal

## 13.4 Learning Entities
- Lesson
- RepairTemplate
- CalibrationProfile
- ReferenceTrackProfile
- PreferenceSignal
- FailureTaxonomyEntry

## 13.5 Engineering Entities
- AcceptanceSpec
- Task
- EvalRun
- GoldenTest
- ContractTest
- BenchmarkResult
- TraceabilityEntry
- ADR

---

# 14. Refined Receipt Architecture

Your receipt model is one of the best parts. I would make it even stronger.

Every mutating tool should return:

```json
{
  "status": "succeeded",
  "summary": "Quantized lead layer to C minor in chorus_1.",
  "command_id": "cmd_01J...",
  "snapshot_id": "snap_0042",
  "branch_id": "branch_main",
  "dry_run": false,
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

## Add:
- `evidence_uris`
- `policy_decisions`
- `replay_cursor`
- `confidence`
- `expected_vs_actual_delta` (when available)

This makes receipts not just acknowledgments, but **decision objects**.

---

# 15. Refined MCP Architecture

Your MCP direction is excellent. I would refine it into a full agent control surface.

---

## 15.1 Add System Resources

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

## 15.2 Add Project Resources

```text
project://<id>/summary
project://<id>/state
project://<id>/budget
project://<id>/policy
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
project://<id>/human-review/latest
```

---

## 15.3 Add Engineering Resources

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

## 15.4 Add Tools

### Governance
- `get_capabilities`
- `get_contracts`
- `get_policy_profile`
- `check_budget`

### Composition
- `generate_config_from_brief`
- `plan_arrangement`
- `create_pattern`
- `write_pattern_notes`
- `write_pattern_steps`
- `apply_motivic_variation`
- `set_energy_curve`

### Repair
- `dry_run_patch`
- `apply_patch`
- `suggest_fixes`
- `compare_candidates`

### Validation
- `run_validation`
- `explain_metric`
- `get_root_causes`
- `get_metric_evidence`

### Debug
- `trace_command`
- `diff_snapshots`
- `isolate_layer`
- `solo_segment`
- `create_human_review_packet`

### Learning
- `get_lessons`
- `record_lesson`
- `get_repair_templates`
- `update_calibration_profile`

### Engineering
- `get_next_task`
- `claim_task`
- `submit_task_result`
- `run_acceptance_tests`
- `get_eval_report`

---

# 16. Refined Validator Architecture

Your validator is one of the strongest areas. I would make it the crown jewel.

---

## 16.1 Validator Should Produce “Diagnostic Packets”

Not just reports.

Each diagnostic packet should contain:

- metric affected
- severity
- scope
- evidence
- root causes
- suggested patches
- expected improvement
- confidence
- cost estimate
- policy implications

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

## 16.2 Add Calibration-Aware Scoring

A metric should not mean the same thing in every genre.

### Example
- high spectral brightness is good in techno
- high spectral brightness may be wrong in ambient
- high drum density is essential in house
- low drum density may be correct in cinematic

So every score should be interpreted through:

- genre profile
- reference profile
- project intent
- delivery target
- human overrides

---

## 16.3 Add Regression Detection

The validator should detect not only “bad states,” but also:

- metric regressions after patches
- cross-layer side effects
- mix drift
- arrangement flattening
- motif coherence loss
- loudness target drift

This is essential for iterative autonomy.

---

# 17. Refined Compiler Architecture

Your compiler pipeline is good. I would add three major refinements.

---

## 17.1 Make Compilation Incremental by Default

Agents will iterate constantly.

So the compiler must recompute only what changed:

- pattern
- segment
- layer
- track
- automation
- mixer state
- DSP graph subset

### Why
Fast loops are a core architectural feature, not a performance nicety.

---

## 17.2 Make Compiler Passes Observable

Each pass should emit:

- input hash
- output hash
- diagnostics
- warnings
- duration
- invalidations
- policy checks

This makes compiler behavior debuggable.

---

## 17.3 Add “Diff Compilation”

Agents often need to know:

- what changed between candidates?
- which pass introduced this event?
- which patch caused this metric delta?

So support:

- candidate A vs candidate B compilation diffs
- IR diffs
- timeline event diffs
- mixer state diffs
- automation diffs

---

# 18. Refined Storage Architecture

Your project bundle is good. I would refine it into a more explicit evidence store.

## Recommended bundle structure

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
  segments/
  assets/
  renders/
  reports/
  snapshots/
  branches/
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

# 19. Refined Repository Layout

Your proposed workspace is reasonable, but I would adjust it.

## Start with fewer, stronger crates

```text
scorestrata/
  Cargo.toml
  README.md
  justfile
  docs/
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
  eval/
  examples/
  tests/
  dev/
```

## Add later, only when boundaries stabilize

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
```

## Why contract first?
If you add `scorestrata-contracts` early, that is reasonable.  
But avoid premature crate explosion.

---

# 20. The Most Important Refinement: Build Around a Smaller Vertical Slice

Your current roadmap is good, but it risks proving the full system too late.

I would introduce a **Minimum Viable Agentic Slice (MVAS)** before the full MVP.

---

## 20.1 Minimum Viable Agentic Slice

### Goal
Prove the closed loop as early as possible.

### Scope
An agent can:

1. receive a brief
2. generate a valid config
3. create an **8-bar loop**
4. compile to timeline IR
5. export MIDI
6. run symbolic validation
7. receive diagnostics
8. apply at least 2 successful patches
9. prove score improvement
10. snapshot and rollback
11. produce receipts and evidence
12. stay within budget

### Why 8 bars?
Because it is enough to test:
- harmony
- rhythm
- layering
- transition logic
- validation
- repair
- receipts
- snapshots

But small enough to iterate fast.

---

## 20.2 Then Expand to MVP

Once the 8-bar loop is strong, expand to:

- 16-bar structure
- 3 segments
- candidate branching
- preview render
- audio metrics
- lessons
- escalation

Then later:

- 64-bar arrangement
- stems
- full audio validation
- engineering plane
- calibration
- multi-agent coordination

This is the fastest path to real autonomy.

---

# 21. Refined Roadmap

I would revise your roadmap into a more agent-first sequence.

---

## Phase -1 — Contracts and Governance

### Goal
Make the system definable, discoverable, and testable.

### Tasks
- define core IDs
- define schemas
- define command/receipt contracts
- define diagnostic contracts
- define tool contracts
- define capability discovery
- define error taxonomy
- generate JSON schemas
- add contract tests

### Exit criteria
- schemas are machine-readable
- contracts are versioned
- tools are discoverable
- errors are classified

---

## Phase 0 — Skeleton and Project Bundle

### Goal
Make the system operable at a basic level.

### Tasks
- create project bundle
- implement storage primitives
- implement ledger
- implement snapshots
- implement CLI doctor
- implement sample project

### Exit criteria
- can create project
- can serialize/deserialize
- can inspect state
- can rollback

---

## Phase 1 — Symbolic Compiler

### Goal
Make composition inspectable.

### Tasks
- musical time model
- scales/chords
- arrangement expansion
- pattern instantiation
- timeline IR
- MIDI export
- compiler diagnostics

### Exit criteria
- deterministic timeline
- inspectable events
- exportable MIDI
- scoped compilation works

---

## Phase 2 — Validator v1

### Goal
Make quality measurable and actionable.

### Tasks
- scale compliance
- chord-tone ratio
- rhythmic density
- drum sanity
- segment length checks
- transition heuristics
- evidence URIs
- suggested patches

### Exit criteria
- diagnostics point to exact evidence
- agent can repair from validator output
- hard gates enforced

---

## Phase 3 — MCP + Agent Kernel MVP

### Goal
Make the system externally operable.

### Tasks
- MCP resources
- MCP tools
- receipts
- dry-run
- budgets
- snapshots
- rollback
- agent step/run commands

### Exit criteria
- external agent can drive full symbolic workflow
- agent can patch and validate
- agent can rollback safely

---

## Phase 4 — Evidence and Replay

### Goal
Make debugging and learning first-class.

### Tasks
- evidence warehouse
- metric lineage
- replay journal
- trace command
- diff snapshots
- diff candidates
- explain metric

### Exit criteria
- any failure can be traced
- any state can be replayed
- any metric can be explained

---

## Phase 5 — Basic Audio Engine

### Goal
Make the system audible.

### Tasks
- drum sampler
- simple synth
- bass synth
- mixer
- basic FX
- offline WAV render
- stem render
- render manifest

### Exit criteria
- deterministic master render
- deterministic stems
- cache works

---

## Phase 6 — Audio Validator

### Goal
Make audio quality measurable.

### Tasks
- LUFS
- true peak
- clipping
- crest factor
- spectral balance
- stereo correlation
- mix diagnostics

### Exit criteria
- agent can detect and repair mix issues
- audio metrics enter scorecard

---

## Phase 7 — Candidate Branching and Learning

### Goal
Make exploration intelligent.

### Tasks
- branches
- candidate tournaments
- experiments
- lessons
- repair templates
- no-improvement detection
- escalation packet

### Exit criteria
- agent compares candidates
- agent learns from outcomes
- agent escalates intelligently

---

## Phase 8 — Engineering Plane

### Goal
Make Scorestrata self-buildable.

### Tasks
- executable specs
- task graph
- golden harness
- eval suite
- regression reports
- contract tests
- docs checks
- traceability matrix

### Exit criteria
- agent can claim task
- agent can verify completion
- regressions are detected automatically

---

## Phase 9 — Calibration and Preference

### Goal
Make quality less generic.

### Tasks
- genre profiles
- reference profiles
- preference capture
- calibration drift detection
- patch-template ranking
- lesson confidence

### Exit criteria
- validator adapts to genre
- agent improves faster over time
- human taste influences scoring

---

# 22. Refined Definition of “Agent-Ready”

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

# 23. Refined Multi-Agent Architecture

Your role list is good. I would refine the coordination model.

---

## 23.1 Use a Task Board + Blackboard Model

### Task board
- ready
- claimed
- blocked
- review
- done

### Blackboard
- current diagnosis
- active candidates
- top issues
- lesson hints
- escalation flags
- budget state

---

## 23.2 Use Leases

Every claimed task should have:

- task ID
- agent ID
- lease expiration
- scope
- allowed tools
- allowed mutations

This prevents conflicts.

---

## 23.3 Separate Critic from Mutator

The critic agent should not mutate project state.

This preserves evaluation integrity.

---

## 23.4 Add a Merge Policy

When candidates compete:

- loser branches are archived
- winner is merged into main branch
- comparison evidence is stored
- lesson is recorded
- scorecard history is preserved

---

# 24. Refined Engineering Plane

This is one of the most important parts of your vision.

To make it real, the engineering plane must be stricter than a normal dev workflow.

---

## 24.1 Every Feature Starts as an Executable Spec

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

---

## 24.2 Every Task Must Have a Definition of Done

A task is done only if it has:

- implementation
- unit tests
- property tests where relevant
- golden example
- schema if data-facing
- MCP surface if agent-facing
- CLI surface if user-facing
- docs
- error mapping
- benchmark if performance-critical
- traceability entry
- updated architecture note or ADR

---

## 24.3 Add Traceability Matrix

Every feature should link:

```text
spec -> task -> code -> tests -> golden -> MCP tool -> docs -> eval result
```

This is what makes autonomous engineering possible.

---

# 25. Refined Evaluation Architecture

Your eval harness should measure both:

1. **Scorestrata quality**
2. **agent performance**

---

## 25.1 System Eval Metrics

- average validation score
- hard failure rate
- render determinism
- golden pass rate
- CPU time
- render time
- cache hit rate
- regression rate
- metric stability
- schema/contract pass rate

---

## 25.2 Agent Eval Metrics

- task success rate
- iterations to target
- cost per successful patch
- rollback rate
- escalation rate
- no-improvement streak frequency
- context efficiency
- tool error rate
- time to first successful repair
- time to finalizable project

---

## 25.3 Engineering Eval Metrics

- spec-to-merge time
- CI pass rate
- flaky test rate
- golden regression rate
- docs drift
- test coverage
- mutation score
- PR rejection rate

---

# 26. Refined Risk Register

Your risks are good. I would add these.

---

## Risk: Contract drift
### Mitigation
- schema registry
- contract tests
- versioning
- deprecation policy
- generated clients

---

## Risk: Agent disorientation
### Mitigation
- onboarding packet
- capability discovery
- summary-first resources
- next-action hints
- playbooks

---

## Risk: Evidence poverty
### Mitigation
- evidence warehouse
- metric lineage
- focus URIs
- explainable diagnostics

---

## Risk: Replay failure
### Mitigation
- event sourcing
- asset hashing
- deterministic seeds
- versioned engines
- replay journal

---

## Risk: Memory rot
### Mitigation
- confidence scoring
- lesson expiration
- genre-scoped lessons
- outcome validation

---

## Risk: Engineering autonomy introduces regressions
### Mitigation
- executable specs
- golden tests
- eval thresholds
- sandboxing
- human approval gates

---

## Risk: Taste drift / metric hacking
### Mitigation
- multi-objective scoring
- Pareto selection
- genre calibration
- human preference capture
- novelty penalties

---

# 27. What I Would Prioritize Above Everything Else

If you want the highest-leverage refinements, do these first:

## Top 10 architectural priorities

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

If I were starting implementation immediately, I would do this in order.

---

## Step 1 — Define the contract core

Create:

- `scorestrata-core`
- `scorestrata-contracts` (or fold into core initially)

Define:

- IDs
- Project
- ProjectConfig
- Pattern
- Segment
- Note
- Step
- Command
- CommandReceipt
- Diagnostic
- ValidationReport
- Scorecard
- Snapshot
- Artifact
- PolicyDecision
- Budget
- Evidence

Generate JSON Schema for all.

---

## Step 2 — Define the error taxonomy

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

- code
- message
- retryable flag
- focus URI
- suggested actions
- causal context

---

## Step 3 — Build the symbolic compiler

Implement:

- bar/beat/tick model
- PPQ conversion
- arrangement expansion
- pattern instantiation
- timeline IR
- MIDI export
- scoped compile
- compiler diagnostics

---

## Step 4 — Build validator v1

Implement:

- scale compliance
- chord-tone ratio
- rhythmic density
- segment length check
- layer presence check
- transition heuristic
- evidence URIs
- suggested patches

---

## Step 5 — Build MCP read/write surface

Expose:

- `create_project`
- `generate_config`
- `plan_arrangement`
- `create_pattern`
- `write_pattern_notes`
- `compile_project`
- `run_validation`
- `get_audit_summary`
- `apply_patch`
- `dry_run_patch`
- `create_snapshot`
- `rollback_to_snapshot`

---

## Step 6 — Build the evidence and replay layer

Implement:

- event ledger
- snapshot graph
- command trace
- metric evidence
- replay cursor
- diff snapshots

---

## Step 7 — Build the agent repair loop

Implement:

- `get_top_issue`
- `suggest_fixes`
- `dry_run_patch`
- `apply_patch`
- `validate_scoped`
- `snapshot`
- `rollback`

Then run:

```bash
ss agent run --project my_song --max-iterations 5
```

---

# 29. Final Refined Architecture Statement

I would summarize the refined architecture like this:

> **Scorestrata is a Rust-based, offline-first, deterministic composition compiler and evidence engine for AI agents. It transforms a brief into config, arrangement, patterns, timeline IR, DSP graph, audio renders, and multi-objective validation reports. It is governed by a contract-first interface layer with capabilities, schemas, receipts, dry-run, budgets, policy checks, replay, and drill-down evidence lenses. Its autonomy plane enables agents to plan, compose, validate, repair, compare, learn, and escalate safely. Its engineering plane enables agents to build Scorestrata itself through executable specs, task graphs, golden tests, eval harnesses, and CI-gated acceptance loops.**

And the single most important architectural shift remains:

> **Do not build a DAW that agents can use.  
> Build an agentic composition compiler that can also render audio.**

---

# 30. My Strongest Recommendations

If you want the architecture to truly maximize agent efficacy, adopt these six rules without compromise:

1. **Build audit-first, not audio-first.**
2. **Make every mutation receipt-based and replayable.**
3. **Make every metric evidence-backed and explainable.**
4. **Make every repair testable via dry-run and scoped validation.**
5. **Make every engineering task executable and verifiable.**
6. **Prove the loop on a tiny vertical slice before expanding.**

---

If you want, I can next turn this into one of the following:

1. a **revised version of `architecture.md`** with these changes integrated,  
2. a **target crate-by-crate architecture breakdown**,  
3. a **phase-by-phase implementation roadmap with acceptance criteria**, or  
4. a **MCP tool/resource contract specification** for the agent-facing surface.