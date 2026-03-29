---
number: 3
title: Lowercase-Only Command Names
date: 2026-03-29
status: accepted
---

# 3. Lowercase-Only Command Names

Date: 2026-03-29

## Status

Proposed

## Context

The syntax RFC restricts command names to the pattern [a-z][a-z0-9-]* where multi-character names must not end with a hyphen. Only lowercase ASCII letters, digits, and hyphens are valid. Uppercase letters, underscores, and Unicode identifiers are excluded. Alternatives considered: case-insensitive matching (accept /Deploy and /deploy as equivalent), mixed case with exact matching (/callTool distinct from /calltool), and Unicode identifiers to support non-Latin command names.

## Decision

Command names are lowercase ASCII only, as specified in the syntax RFC Section 4.1.

## Consequences

Commands like /callTool or /Deploy are classified as text, not commands. Consumers who want case-insensitive behavior must lowercase their input before parsing. The command name grammar is simple enough to validate with a tight character loop and no external dependencies. This was primarily a convention-driven choice. Most existing slash command implementations (Slack, Discord, CLI tools) use lowercase. It eliminates case-sensitivity ambiguity entirely, keeps the grammar to a single-pass character check with no locale-dependent logic, and avoids pulling in Unicode normalization (NFC/NFD, case folding). There is no single compelling technical reason that rules out mixed case. The choice was made for simplicity, convention alignment, and to avoid the complexity cost of case handling for a feature with no demonstrated need.