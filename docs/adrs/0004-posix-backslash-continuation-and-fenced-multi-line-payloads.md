---
number: 4
title: POSIX Backslash Continuation and Fenced Multi-Line Payloads
date: 2026-03-29
status: accepted
---

# 4. POSIX Backslash Continuation and Fenced Multi-Line Payloads

Date: 2026-03-29

## Status

Proposed

## Context

Slash commands in chat and agent interfaces often need to carry multi-line or complex arguments. A single-line-only model forces users to encode structure into one long line or use out-of-band mechanisms. Two separate but related features address this: backslash continuation (joining physical lines into a single logical line) and fenced blocks (attaching a verbatim multi-line payload using backtick delimiters). For backslash continuation, alternatives considered: no continuation at all, a custom join character or syntax, and POSIX shell-style backslash-newline removal. For multi-line payloads, alternatives considered: heredoc-style delimiters, indentation-based blocks (like Python or YAML), and backtick-fenced blocks (like Markdown code fences).

## Decision

Adopt POSIX shell backslash-newline removal for line joining, and backtick-fenced blocks for multi-line payloads, as specified in the syntax RFC Sections 3.2 and 5.2. Line joining uses true POSIX semantics: the trailing backslash and line boundary are removed and the remainder is concatenated directly with the next physical line. No separator character is inserted.

## Consequences

POSIX backslash continuation gives us familiarity and existing UX to model. Developers already know this behavior from shell scripts, Makefiles, and similar tools. Fenced blocks open the door to powerful inputs that are also easy to read. A user can attach JSON, YAML, code, or any structured content to a command with clear visual boundaries. The backtick fence syntax is already familiar from Markdown. Simple commands stay on one line, moderately long commands can wrap with backslash continuation, and complex payloads get their own clearly delimited block. The parser must track whether a fence is open before deciding whether to apply line joining (fence body lines are immune to joining). Earlier engine versions (v0.3.0) inserted a space between joined lines; this was changed to true POSIX direct concatenation in v0.4.0 to match well-known behavior exactly.