# Solidus Engine Fuzzing System

This directory contains an automated fuzz testing pipeline for the Solidus Engine. It uses both
libFuzzer and LibAFL, driven by Moonrepo task orchestration, with continuous regression testing
via `datatest-stable` and GitHub Actions CI.


## Architecture

The pipeline has three phases that form a feedback loop: Discovery, Triage, and Regression
Prevention.

```text
┌────────────────────────────────────────────────────────────────────────┐
│ 1. Discovery (moon run engine-fuzz:fuzz-saturate)                    │
│                                                                      │
│   [ libFuzzer ] ─────┬───► Raw unstructured bytes ───────┐           │
│                      │                                   │           │
│   [ LibAFL ] ────────┘                                   ▼           │
│                                                   [ parse_document ]│
│   [ libFuzzer ] ─────┬───► arbitrary::Arbitrary ────────▲           │
│                      │     (Structured generator)        │           │
│   [ LibAFL ] ────────┘                                   │           │
└──────────────────────┤───────────────────────────────────┤─────────────┘
                       │ (Crashes)                         │ (Valid inputs)
                       ▼                                   ▼
              fuzz/artifacts/<target>/             fuzz/corpus/<target>/
                       │                                   │
┌──────────────────────┤───────────────────────────────────┤─────────────┐
│ 2. Triage & Compact  │                                   │             │
│                      ▼                                   ▼             │
│           moon run :fuzz-triage               moon run :compact     │
│            (cargo fuzz tmin)                   (cargo fuzz cmin)     │
│                      │                                   │             │
│                      ▼                                   ▼             │
│          fuzz/regressions/<target>/              fuzz/corpus/<target>/│
└──────────────────────┤─────────────────────────────────────────────────┘
                       │ (Minimized crashes)
                       ▼
┌────────────────────────────────────────────────────────────────────────┐
│ 3. Regression Prevention (cargo test)                                │
│                                                                      │
│   [ datatest-stable ] ──► Auto-discovers fuzz/regressions/ at test time│
│           │                                                            │
│           ▼                                                            │
│   [ engine/tests/fuzz_regression_*.rs ] (harness = false)             │
│           │                                                            │
│           ▼                                                            │
│   Replays each regression file through parse_document with the        │
│   same assertions used by the fuzz harnesses (via engine-fuzz-common) │
└────────────────────────────────────────────────────────────────────────┘
```


## How It Works

### Multi-Engine Fuzzing (`fuzz-run.sh`)

Different fuzzing engines use different mutation heuristics, so we run a 2x2 matrix:

- Targets: `parse_document_unstructured` (raw bytes) and `parse_document_structured`
  (`arbitrary::Arbitrary` AST-aware generation via `engine-fuzz-common`).
- Engines: Standard LLVM libFuzzer and LibAFL, selected via the `FUZZ_FEATURES` env var
  (e.g. `--no-default-features --features libafl`).

`fuzz-run.sh` accepts a target name, max time, job count, and an optional `--replay` flag. It
handles CWD resolution, pre/post crash counting, and sweeps stray `fuzz-*.log` files into
`artifacts/<target>/logs/`.

LibAFL does not support `-runs=0` corpus replay. Its value is in long-running saturation, not
quick CI checks. CI replay tasks use libFuzzer only.


### Triage and Compaction (`fuzz-manage.sh`)

Two subcommands manage the data fuzzers generate:

- `triage` (`cargo fuzz tmin`): Minimizes crash files from `artifacts/<target>/` to the smallest
  byte sequence that still triggers the panic. Saves results into `regressions/<target>/` with a
  hash-based filename (`regression-<hash>`) to avoid duplicates.
- `compact` (`cargo fuzz cmin`): Removes redundant corpus entries that don't increase coverage,
  keeping only the most efficient inputs.


### Automated Regression Tests (`datatest-stable`)

Once a crash is minimized into `regressions/`, it becomes a permanent test with no manual wiring.

- `engine/tests/fuzz_regression_parse_document_unstructured.rs` and
  `engine/tests/fuzz_regression_parse_document_structured.rs` use `datatest_stable::harness!`
  with `harness = false` in `Cargo.toml`.
- At test time, `datatest-stable` auto-discovers all files matching `^regression-` under
  `fuzz/regressions/<target>/`.
- Each file is replayed through `parse_document` with the same assertion suite used by the
  fuzz harnesses: sequential IDs, valid argument modes, determinism, and unclosed fence warnings.
- Shared assertions live in `engine-fuzz-common` (`assertions.rs`), ensuring parity between
  fuzz harnesses and regression tests.
- Adding a regression is zero-ceremony: drop the file, run `cargo test`.


