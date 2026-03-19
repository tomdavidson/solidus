# Slash Command Parser Specification

Version: 0.3.0 Date: 2026-03-18

## 1. Overview

The Slash Command Parser consumes a single UTF-8 text input and produces a structured JSON result
containing all detected slash commands, interleaved text blocks, optional caller-supplied context,
and any parser warnings.

The parser is deterministic and pure: given the same input and context, it always produces the same
output. It performs no I/O, maintains no global mutable state, and accepts all configuration through
its input parameters.

### 1.1 Design Principles

1. **Command-agnostic.** The parser never interprets argument content. It determines argument
   boundaries and transport mode but treats all argument strings as opaque byte sequences.
   Tokenization, quoting, key-value parsing, and any other semantic interpretation of arguments are
   the sole responsibility of the command implementation.

2. **Flat output, no AST.** The parser emits a flat list of commands and text blocks in document
   order. There is no intermediate abstract syntax tree.

3. **Incremental emission.** A conforming implementation must be capable of finalizing and emitting
   each command or text block as soon as its last line has been consumed, without buffering the
   entire document. Memory usage should be bounded by the size of the largest single command payload
   or text block, not the total input size.

4. **Single forward pass.** The parser processes the input in a single left-to-right pass (after
   normalization and line joining). It never backtracks.

## 2. Input Model

### 2.1 Line Ending Normalization

Before any other processing:

1. Replace all `\r\n` (CRLF) sequences with `\n` (LF).
2. Replace all remaining `\r` (bare CR) characters with `\n`.

After normalization, all line terminators are LF. The parser splits the normalized input on `\n` to
produce a sequence of physical lines. Each `\n` terminates a line and is not part of the line
content. A trailing `\n` at the end of input produces a trailing empty line.

Literal escape sequences inside content (e.g., `\n` in a JSON string `"blah\nblah"`) are ordinary
characters, not line terminators. They are preserved verbatim in the output.

### 2.2 Line Joining (Backslash Continuation)

After line-ending normalization and before command parsing, the parser performs a line-joining
pre-pass. This mechanism is identical in behavior to POSIX shell backslash-newline removal.

For each physical line, if the line ends with a backslash (`\`) character:

1. Remove the trailing backslash.
2. Remove the line terminator (the split boundary).
3. Concatenate the remainder with the next physical line, separated by a single space.
4. Repeat: if the joined result still ends with `\`, continue joining with subsequent lines.

Lines that do not end with `\` are left unchanged.

The join marker is any backslash character immediately before the physical line terminator,
regardless of what precedes it. There is no requirement for a space or any other character before
the backslash. This includes lines that serve other syntactic roles, such as a closing fence line
followed by a trailing backslash (e.g., `` ``` \ ``). In all cases, a trailing `\` triggers line
joining. This matches POSIX shell behavior.

After this pass, the parser operates on a sequence of logical lines. Each logical line maps back to
one or more physical lines from the original input.

#### 2.2.1 Physical Line Tracking

The parser must track the mapping from each logical line back to its original physical line range.
This mapping is used to populate `range.start_line` and `range.end_line` in the output, which always
refer to zero-based physical line numbers from the normalized input (before joining).

#### 2.2.2 Trailing Backslash at EOF

If the final physical line ends with a backslash and there is no subsequent line to join with, the
trailing backslash is removed and the line stands alone. This mirrors POSIX shell behavior where a
backslash-newline pair removes the newline; at EOF, the backslash simply disappears.

Example:

```text
/echo hello \
```

Logical line: `/echo hello`

The command parses as a single-line command with `arguments.payload` of `"hello "`.

#### 2.2.3 Joining Examples

Physical input (three physical lines):

```text
/mcp call_tool read_file \
  --path src/index.ts \
  --format json
