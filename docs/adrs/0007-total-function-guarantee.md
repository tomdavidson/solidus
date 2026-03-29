---
number: 7
title: Total Function Guarantee
date: 2026-03-29
status: accepted
---

# 7. Total Function Guarantee

Date: 2026-03-29

## Status

Proposed

## Context

Most parsers can fail: they return errors for malformed input, throw exceptions, or panic. The question is whether the slash command parser should follow this pattern or guarantee a valid result for every input. Alternatives considered: return Result<ParseResult, ParseError> with explicit failure cases, panic on malformed input, or always return a valid ParseResult using warnings for non-fatal issues.

## Decision

parse_document is a total function. It MUST return a valid ParseResult for any input, including empty input, malformed input, and adversarial input. It MUST NOT panic, return an error type, or use any fallible return mechanism. This is specified in the syntax RFC Section 8.2 and engine spec Section 4.2. Malformed constructs (like unclosed fences) produce partial results with corresponding entries in the warnings vector.

## Consequences

The goal is to defer UX and error handling to consumers of the engine. The parser's job is to partition text. There is no input that is invalid from a partitioning perspective: every byte sequence can be partitioned into commands and text blocks, even if some constructs are incomplete. Consumers never need to handle parser errors; they always get a result they can work with. Error presentation, retry logic, and user-facing messages are the consumer's responsibility, where they have context about the user and the environment. The API surface is minimal: one function, one return type, no error variants to match on. Testing is simpler: every input produces output, so property-based tests and fuzz tests can run without needing to distinguish valid failures from bugs. The ParseResult type is not wrapped in Result. Warnings are informational, not errors. Any crash is definitively a bug.