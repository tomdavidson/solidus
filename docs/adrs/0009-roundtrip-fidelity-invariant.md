---
number: 9
title: Roundtrip Fidelity Invariant
date: 2026-03-29
status: accepted
---

# 9. Roundtrip Fidelity Invariant

Date: 2026-03-29

## Status

Proposed

## Context

The parser partitions input text into commands and text blocks. The question is whether the output must preserve enough information to reconstruct the original input, and if so, to what degree of fidelity. Alternatives considered: no roundtrip guarantee (output is lossy, consumers get only parsed fields), byte-for-byte reconstruction (output must reproduce the exact original bytes), or structural equivalence on re-parse where P(F(P(I))) = P(I) with P as the parser and F as a formatter.

## Decision

Adopt the structural roundtrip fidelity invariant as specified in the syntax RFC Section 8.3: for any valid input I, parsing, formatting, and parsing again yields a structurally equivalent result. P(F(P(I))) = P(I). Structural equivalence means the same sequence of commands and text blocks with identical values for name, argument mode, header, payload, fence language, and text block content. Raw source values, line numbers, and whitespace details MAY differ.

## Consequences

The primary motivation is testability to ensure correctness. Property-based tests can generate arbitrary input, parse it, format it, parse again, and assert structural equality, catching classes of bugs that example-based tests miss. Fuzz tests can use the roundtrip invariant as an oracle: any input where the invariant fails is a bug. The invariant constrains every output field; if the parser drops or mangles information, the roundtrip breaks. The invariant is structural, not byte-for-byte, because normalization (CRLF to LF) and line joining (backslash removal) intentionally transform the input. The raw field on commands must contain the exact pre-join physical lines so a formatter can reconstruct source text. Text block content must preserve original physical lines. Any future output field must be evaluated against the roundtrip invariant before being added. A formatter utility is implied by this invariant, specified as future work in the SDK spec.