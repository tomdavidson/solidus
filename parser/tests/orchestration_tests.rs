//! Integration tests for parse_document orchestration.
//!
//! These tests verify that the sub-modules are wired together correctly
//! by the state machine in parse.rs. Each test targets a specific
//! orchestration concern that cannot be tested at the unit level.
//!
//! Sub-module behavior (normalization, classification, joining, fence
//! accumulation, text accumulation) is covered by unit tests in each
//! module's #[cfg(test)] block. These tests do NOT re-verify sub-module
//! logic; they verify the decisions the orchestrator makes.

use slasher_engine::{parse_document, ArgumentMode, SPEC_VERSION};

// =============================================================================
// State: Idle -> single-line command -> Idle
// Orchestration: classify_line returns Command(SingleLine), orchestrator
// calls finalize_single_line, increments cmd_seq, returns to Idle.
// =============================================================================

#[test]
fn single_line_returns_to_idle() {
    // Two consecutive single-line commands prove the state machine returns
    // to Idle after each. If it didn't, the second would be lost.
    let r = parse_document("/cmd1 a\n/cmd2 b");
    assert_eq!(r.commands.len(), 2);
    assert_eq!(r.commands[0].name, "cmd1");
    assert_eq!(r.commands[1].name, "cmd2");
}

// =============================================================================
// State: Idle -> fence command -> InFence -> Completed -> Idle
// Orchestration: classify_line returns Command(Fence), orchestrator calls
// open_fence, transitions to InFence, feeds physical lines via
// next_physical, detects Completed, finalizes, returns to Idle.
// =============================================================================

#[test]
fn fence_completes_and_returns_to_idle() {
    // A fenced command followed by a single-line command proves the full
    // Idle -> InFence -> Idle cycle.
    let r = parse_document("/fenced ```\nbody line\n```\n/after arg");
    assert_eq!(r.commands.len(), 2);
    assert_eq!(r.commands[0].arguments.mode, ArgumentMode::Fence);
    assert_eq!(r.commands[0].arguments.payload, "body line");
    assert_eq!(r.commands[1].arguments.mode, ArgumentMode::SingleLine);
    assert_eq!(r.commands[1].name, "after");
}

// =============================================================================
// State: InFence -> EOF (unclosed)
// Orchestration: next_physical returns None, orchestrator calls
// finalize_fence(fence, true), pushes warning.
// =============================================================================

#[test]
fn unclosed_fence_at_eof() {
    let r = parse_document("/cmd ```\nline one\nline two");
    assert_eq!(r.commands.len(), 1);
    assert_eq!(r.commands[0].arguments.payload, "line one\nline two");
    assert_eq!(r.warnings.len(), 1);
    assert_eq!(r.warnings[0].start_line, Some(0));
}

// =============================================================================
// Flush: text flushed before command
// Orchestration: when classify_line returns Command, flush_text is called
// first, finalizing any pending text block.
// =============================================================================

#[test]
fn text_flushed_before_command() {
    let r = parse_document("prose line\n/cmd arg");
    assert_eq!(r.textblocks.len(), 1);
    assert_eq!(r.textblocks[0].content, "prose line");
    assert_eq!(r.textblocks[0].id, "text-0");
    // Text block appears before the command in document order.
    assert_eq!(r.textblocks[0].range.end_line, 0);
    assert_eq!(r.commands[0].range.start_line, 1);
}

// =============================================================================
// Flush: text flushed at EOF
// Orchestration: after the main loop breaks, flush_text is called to
// finalize any trailing text.
// =============================================================================

#[test]
fn text_flushed_at_eof() {
    let r = parse_document("/cmd arg\ntrailing prose");
    assert_eq!(r.commands.len(), 1);
    assert_eq!(r.textblocks.len(), 1);
    assert_eq!(r.textblocks[0].content, "trailing prose");
}

// =============================================================================
// Flush: no spurious text blocks
// Orchestration: consecutive commands with no text between them must NOT
// produce an empty text block.
// =============================================================================

#[test]
fn no_text_block_between_consecutive_commands() {
    let r = parse_document("/cmd1 a\n/cmd2 b\n/cmd3 c");
    assert_eq!(r.commands.len(), 3);
    assert!(r.textblocks.is_empty());
}