```

Logical line after joining:

```text
/mcp call_tool read_file   --path src/index.ts   --format json
```

This logical line maps to physical lines 0 through 2.

A line ending with a literal backslash that is not intended as a join marker should use a fenced
block instead (see Section 5.2).

### 2.3 Fence Immunity

Line joining does not apply inside fenced blocks. Once the parser enters fence mode (see Section
5.2), all physical lines are consumed verbatim until the closing fence or EOF. A trailing `\` inside
a fence is literal content, not a join marker.

Because line joining is conceptually a pre-pass and fences are detected during command parsing, the
implementation must either (a) perform joining lazily during parsing, skipping lines inside fences,
or (b) use a two-pass approach where fence boundaries are identified first. Either strategy is
acceptable as long as fence content is never subject to line joining.

## 3. Command Detection

A command line is any logical line (after normalization and joining) whose first non-whitespace
character is `/` (U+002F) and whose subsequent characters form a valid command name.

### 3.1 Command Name

The command name:

- Starts immediately after the leading `/` with no intervening space.
- Ends at the first whitespace character or end-of-line.
- Must match the pattern `[a-z][a-z0-9-]*`:
  - Begins with a lowercase ASCII letter (a-z).
  - Followed by zero or more lowercase ASCII letters, ASCII digits, or hyphens.

### 3.2 Invalid Slash Lines

If a logical line's first non-whitespace character is `/` but the text after `/` does not match the
command name pattern (for example, a bare `/`, `/123`, `/Hello`, or `/ space`), the line is not a
command. In `idle` state, such lines are treated as ordinary text and may become part of a text
block (Section 6). In `in_fence` state, all lines (including invalid slash lines) are literal
payload content.

### 3.3 Arguments

Everything after the first whitespace following the command name is the arguments portion. The
whitespace between the command name and the arguments is consumed as a separator and is not included
in the arguments string.

The arguments portion may be:

- Empty (command with no arguments).
- Inline text (single-line mode, Section 5.1).
- A fence opener (fence mode, Section 5.2).
- Inline text followed by a fence opener (the text before the fence becomes the `header`, the fenced
  content becomes the `payload`).

### 3.4 The `header` Field

The `header` field contains the inline, non-fenced argument portion of the command line. It serves
as the dispatch or routing segment of the command and is present in both argument modes:

- In single-line mode, `header` and `payload` contain the same string (the full arguments text).
- In fence mode, `header` contains the arguments text that appears before the fence opener on the
  command line. This is typically used for subcommand names and flags, while the fenced `payload`
  carries bulk data such as JSON or code.

## 4. Parser States

The parser uses two states:

- `idle`: not inside a fenced block. The parser scans logical lines for commands and collects
  non-command lines into text blocks.
- `in_fence`: collecting raw physical lines inside a fenced block for the current command.

There is no "accumulating" or "continuation" state. Multi-physical-line commands that are not fenced
are handled entirely by line joining (Section 2.2) before the state machine runs.

### 4.1 State Transitions

| Current State | Condition                                            | Action                                        | Next State |
| ------------- | ---------------------------------------------------- | --------------------------------------------- | ---------- |
| `idle`        | Logical line is a valid command with no fence opener | Finalize as single-line command               | `idle`     |
| `idle`        | Logical line is a valid command with a fence opener  | Begin fence; record header and fence metadata | `in_fence` |
| `idle`        | Logical line is not a command                        | Append to current text block                  | `idle`     |
| `in_fence`    | Physical line is a closing fence                     | Finalize fenced command                       | `idle`     |
| `in_fence`    | Physical line is not a closing fence                 | Append to payload                             | `in_fence` |
| `in_fence`    | EOF reached without closing fence                    | Finalize command with warning                 | `idle`     |

## 5. Argument Modes

### 5.1 Single-Line Mode

If a command's arguments portion (after joining) does not contain a fence opener:

- The full arguments text is stored in both `header` and `payload`.
- `mode` is `"single-line"`.
- `fence_lang` is `null`.
- The command is finalized on that logical line.

Example:

```text
/deploy production --region us-west-2
```

Result:

- `name`: `"deploy"`
- `arguments.header`: `"production --region us-west-2"`
- `arguments.mode`: `"single-line"`
- `arguments.fence_lang`: `null`
- `arguments.payload`: `"production --region us-west-2"`

### 5.2 Fence Mode

Fenced blocks allow attaching a raw, multi-line payload to a command using markdown-style code fence
syntax. Fenced payloads are completely verbatim: no parsing rules (including line joining) apply to
content lines, eliminating escaping concerns.

Only backtick (`` ` ``) fences are recognized. Tilde (`~`) fences are not supported.

#### 5.2.1 Fence Opener

In the arguments portion of a command line, the first occurrence of three or more consecutive
backtick characters is treated as a fence opener.

