---
number: 10
title: Multi-Engine Fuzz Testing with Datatest Regression Prevention
date: 2026-03-29
status: accepted
---

# 10. Multi-Engine Fuzz Testing with Datatest Regression Prevention

Date: 2026-03-29

## Status

Proposed

## Context

The engine's total function guarantee (ADR 0007) means any panic is a bug. The roundtrip fidelity invariant (ADR 0009) provides a mechanical correctness oracle beyond crash detection. Fuzz testing is the primary tool for discovering panics on adversarial or unexpected input. Key design questions: which fuzzing engines to use, how to handle structured-vs-unstructured input, how to prevent regressions without manual test wiring, and how to manage corpus and crash artifacts across local development and CI. Alternatives considered: single-engine fuzzing (libFuzzer only), manual crash-to-test conversion, build.rs code generation for regression tests (used in earlier versions, replaced by datatest-stable), corpus committed to the repository, and ad-hoc local-only fuzzing without CI integration.

## Decision

Adopt a three-phase fuzz pipeline (Discovery, Triage, Regression Prevention) using a 2x2 engine/target matrix. Discovery runs both libFuzzer and LibAFL against two targets: parse_document_unstructured (raw bytes fed to parse_document) and parse_document_structured (arbitrary::Arbitrary-generated FuzzDoc rendered to text via engine-fuzz-common). Triage minimizes crashes via cargo fuzz tmin and compacts corpus via cargo fuzz cmin. Regression prevention uses datatest-stable with harness=false integration tests that auto-discover minimized crash files from fuzz/regressions/<target>/ at test time, with no code generation step. Shared assertion helpers and structured types live in the engine-fuzz-common crate. Moon task orchestration manages parallelization of the full matrix. Corpus is gitignored and persisted via GitHub Actions caching. Minimized regression files are committed to fuzz/regressions/.

## Consequences

Different fuzzing engines use different mutation heuristics, so running both libFuzzer and LibAFL increases coverage diversity. Structured generation via engine-fuzz-common types (FuzzDoc, Fragment, CmdName, FenceBody) produces syntactically plausible inputs that exercise deeper parser paths, while unstructured raw bytes stress error handling and boundary conditions. The datatest-stable approach eliminates all code generation: dropping a minimized crash file into fuzz/regressions/<target>/ with a regression- prefix is sufficient to create a new test case. No build.rs, no manual #[test] functions. The replay functions in tests/ re-run the same assertions used by the fuzz harnesses (sequential IDs, valid argument modes, determinism, unclosed fence warnings). The engine-fuzz-common crate is shared between fuzz harnesses and datatest regression tests, ensuring assertion parity. Corpus caching in CI avoids repository bloat while preserving fuzzing progress. Two CI workflows automate the pipeline: weekly trunk saturation with automatic PR creation for new regressions, and on-demand PR saturation triggered by a fuzz label. The fuzz/regressions/ directory is committed to version control. The fuzz/corpus/ and fuzz/artifacts/ directories are gitignored.