#[test]
fn no_text_block_after_fence_closer_before_command() {
    let r = parse_document("/f ```\nbody\n```\n/cmd arg");
    assert_eq!(r.commands.len(), 2);
    assert!(r.textblocks.is_empty());
}

// =============================================================================
// Raw wiring: joined single-line command
// Orchestration: step_idle rebuilds raw from physical line slice
// phys[first..=last].join("\n"), preserving backslashes.
// =============================================================================

#[test]
fn joined_command_raw_has_physical_lines() {
    let r = parse_document("/deploy prod \\\n --region us-west-2");
    assert_eq!(r.commands.len(), 1);
    // raw must contain both physical lines with the backslash intact.
    assert_eq!(r.commands[0].raw, "/deploy prod \\\n --region us-west-2");
    // But the payload is the joined logical content.
    assert!(r.commands[0].arguments.payload.contains("--region"));
}

// =============================================================================
// Raw wiring: fenced command
// Orchestration: raw is seeded by open_fence (opener line from phys slice),
// then accept_fence_line appends body and closer.
// =============================================================================

#[test]
fn fenced_raw_includes_opener_body_closer() {
    let r = parse_document("/cmd ```\nfirst\nsecond\n```");
    assert_eq!(r.commands[0].raw, "/cmd ```\nfirst\nsecond\n```");
}

// =============================================================================
// Physical text: text block uses physical lines, not logical lines
// Orchestration: accumulate_text calls fold_physical_lines over the phys
// slice, so backslash-continued text lines keep their backslashes.
// Engine Spec §10: "A logical line formed by backslash continuation
// contributes all of its constituent physical lines with backslashes
// intact to the text block content."
// =============================================================================

#[test]
fn text_block_preserves_physical_lines_with_backslashes() {
    // "hello \" and " world" are two physical lines that join into one
    // logical line. The text block content must have BOTH physical lines.
    let r = parse_document("hello \\\n world");
    assert_eq!(r.textblocks.len(), 1);
    assert_eq!(r.textblocks[0].content, "hello \\\n world");
    assert_eq!(r.textblocks[0].range.start_line, 0);
    assert_eq!(r.textblocks[0].range.end_line, 1);
}

// =============================================================================
// ID counters: independent cmd_seq and text_seq
// Orchestration: cmd_seq and text_seq are separate fields in ParseCtx,
// incremented independently.
// =============================================================================

#[test]
fn id_counters_independent_across_interleaving() {
    let r = parse_document("t0\n/c0 a\nt1\n/c1 b\nt2");
    assert_eq!(r.textblocks[0].id, "text-0");
    assert_eq!(r.commands[0].id, "cmd-0");
    assert_eq!(r.textblocks[1].id, "text-1");
    assert_eq!(r.commands[1].id, "cmd-1");
    assert_eq!(r.textblocks[2].id, "text-2");
}

#[test]
fn fenced_command_shares_cmd_seq_with_single_line() {
    // A fence command consumes cmd_seq=0, the next single-line gets cmd_seq=1.
    let r = parse_document("/fenced ```\nbody\n```\n/single arg");
    assert_eq!(r.commands[0].id, "cmd-0");
    assert_eq!(r.commands[1].id, "cmd-1");
}

// =============================================================================
// split_physical_lines: trailing LF handling
// Orchestration: split_physical_lines pops trailing empty element so
// "input\n" does not create a phantom empty text block.
// =============================================================================

#[test]
fn trailing_lf_no_phantom_text_block() {
    let r = parse_document("/cmd arg\n");
    assert_eq!(r.commands.len(), 1);
    assert!(r.textblocks.is_empty());
}

#[test]
fn no_trailing_lf_still_processes_last_line() {
    let r = parse_document("/cmd1 a\n/cmd2 b");
    assert_eq!(r.commands.len(), 2);
    assert_eq!(r.commands[1].name, "cmd2");
}

// =============================================================================
// Fence boundary: next_logical resumes after fence closes
// Orchestration: after FenceResult::Completed, state returns to Idle,
// and the next iteration calls next_logical (with joining active).
// This means a backslash-continued line after a fence closer is joined.
// =============================================================================