- Text before the backtick run (trimmed of trailing whitespace) becomes `arguments.header`.
- The backtick run length is recorded as the fence marker length.
- Text after the backtick run (trimmed of leading whitespace), if non-empty and consisting of a
  single token (no internal whitespace), is the optional language identifier stored in
  `arguments.fence_lang`.
- The parser transitions to `in_fence`.

The variable-length backtick fence (three or more) means content containing triple backticks can be
fenced with four or more backticks, avoiding collision.

#### 5.2.2 Fence Body

While in `in_fence`, the parser reads physical lines from the normalized input (not joined logical
lines). All lines are appended to the payload verbatim, preserving their original content including
any trailing backslashes.

Lines are joined in the payload with `\n` separators. The payload does not include a trailing `\n`
after the last content line.

#### 5.2.3 Fence Lifetime

A fenced argument block extends from the fence opener line through either:

- The first closing fence line, or
- End of input (EOF), in which case the fence is considered unclosed and a warning is emitted (see
  Section 5.2.5).

There is no other mechanism that terminates a fence. Inside a fence, command triggers, invalid slash
lines, blank lines, and all other content are literal payload.

#### 5.2.4 Fence Closer

A physical line is a closing fence if, after trimming leading and trailing whitespace:

- It consists solely of backtick characters.
- The number of backticks is greater than or equal to the opener's backtick count.

The closing fence line is not included in the payload. Once found, the parser finalizes the command
and returns to `idle`.