### Structured Fuzzing (`engine-fuzz-common`)

The `engine-fuzz-common` crate provides:

- `types.rs`: `FuzzDoc`, `Fragment`, `CmdName`, `FenceBody`, and other AST-level types with
  `Arbitrary` implementations that generate syntactically plausible parser inputs.
- `render.rs`: Renders a `FuzzDoc` into a UTF-8 string that exercises realistic parser paths
  (valid commands, fenced blocks, joined lines, invalid slash patterns, blank lines).
- `assertions.rs`: Shared assertion helpers (`assert_ids_sequential`, `assert_argument_modes`,
  `assert_unclosed_fence_warning`) used by both fuzz harnesses and datatest regression tests.


## Moon Task Graph

Moon manages the parallelization of the full 2x2 engine/target matrix. Moon project metadata
(`corpus-path`, `regressions-path`) is used by CI workflows to resolve artifact paths.

```text
fuzz-ci (sequential: saturate then gate)
├── fuzz-saturate (parallel)
│   ├── fuzz-structured-saturate
│   │   ├── libfuzz-structured-saturate    (fuzz-run.sh ... 14400 2)
│   │   └── libafl-structured-saturate     (fuzz-run.sh ... 14400 2 + FUZZ_FEATURES)
│   │   └── compact (fuzz-manage.sh compact)
│   └── fuzz-unstructured-saturate
│       ├── libfuzz-unstructured-saturate
│       └── libafl-unstructured-saturate
│       └── compact
└── fuzz-gate (checks artifacts/*/crash-summary.txt)

fuzz-triage (sequential, after gate signals crashes)
├── triage parse_document_unstructured
└── triage parse_document_structured

fuzz (quick replay, libFuzzer only, -runs=0)
├── libfuzz-structured     (fuzz-run.sh ... --replay)
└── libfuzz-unstructured   (fuzz-run.sh ... --replay)
```

### Commands

```bash
# Quick replay: re-run existing corpus with -runs=0 (seconds, libFuzzer only)
moon run engine-fuzz:fuzz

# CI pipeline: saturate all four variants then gate on crashes
moon run engine-fuzz:fuzz-ci

# Full saturation: parallel fuzzing + compaction (local or CI)
moon run engine-fuzz:fuzz-saturate

# Triage only: minimize new crash artifacts into regressions/
moon run engine-fuzz:fuzz-triage
```


## CI Automation

Two GitHub Actions workflows automate fuzzing in CI.

### Trunk Saturation (`main-fuzz.yml`)

Runs weekly (Saturday 2 AM UTC) and on manual dispatch. Discovers all projects with a `fuzz-ci`
task via `moon-q-projects`, then runs `fuzz-saturate` across them in a matrix. If crashes are found,
it triages them automatically and opens a PR with the minimized regression files via the `pr-create`
composite action.

Defaults: `FUZZ_MAXTIME=14400` (4h per engine/target), `FUZZ_JOBS=4`.
Retention: regression artifacts kept 90 days.

### PR Saturation (`pr-fuzz.yml`)

Triggered by adding the `fuzz` label to a PR, or via `workflow_dispatch` with a PR number. Runs the
same discover/saturate/triage cycle against the PR branch. Regression files are committed directly
to the PR branch (attributed to the last branch author). Results are reported as a commit status and
sticky PR comment via the `pr-set-status` composite action.

Defaults: `FUZZ_MAXTIME=18000` (5h per engine/target), `FUZZ_JOBS=4`.
Retention: regression artifacts kept 30 days.

Both workflows use these shared composite actions:

- `setup`: Checkout, toolchain installation, Cargo/Moon/pnpm caches
- `moon-q-projects`: Discover fuzz-capable projects via moon metadata
- `moon-run`: Execute moon tasks with retrospect reporting
- `fuzz-cache`: Per-project corpus caching with main/PR isolation
- `git-stage-artifacts`: Download and place matrix artifacts into project directories
- `git-commit-push`: Commit and push with last-author attribution (PR workflow)
- `pr-create`: Idempotent PR creation (trunk workflow)
- `pr-set-status`: Commit status + sticky PR comment (PR workflow)
- `resolve-pr`: Normalize PR metadata across event types


## State Management

- `fuzz/corpus/<target>/`: Gitignored. Persisted exclusively via GitHub Actions caching
  (`fuzz-cache` action) to avoid repository bloat. Main and PR caches are isolated.
- `fuzz/artifacts/<target>/`: Gitignored. Temporary storage for raw crashes, minimized files,
  crash summaries, and log output.
- `fuzz/regressions/<target>/`: Committed. Tiny, minimized crash files that serve as source of
  truth for `cargo test` via `datatest-stable`.