#[test]
fn joining_resumes_after_fence_closes() {
    // After the fence closes, "hello \" + " world" should join into one
    // logical text line.
    let r = parse_document("/cmd ```\nbody\n```\nhello \\\n world");
    assert_eq!(r.commands.len(), 1);
    assert_eq!(r.textblocks.len(), 1);
    // The text block preserves physical lines (backslash intact).
    assert_eq!(r.textblocks[0].content, "hello \\\n world");
    // But it's a single logical line, so only one text block.
    assert_eq!(r.textblocks[0].range.start_line, 3);
    assert_eq!(r.textblocks[0].range.end_line, 4);
}

// =============================================================================
// Fence body: next_physical bypasses joining
// Orchestration: in InFence state, the orchestrator calls next_physical,
// so backslashes inside the fence body are NOT consumed as join markers.
// =============================================================================

#[test]
fn backslash_in_fence_body_is_literal() {
    let r = parse_document("/cmd ```\nline one \\\nline two\n```");
    assert_eq!(r.commands.len(), 1);
    // Both lines are separate payload lines; the backslash did not join them.
    assert_eq!(r.commands[0].arguments.payload, "line one \\\nline two");
}

// =============================================================================
// Version wiring
// Orchestration: into_result sets version from SPEC_VERSION.
// =============================================================================

#[test]
fn version_from_spec_constant() {
    let r = parse_document("");
    assert_eq!(r.version, SPEC_VERSION);
}

// =============================================================================
// Full scenario: RFC Appendix B.5 equivalent
// Orchestration: exercises the complete interleaving of text blocks,
// commands, blank lines, and ID assignment in a single document.
// =============================================================================

#[test]
fn full_scenario_text_commands_interleaved() {
    let input = "Welcome to the system.\n\n/deploy staging\n/notify team --channel ops\nDone.";
    let r = parse_document(input);

    // Text block 0: lines 0-1 (prose + blank line)
    assert_eq!(r.textblocks[0].id, "text-0");
    assert_eq!(r.textblocks[0].content, "Welcome to the system.\n");
    assert_eq!(r.textblocks[0].range.start_line, 0);
    assert_eq!(r.textblocks[0].range.end_line, 1);

    // Commands
    assert_eq!(r.commands[0].id, "cmd-0");
    assert_eq!(r.commands[0].name, "deploy");
    assert_eq!(r.commands[0].arguments.payload, "staging");

    assert_eq!(r.commands[1].id, "cmd-1");
    assert_eq!(r.commands[1].name, "notify");
    assert_eq!(r.commands[1].arguments.payload, "team --channel ops");

    // Text block 1: trailing prose
    assert_eq!(r.textblocks[1].id, "text-1");
    assert_eq!(r.textblocks[1].content, "Done.");
}

// =============================================================================
// Empty and minimal inputs
// Orchestration: total function guarantee, no panics, correct empty result.
// =============================================================================

#[test]
fn empty_input() {
    let r = parse_document("");
    assert!(r.commands.is_empty());
    assert!(r.textblocks.is_empty());
    assert!(r.warnings.is_empty());
}

#[test]
fn single_newline_only() {
    // "\n" normalizes to one LF. split_physical_lines produces [""] then
    // pops it. Result should be empty.
    let r = parse_document("\n");
    assert!(r.commands.is_empty());
    assert!(r.textblocks.is_empty());
}

// =============================================================================
// Invalid slash lines flow through as text
// RFC §4.5 / RFC Appendix B.6
// =============================================================================

#[test]
fn invalid_slash_lines_are_text() {
    let input = "/123\n/ bare\n/Hello\n/cmd- trailing\n/deploy staging";
    let r = parse_document(input);
    // Only "/deploy" is a valid command.
    assert_eq!(r.commands.len(), 1);
    assert_eq!(r.commands[0].name, "deploy");
    // The four invalid lines form one text block before the command.
    assert_eq!(r.textblocks.len(), 1);
    assert_eq!(r.textblocks[0].range.start_line, 0);
    assert_eq!(r.textblocks[0].range.end_line, 3);
}