Note that line joining still applies to lines outside fences. If a closing fence line ends with a
trailing backslash (e.g., `` ``` \ ``), the fence closes normally, and then the backslash triggers a
line join between the closing fence and the next physical line. The joined result is outside the
fenced command and is parsed normally (see Appendix A.8 for an example).

#### 5.2.5 Unclosed Fence

If the input ends before a closing fence is found:

- The parser finalizes the command with whatever payload has been accumulated through EOF.
- A warning object with `"type": "unclosed_fence"` and `"start_line"` set to the fence opener's
  physical line number is added to the `warnings` array in the result envelope.

The parser emits the command with its partial payload. Consumers that require strictly well-formed
fenced payloads SHOULD treat `unclosed_fence` warnings as hard errors and reject the affected
command.

#### 5.2.6 Backslash Joining Around Fences

Backslash line joining applies to physical lines outside of fences. This means joining can merge
lines before the fence opener and lines after the fence closer, but never lines inside the fence
body (see Section 2.3).

Joining into a fence opener. When backslash joining merges a command line with a line containing a
fence opener, the fence is detected in the resulting logical line. This is the natural way to place
a fence opener on a separate physical line from the command name.

Example (four physical lines):

/mcp call_tool write_file \

```json
{ "path": "foo" }
```

After joining, physical lines 0 and 1 become one logical line:

/mcp call_tool write_file ```json

The backtick run is detected as a fence opener. arguments.header is "call_tool write_file",
arguments.fence_lang is "json", and the JSON body is collected as the fenced payload.

Joining after a fence closer. Once the closing fence is found and the parser returns to idle, any
subsequent physical lines are again subject to normal line joining. If lines following the closing
fence end with trailing backslashes, they join with each other as usual.

Example (six physical lines):

/mcp call_tool write_file -c\

```json
{ "path": "foo" }
```

\
production

    Lines 0 and 1 join into: /mcp call_tool write_file -c ```json

    Lines 2 and 3 are inside and closing the fence (no joining applies).

    Lines 4 and 5 join into: production (the backslash on line 4 is removed and line 5 is
    appended).

The fenced command has header of "call_tool write_file -c", fence_lang of "json", and payload of "{
\"path\": \"foo\" }". The joined production line is outside the command and becomes a text block.
See Appendix A.9 for the full output.

## 6. Text Blocks

Non-command logical lines encountered in `idle` state are collected into text blocks.

- Consecutive non-command lines form a single text block.
- Blank lines that are part of a text region are included in the text block content.
- Text block content preserves the original lines joined with `\n` separators.
- Text blocks use physical line numbers for their `range`.
- A new text block begins after a command is finalized, if non-command lines follow.

Text blocks exist so that:

- The roundtrip invariant (Section 9.4) can be satisfied: a formatter needs to know where
  non-command regions are to reconstruct the input.
- Tooling can distinguish "what will run" from "what is commentary" when rendering or analyzing
  documents that mix commands with prose.

Consumers that do not need text blocks may ignore the `text_blocks` array.

## 7. Multiple Commands and Ordering

The parser walks the input line by line:

1. In `idle`, when it encounters a command line, it starts a new command according to the argument
   mode rules (Section 5).
2. After a command is finalized, the parser returns to `idle` and continues scanning.
3. Non-command lines in `idle` are collected into text blocks (Section 6).

Commands are assigned sequential zero-based IDs in encounter order: `cmd-0`, `cmd-1`, `cmd-2`, and
so on. Text blocks are assigned IDs independently in the same manner: `text-0`, `text-1`, `text-2`,
and so on.

Commands and text blocks appear in the output arrays in document order.

## 8. Output Format

### 8.1 Envelope

A parser run produces a single JSON object (the "envelope") conforming to the `SlashParserResult`
schema defined in Section 8.5. The envelope contains:

- `version`: the spec version (`"0.3.0"`).
- `context`: caller-supplied metadata passed through to the output unchanged.
- `commands`: an ordered array of all detected commands.
- `text_blocks`: an ordered array of all non-command text regions.
- `warnings`: an array of parser warnings (e.g., unclosed fences). Empty if no warnings.

The parser is a total function: it always produces a valid envelope for any input. There is no input
that causes the parser to fail or return an error instead of an envelope. An empty input produces an
envelope with empty `commands`, `text_blocks`, and `warnings` arrays. Malformed constructs (such as
unclosed fences) produce commands with partial data and corresponding entries in the `warnings`
array, never a parse failure.

### 8.2 The `raw` Field

The `raw` field on each command contains the exact source text for that command as it appeared in
the normalized input (after line-ending normalization but before line joining). For single-line
commands that span multiple physical lines via backslash joining, `raw` contains all the physical
lines including the backslash characters and the `\n` separators between them. For fenced commands,
`raw` includes the command line, all fence body lines, and the closing fence line (if present).

### 8.3 JSONL Streaming Mode

When a parser implementation supports JSON Lines (JSONL) output, each line of the JSONL stream is
exactly one complete `SlashParserResult` envelope, encoded as a single JSON object on one line.

JSONL mode does not emit one command per line. Commands remain elements of the `commands` array
inside each envelope. In a pipeline processing multiple input files, each input produces one JSONL
line.

This design was chosen because:

- Each JSONL line is a self-contained parse result with commands, text blocks, warnings, and
  context, making each line independently meaningful.
- Command IDs (`cmd-0`, `cmd-1`) are scoped per envelope, so per-envelope lines maintain stable ID
  semantics.
- Consumers can treat each line as a complete unit of work corresponding to one input document.

Per-command JSONL (one command per line) is out of scope for this specification. Implementations may
offer it as a non-standard extension.

### 8.4 Envelope Example

```json
{
  "version": "0.3.0",
  "context": { "source": "deploy.md", "user": "tom" },
  "commands": [
    {
      "id": "cmd-0",
      "name": "deploy",
      "raw": "/deploy production \\\n  --region us-west-2 \\\n  --canary",
      "range": { "start_line": 0, "end_line": 2 },
      "arguments": {
        "header": "production   --region us-west-2   --canary",
        "mode": "single-line",
        "fence_lang": null,
        "payload": "production   --region us-west-2   --canary"
      }
    }
  ],
  "text_blocks": [
    { "id": "text-0", "range": { "start_line": 3, "end_line": 3 }, "content": "Deploy initiated." }
  ],
  "warnings": []
}
```

### 8.5 JSON Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://slash-parser.dev/schema/v0.3.0/parse-result.json",
  "title": "SlashParserResult",
  "description": "Output envelope for the Slash Command Parser v0.3.0.",
  "type": "object",
  "required": ["version", "context", "commands", "text_blocks", "warnings"],
  "additionalProperties": false,
  "properties": {
    "version": {
      "type": "string",
      "const": "0.3.0",
      "description": "Specification version that produced this result."
    },
    "context": {
      "type": "object",
      "description": "Caller-supplied metadata passed through to the output unchanged.",
      "additionalProperties": true,
      "properties": {
        "source": {
          "type": ["string", "null"],
          "description": "Identifier for the input source (e.g., filename, URI)."
        },
        "timestamp": {
          "type": ["string", "null"],
          "description": "ISO 8601 timestamp, if provided by the caller."
        },
        "user": {
          "type": ["string", "null"],
          "description": "User identifier, if provided by the caller."
        },
        "session_id": {
          "type": ["string", "null"],
          "description": "Session identifier, if provided by the caller."
        },
        "extra": {
          "type": ["object", "null"],
          "description": "Arbitrary additional metadata.",
          "additionalProperties": true
        }
      }
    },
    "commands": {
      "type": "array",
      "description": "Ordered array of all detected commands, in document order.",
      "items": { "$ref": "#/$defs/Command" }
    },
    "text_blocks": {
      "type": "array",
      "description": "Ordered array of non-command text regions, in document order.",
      "items": { "$ref": "#/$defs/TextBlock" }
    },
    "warnings": {
      "type": "array",
      "description": "Parser warnings (e.g., unclosed fences). Empty array if no warnings.",
      "items": { "$ref": "#/$defs/Warning" }
    }
  },
  "$defs": {
    "LineRange": {
      "type": "object",
      "description": "Zero-based physical line range (inclusive on both ends).",
      "required": ["start_line", "end_line"],
      "additionalProperties": false,
      "properties": {
        "start_line": {
          "type": "integer",
          "minimum": 0,
          "description": "First physical line (zero-based) covered by this element."
        },
        "end_line": {
          "type": "integer",
          "minimum": 0,
          "description": "Last physical line (zero-based) covered by this element."
        }
      }
    },
    "CommandArguments": {
      "type": "object",
      "description": "Parsed argument structure for a command. Content is opaque to the parser.",
      "required": ["header", "mode", "fence_lang", "payload"],
      "additionalProperties": false,
      "properties": {
        "header": {
          "type": "string",
          "description": "Inline argument text from the command line, before any fence opener. Serves as the dispatch/routing portion in both single-line and fence modes."
        },
        "mode": {
          "type": "string",
          "enum": ["single-line", "fence"],
          "description": "How the arguments were assembled."
        },
        "fence_lang": {
          "type": ["string", "null"],
          "description": "Language identifier from the fence opener, or null if not in fence mode or no language was specified."
        },
        "payload": {
          "type": "string",
          "description": "Assembled argument content. In single-line mode, identical to header. In fence mode, the verbatim content between the fence delimiters."
        }
      }
    },
    "Command": {
      "type": "object",
      "description": "A single parsed slash command.",
      "required": ["id", "name", "raw", "range", "arguments"],
      "additionalProperties": false,
      "properties": {
        "id": {
          "type": "string",
          "pattern": "^cmd-[0-9]+$",
          "description": "Sequential zero-based identifier (cmd-0, cmd-1, ...)."
        },
        "name": {
          "type": "string",
          "pattern": "^[a-z][a-z0-9-]*$",
          "description": "Command name without the leading slash."
        },
        "raw": {
          "type": "string",
          "description": "Exact source text from the normalized input (before line joining), including backslashes and physical newlines for joined commands, and fence delimiters for fenced commands."
        },
        "range": {
          "$ref": "#/$defs/LineRange",
          "description": "Physical line range in the normalized input covered by this command."
        },
        "arguments": { "$ref": "#/$defs/CommandArguments" }
      }
    },
    "TextBlock": {
      "type": "object",
      "description": "A contiguous region of non-command text.",
      "required": ["id", "range", "content"],
      "additionalProperties": false,
      "properties": {
        "id": {
          "type": "string",
          "pattern": "^text-[0-9]+$",
          "description": "Sequential zero-based identifier (text-0, text-1, ...)."
        },
        "range": {
          "$ref": "#/$defs/LineRange",
          "description": "Physical line range covered by this text block."
        },
        "content": {
          "type": "string",
          "description": "Original text content with lines joined by newline separators."
        }
      }
    },
    "Warning": {
      "type": "object",
      "description": "A parser warning indicating a non-fatal issue.",
      "required": ["type"],
      "additionalProperties": true,
      "properties": {
        "type": {
          "type": "string",
          "description": "Warning category identifier (e.g., 'unclosed_fence')."
        },
        "start_line": {
          "type": "integer",
          "minimum": 0,
          "description": "Physical line number where the issue was detected."
        },
        "message": {
          "type": "string",
          "description": "Human-readable description of the warning."
        }
      }
    }
  }
}
```

### 8.6 Parser Engine Interface

The parser engine exposes two entry points. Implementations may adapt the signatures to their host
language, but the semantics must be preserved.

1. **Parse with default context.** Accepts a single UTF-8 string input. Returns a
   `SlashParserResult` envelope with an empty `context` object.

Logical signature:

    parse(input: string) → SlashParserResult

2. **Parse with caller-supplied context.** Accepts a UTF-8 string input and a context object. The
   context is passed through to the envelope unchanged. The parser does not interpret context
   fields.

Logical signature:

    parse(input: string, context: object) → SlashParserResult

Both entry points are pure functions: no I/O, no global mutable state, no side effects. All
configuration is supplied through the input parameters.

The return type is always `SlashParserResult` as defined in Section 8.5. Implementations must not
use exceptions, panics, or error return channels for parse outcomes. Warnings about malformed input
(such as unclosed fences) are reported inside the envelope's `warnings` array.

## 9. Conformance

### 9.1 No AST Requirement

A conforming implementation must not expose or require an abstract syntax tree as a public artifact.
The only standardized observable output is the JSON envelope defined in Section 8. Implementations
may use any internal data structures, but the contract with consumers is the envelope schema alone.

### 9.2 Incremental Emission

A conforming implementation must support finalizing each command or text block as soon as its last
physical line has been consumed. Memory usage must be bounded by the size of the largest single
command payload or text block, not the total size of the input.

### 9.3 Opaque Payload

The parser is command-agnostic. It never interprets argument content, does not tokenize, and does
not apply quoting, escaping, or key-value semantics to the `header` or `payload` fields. All
argument strings are treated as opaque byte sequences. Any higher-level parsing (e.g., splitting on
spaces, parsing JSON or YAML) is the sole responsibility of the command implementation.

### 9.4 Roundtrip Fidelity Invariant

Let `P` be a conforming parser and `F` be a formatter that generates a canonical plaintext
representation from `P`'s JSON envelope output. The following invariant must hold:

For any valid input `I`, parsing, formatting, and parsing again yields a structurally equivalent
result:

```
P(F(P(I))) ≡ P(I)
```

Structural equivalence means the same sequence of commands and text blocks with identical values for
`name`, `arguments.mode`, `arguments.header`, `arguments.payload`, `arguments.fence_lang`, and text
block `content`.

The invariant is structural, not byte-for-byte: `raw` values, line numbers, and whitespace details
may differ between the two parse results due to normalization and line joining. These are allowed
lossy transformations.

### 9.5 Determinism

A conforming implementation must be deterministic: given identical input bytes and identical
context, the output envelope must be byte-for-byte identical across invocations and across
implementations.

### 9.6 JSONL Envelope Semantics

In JSONL streaming mode, each line of the output stream is exactly one `SlashParserResult` envelope
encoded as a single JSON object. One input unit (e.g., one file or one string passed to the parse
function) produces exactly one JSONL line.

The `commands` and `text_blocks` arrays inside each envelope are ordered by document position.
Command IDs (`cmd-0`, `cmd-1`, ...) and text block IDs (`text-0`, `text-1`, ...) are scoped to their
envelope and reset for each input unit.

Implementations that process multiple inputs in a pipeline emit one JSONL line per input, in the
order the inputs were supplied. The parser itself is not aware of the pipeline; the JSONL framing is
the responsibility of the host runtime or CLI layer.

## Appendix A: Parsing Examples

### A.1 Single-Line Command

Input:

```text
/echo hello world
```

Output (commands array element):

```json
{
  "id": "cmd-0",
  "name": "echo",
  "raw": "/echo hello world",
  "range": { "start_line": 0, "end_line": 0 },
  "arguments": {
    "header": "hello world",
    "mode": "single-line",
    "fence_lang": null,
    "payload": "hello world"
  }
}
```

### A.2 Joined Multi-Line Command

Input (three physical lines):

```text
/deploy production \
  --region us-west-2 \
  --canary
```

After joining: `/deploy production   --region us-west-2   --canary`

Output:

```json
{
  "id": "cmd-0",
  "name": "deploy",
  "raw": "/deploy production \\\n  --region us-west-2 \\\n  --canary",
  "range": { "start_line": 0, "end_line": 2 },
  "arguments": {
    "header": "production   --region us-west-2   --canary",
    "mode": "single-line",
    "fence_lang": null,
    "payload": "production   --region us-west-2   --canary"
  }
}
```

### A.3 Fenced Command with Header

Input:

````text
/mcp call_tool write_file ```json
{ "path": "/src/index.ts" }
```
````

Output:

````json
{
  "id": "cmd-0",
  "name": "mcp",
  "raw": "/mcp call_tool write_file ```json\n{ \"path\": \"/src/index.ts\" }\n```",
  "range": { "start_line": 0, "end_line": 2 },
  "arguments": {
    "header": "call_tool write_file",
    "mode": "fence",
    "fence_lang": "json",
    "payload": "{ \"path\": \"/src/index.ts\" }"
  }
}
````

### A.4 Backslash Join into Fence

Input (four physical lines):

````text
/mcp call_tool write_file \
```json
{ "path": "foo" }
```
````

After joining, physical lines 0 and 1 become: ``/mcp call_tool write_file ```json``

Output:

````json
{
  "id": "cmd-0",
  "name": "mcp",
  "raw": "/mcp call_tool write_file \\\n```json\n{ \"path\": \"foo\" }\n```",
  "range": { "start_line": 0, "end_line": 3 },
  "arguments": {
    "header": "call_tool write_file",
    "mode": "fence",
    "fence_lang": "json",
    "payload": "{ \"path\": \"foo\" }"
  }
}
````

### A.5 Text Blocks and Multiple Commands

Input:

```text
Welcome to the deployment system.

/deploy staging
/notify team --channel ops
Deployment complete.
```

Output:

```json
{
  "version": "0.3.0",
  "context": {},
  "commands": [
    {
      "id": "cmd-0",
      "name": "deploy",
      "raw": "/deploy staging",
      "range": { "start_line": 2, "end_line": 2 },
      "arguments": {
        "header": "staging",
        "mode": "single-line",
        "fence_lang": null,
        "payload": "staging"
      }
    },
    {
      "id": "cmd-1",
      "name": "notify",
      "raw": "/notify team --channel ops",
      "range": { "start_line": 3, "end_line": 3 },
      "arguments": {
        "header": "team --channel ops",
        "mode": "single-line",
        "fence_lang": null,
        "payload": "team --channel ops"
      }
    }
  ],
  "text_blocks": [
    {
      "id": "text-0",
      "range": { "start_line": 0, "end_line": 1 },
      "content": "Welcome to the deployment system.\n"
    },
    {
      "id": "text-1",
      "range": { "start_line": 4, "end_line": 4 },
      "content": "Deployment complete."
    }
  ],
  "warnings": []
}
```

### A.6 Invalid Slash Lines

Input:

```text
/123 not a command
/ bare slash
/Hello capitalized
/deploy staging
```

Output: the first three lines form `text-0`; `/deploy staging` is `cmd-0`.

### A.7 Unclosed Fence

Input:

````text
/mcp call_tool ```json
{ "incomplete": true }
````

Output: the command is finalized with the accumulated payload, and a warning is emitted:

````json
{
  "version": "0.3.0",
  "context": {},
  "commands": [
    {
      "id": "cmd-0",
      "name": "mcp",
      "raw": "/mcp call_tool ```json\n{ \"incomplete\": true }",
      "range": { "start_line": 0, "end_line": 1 },
      "arguments": {
        "header": "call_tool",
        "mode": "fence",
        "fence_lang": "json",
        "payload": "{ \"incomplete\": true }"
      }
    }
  ],
  "text_blocks": [],
  "warnings": [
    {
      "type": "unclosed_fence",
      "start_line": 0,
      "message": "Fenced block opened at line 0 was never closed."
    }
  ]
}
````

