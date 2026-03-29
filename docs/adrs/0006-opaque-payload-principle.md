---
number: 6
title: Opaque Payload Principle
date: 2026-03-29
status: accepted
---

# 6. Opaque Payload Principle

Date: 2026-03-29

## Status

Proposed

## Context

The parser produces a payload string for each command. The question is whether the parser should interpret that payload in any way: tokenize it, apply key-value parsing, handle quoting or escaping, validate JSON, etc. Alternatives considered: parse arguments into key-value pairs or positional tokens, support shell-style quoting and escaping within arguments, or treat all argument content as an opaque string.

## Decision

The parser treats all argument content (header and payload) as opaque, as specified in the syntax RFC Section 8.2 item 3. A conforming parser MUST NOT interpret, tokenize, or apply quoting, escaping, or key-value semantics to the header or payload.

## Consequences

Two main reasons drove this decision. First, avoid poisoning: if the parser interprets content, it imposes assumptions about argument structure that may not match the consumer's needs. A command like /mcp calltool writefile with a JSON payload should not have its JSON tokenized by the parser. Second, it is out of scope: the parser's job is to partition text into commands and text blocks, and argument interpretation is the consumer's responsibility. The parser provides no sanitization of argument content, so consumers MUST validate arguments before acting on them. The engine has no dependency on any serialization or parsing library for argument content. The security boundary is clear: the parser detects commands, consumers validate them.