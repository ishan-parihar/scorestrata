# Contributing to Scorestrata

Thanks for wanting to contribute to Scorestrata — a deterministic music-generation
compiler (9 crates, 944+ tests).

## Ground rules

- **Determinism is sacred.** The compiler must produce byte-identical WAV output for a
  given project + seed. Any change that makes rendering non-reproducible is a regression.
- **No regressions.** `cargo test --workspace` must pass with **zero** failures before any
  PR. `cargo clippy --workspace --all-targets -- -D warnings` must be clean.
- **Plans first.** Major features should be planned in `docs/planning/` and referenced from
  the PR description.

## Development loop

```bash
# Build + test
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --check

# Render a demo and verify it is deterministic
cargo run -p scorestrata-cli -- render demos/<demo> --out /tmp/out.wav
cargo run -p scorestrata-cli -- render demos/<demo> --out /tmp/out2.wav
cmp /tmp/out.wav /tmp/out2.wav   # must be identical
```

## What to check before submitting

1. Tests pass across the whole workspace.
2. Clippy is clean with `-D warnings`.
3. No `.cortexkit/`, `.contexty/`, or other tool-state artifacts are tracked.
4. If you touched audio rendering: re-render a demo and confirm byte-identical output.
5. If you touched MCP tools: update `docs/` references and any tool-count claims in READMEs.

## Structure

| Crate | Purpose |
|---|---|
| `scorestrata-core` | Domain model, events, policy engine, state machine |
| `scorestrata-theory` | Music theory: intervals, keys, chords, scales |
| `scorestrata-compiler` | Timeline, arrangement, MIDI export, rendering plan |
| `scorestrata-validator` | All validation checks (scale, segment, audio, register, …) |
| `scorestrata-audio` | Synthesis, SF2 banks, mixer, mastering, analysis |
| `scorestrata-storage` | Persistence: bundles, ledger, evidence, knowledge |
| `scorestrata-agent` | Agent kernel, task board, lessons |
| `scorestrata-mcp` | MCP server exposing tools + resources |
| `scorestrata-cli` | `ss` command line interface |

## Questions

Open an issue, or start a discussion — keep the conversation in the repo so decisions
are recorded.