### A.8 Closing Fence with Trailing Backslash

A closing fence line that ends with a trailing backslash closes the fence normally, then the
backslash triggers a line join with the next physical line. The joined content is outside the fenced
command.

Input (six physical lines):

````text
/mcp call_tool write_file -c \
```json
{ "path": "foo" }
``` \
\
production
````

Physical lines:

- 0: `/mcp call_tool write_file -c \`
- 1: `` ```json ``
- 2: `{ "path": "foo" }`
- 3: `` ``` \ ``
- 4: `\`
- 5: `production`

Line joining (outside fences):

- Lines 0 and 1 join: ``/mcp call_tool write_file -c ```json``
- Lines 2 is inside the fence (verbatim, no joining).
- Line 3 is the closing fence (backticks only after trim? No: it contains `` ``` \ ``, which after
  trimming whitespace is `` ```\ `` or `` ``` \ ``. The backslash means this is not solely
  backticks, so it is not a valid closing fence.)

Because line 3 is not a valid closing fence, the fence never closes. All remaining lines (2 through
5) become part of the fenced payload, and the parser emits an `unclosed_fence` warning.

Output:

````json
{
  "version": "0.3.0",
  "context": {},
  "commands": [
    {
      "id": "cmd-0",
      "name": "mcp",
      "raw": "/mcp call_tool write_file -c \\\n```json\n{ \"path\": \"foo\" }\n``` \\\n\\\nproduction",
      "range": { "start_line": 0, "end_line": 5 },
      "arguments": {
        "header": "call_tool write_file -c",
        "mode": "fence",
        "fence_lang": "json",
        "payload": "{ \"path\": \"foo\" }\n``` \\\n\\\nproduction"
      }
    }
  ],
  "text_blocks": [],
  "warnings": [
    {
      "type": "unclosed_fence",
      "start_line": 1,
      "message": "Fenced block opened at line 1 was never closed."
    }
  ]
}
````

