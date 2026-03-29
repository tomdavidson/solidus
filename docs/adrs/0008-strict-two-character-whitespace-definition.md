---
number: 8
title: Strict Two-Character Whitespace Definition
date: 2026-03-29
status: accepted
---

# 8. Strict Two-Character Whitespace Definition

Date: 2026-03-29

## Status

Proposed

## Context

The parser needs to identify whitespace in several places: separating a command name from its arguments, trimming fence closers, and trimming headers. Rust provides two built-in options: char::is_whitespace() which matches 25 Unicode whitespace characters including LF, CR, non-breaking space, and various Unicode spaces; and char::is_ascii_whitespace() which matches space, tab, LF, CR, and FF. Alternatives considered: use char::is_whitespace() (full Unicode whitespace), use char::is_ascii_whitespace() (ASCII whitespace), or define whitespace as exactly SP (U+0020) and HTAB (U+0009).

## Decision

Whitespace in the slash command syntax means exactly two characters: SP (U+0020, space) and HTAB (U+0009, horizontal tab), as specified in the syntax RFC and engine spec Section 6. The engine MUST NOT use Rust's char::is_whitespace() or char::is_ascii_whitespace() for whitespace checks.

## Consequences

Both of Rust's built-in whitespace functions include characters that would cause bugs in this parser. char::is_whitespace() includes LF and CR, which have already been consumed by line-ending normalization and line splitting. If the parser treated LF as whitespace during classification, it would incorrectly consume characters that are line boundaries, not separators. It also includes 20+ Unicode whitespace characters that should be treated as literal content in command arguments, not as separators. char::is_ascii_whitespace() includes LF, CR, and FF, all with the same problem. The engine defines a dedicated is_wsp helper and uses it consistently across all modules. Non-breaking spaces (U+00A0) and other Unicode whitespace characters are treated as literal content, not separators. If a user types a non-breaking space between a command name and its arguments, the line will be classified as text, not a command. This is intentional.