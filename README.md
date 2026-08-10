# Scorestrata

> **A contract-first, event-sourced composition compiler for deterministic music generation.** Describe a track in plain language — to an agent over MCP or an operator via the `ss` CLI — and Scorestrata plans a layered arrangement, compiles it to a timeline IR, and renders a WAV with per-layer FX, automation, and a mastered stereo mix. Same input → byte-identical output, every time.

[![CI](https://img.shields.io/github/actions/workflow/status/ishan-parihar/scorestrata/ci.yml?style=flat-square)](https://github.com/ishan-parihar/scorestrata/actions)
[![Rust](https://img.shields.io/badge/rust-1.85+-blue?style=flat-square)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/tests-847%20passing-brightgreen?style=flat-square)](#)
[![License](https://img.shields.io/badge/license-MIT-purple?style=flat-square)](LICENSE)

---

## What Scorestrata Is

Scorestrata is a **deterministic, offline music-composition and rendering harness**. There is no sampler and no ""magic"" — every note, velocity, FX send, and automation curve is an explicit, auditable event. That makes it a *compiler*: brief in, iterated timeline IR, mastered WAV out.

**Pipeline:**

```
brief ──▶ config_gen ──▶ planner ──▶ compiler ──▶ timeline IR ──▶ audio engine ──▶ WAV
              │             │            │                │               │
            genre       segments/    deterministic    synth voices +  mastered
          detection     patterns/    events          FX + automation   stereo
          + defaults    layers
```

**Core properties:**

- **Deterministic** — identical input yields byte-identical WAV output; no RNG wiggle between runs
- **Contract-first** — the domain model (`Project`, `Segment`, `Layer`, mixer) is a typed contract; audio renders against it
- **Event-sourced** — the timeline is a deterministic event stream, inspectable and replayable at every stage
- **Agent-native** — the full pipeline is exposed as MCP tools, so any agent can compose and audit tracks in-conversation
- **Auditable** — per-layer spectral reports and spectrograms (`ss audit`, `audit_layer_spectral` MCP tool) verify the mix, not just claims

## Quick Start

```bash
cargo build --release -p scorestrata-cli
./target/release/ss --help
```

Describe and render a track:

```bash
ss render "an 80s pop track, 116 BPM, house kit, octave-bounce bass, off-beat guitar stabs"
```

The CLI is one of **four interfaces**:

| Interface | Purpose |
|-----------|---------|
| `ss` CLI | operator-driven composition and auditing |
| **MCP server** | agent-driven composition in-conversation |
| **agent crate** | programmatic composition API |
| **eval suite** | automated quality scoring against gold-standard demos |

## Features

- **Genre engine** — detection keywords, defaults, genre fingerprints, arrangement templates, and production playbooks (pop, funk, jazz, hip-hop, dnb, cinematic, synthwave, lo-fi, blues, ...)
- **Layered synthesis** — detuned dual-osc, sub, noise, drive, filter env, vibrato, 2× oversampled drive, per-instrument timbres
- **Per-layer mixing & automation** — FX chains, volume automation (CC7), mix chains per production family
- **Mastering** — final stereo master with loudness targets (LUFS, crest)
- **Audit tooling** — layer-wise spectral reports + per-layer spectrogram PNGs on the timeline
- **Gold-standard demos** — reference tracks and eval kit shipped under `demos/` / `eval/`

## Workspace Layout

```
scorestrata/
├── crates/
│   ├── scorestrata-core       # domain model (Project, Segment, Layer, mixer)
│   ├── scorestrata-theory     # music theory — harmony, rhythm, structure
│   ├── scorestrata-compiler   # brief → timeline IR
│   ├── scorestrata-validator  # contract enforcement
│   ├── scorestrata-audio      # synth voices, FX, automation, rendering
│   ├── scorestrata-storage    # event-sourced persistence
│   ├── scorestrata-agent      # programmatic composition API
│   ├── scorestrata-mcp        # MCP tool surface
│   ├── scorestrata-cli        # `ss` CLI
│   ├── scorestrata-eval       # automated quality scoring
│   └── scorestrata-testkit    # test fixtures and helpers
├── docs/
│   ├── architecture/          # architecture.md
│   ├── planning/              # re-architecture, audio-quality, UX plans
│   └── system-audit-report.md # live system-status audit
├── demos/  demo/              # gold-standard rendered tracks
├── examples/  eval/  tests/   # examples and quality evaluation
└── scripts/                   # demo generators (e.g. gen_nu_disco.py)
```

## Testing & Quality Gate

- **847 tests passing**, `clippy -D warnings` clean, `cargo fmt` checked — enforced in CI on every push (`ci.yml`)
- Build-time validation is the F′/F3 rule: a local-only green is not the only gate
- Spectral audit produces evidence, not assertions — render, inspect the spectrogram, iterate

```bash
cargo test --workspace
cargo clippy --workspace --all-targets                                    # zero warnings
cargo fmt --all -- --check
```

## License

MIT. See `LICENSE`.