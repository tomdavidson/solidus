---
number: 2
title: Adopt Slash Command Syntax RFC as Normative Specification
date: 2026-03-29
status: accepted
---

# 2. Adopt Slash Command Syntax RFC as Normative Specification

Date: 2026-03-29

## Status

Proposed

## Context

Solidus needs a single authoritative specification that defines observable parser behavior for slash commands. Without a normative contract, the engine, SDK, WASM, WASI, and every language binding would each drift toward their own interpretation of how commands are detected, how arguments are delimited, and what output is required.

## Decision

Adopt the Slash Command Syntax RFC (currently v1.1.x, authored by Tom D. Davidson) as the sole normative specification for all parser behavior in this project. The engine spec, SDK spec, and all downstream targets MUST conform to the syntax RFC. Conformance means producing identical output for identical input as defined by the RFC Sections 6, 7, and 8.

## Consequences

The syntax RFC is the source of truth for what the parser does. The engine spec and SDK spec describe how, not what. Any change to observable parsing behavior requires a syntax RFC revision first, then corresponding updates to the engine and SDK specs. The version field in serialized output tracks the syntax RFC version, not the engine or SDK version. ADRs in this repository capture project-level decisions (why we chose a particular approach) and do not duplicate or override the RFC normative rules. If this project ever needs to diverge from the RFC, that divergence requires a new ADR and a corresponding RFC amendment.