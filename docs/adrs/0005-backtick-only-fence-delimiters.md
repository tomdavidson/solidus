---
number: 5
title: Backtick-Only Fence Delimiters
date: 2026-03-29
status: accepted
---

# 5. Backtick-Only Fence Delimiters

Date: 2026-03-29

## Status

Proposed

## Context

Markdown supports two fence delimiter characters: backtick and tilde. Both serve the same purpose in Markdown. The question is whether the slash command syntax should recognize both. Alternatives considered: support both backtick and tilde fences (full Markdown compatibility), support backtick fences only, or define a custom delimiter.

## Decision

Only backtick fences are recognized. Tilde fences MUST NOT be treated as fence openers, as specified in the syntax RFC Section 5.2.

## Consequences

Supporting only backticks keeps the state machine simple and avoids a more complex AST. The parser tracks one delimiter type with one counter. Adding tilde support would mean two opener patterns to detect during line classification, tracking which delimiter type opened the current fence to match the correct closer, and edge cases around mixed delimiters. Tilde fences are infrequent in practice. In the slash command context (chat, agent interfaces, developer tooling), backtick fences are the dominant convention. The complexity cost of supporting tildes is not justified by the usage. Lines like ~~~ inside a fenced block are treated as literal payload content, not closers. If tilde support is ever needed, it would require a syntax RFC revision and a new ADR superseding this one.