To avoid this, write the closing fence on its own line without a trailing backslash:

````text
/mcp call_tool write_file -c \
```json
{ "path": "foo" }
```
````

### A.9 Proper Fence Close Followed by Additional Content

Input (six physical lines):

````text
/mcp call_tool write_file -c\
```json
{ "path": "foo" }
```
\
production
````

Physical lines:

- 0: `/mcp call_tool write_file -c\`
- 1: `` ```json ``
- 2: `{ "path": "foo" }`
- 3: `` ``` ``
- 4: `\`
- 5: `production`

Line joining (outside fences):

- Lines 0 and 1 join: ``/mcp call_tool write_file -c ```json``
- Line 2 is inside the fence (verbatim).
- Line 3 is a valid closing fence (solely backticks after trim). Fence closes. Parser returns to
  `idle`.
- Lines 4 and 5 join: `production`

Output:

````json
{
  "version": "0.3.0",
  "context": {},
  "commands": [
    {
      "id": "cmd-0",
      "name": "mcp",
      "raw": "/mcp call_tool write_file -c\\\n```json\n{ \"path\": \"foo\" }\n```",
      "range": { "start_line": 0, "end_line": 3 },
      "arguments": {
        "header": "call_tool write_file -c",
        "mode": "fence",
        "fence_lang": "json",
        "payload": "{ \"path\": \"foo\" }"
      }
    }
  ],
  "text_blocks": [
    { "id": "text-0", "range": { "start_line": 4, "end_line": 5 }, "content": " production" }
  ],
  "warnings": []
}
````

## Appendix B: Change Log

Changes from version 0.2.0:

- Replaced continuation markers (`" /"`) with POSIX-style backslash line joining, reducing the state
  machine from three states to two.
- Added explicit treatment of invalid slash lines in idle state (Section 3.2).
- Defined fence lifetime as "until closing fence or EOF" (Section 5.2.3).
- Formalized trailing backslash at EOF behavior (Section 2.2.2).
- Added backslash-join-then-fence example (Section 5.2.6).
- Added closing-fence-with-backslash examples (Appendix A.8, A.9).
- Stated opaque payload principle as a conformance requirement (Section 9.3).
- Stated roundtrip fidelity invariant as a testable property (Section 9.4).
- Stated incremental emission as a conformance constraint (Section 9.2).
- Committed to no-AST as a conformance requirement (Section 9.1).
- Added determinism requirement (Section 9.5).
- Clarified JSONL streaming semantics with rationale (Section 8.3).
- Documented `header` as the dispatch portion available in both modes (Section 3.4).
- Introduced `warnings` array for non-fatal issues (Section 5.2.5).
- Removed unused `children` field from Command schema.
- Only backtick fences are supported; tilde fences are explicitly excluded (Section 5.2).
- Version bumped to 0.3.0.
