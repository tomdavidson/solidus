use super::{
    classify::{CommandHeader, LineKind, classify_line},
    fence::{FenceResult, PendingFence, accept_fence_line, finalize_fence, open_fence},
    join::LineJoiner,
    normalize::normalize,
    single_line::finalize_single_line,
    text::{PendingText, append_text, finalize_text, start_text},
};
use crate::{
    LineRange,
    ArgumentMode, Command, ParseResult, SPEC_VERSION, TextBlock, Warning,
};

// --- Types ---

#[derive(Debug)]
enum ParserState {
    Idle,
    InFence(PendingFence),
}

#[derive(PartialEq, Eq)]
enum LoopAction {
    Continue,
    Break,
}

struct ParseCtx {
    state: ParserState,
    current_text: Option<PendingText>,
    commands: Vec<Command>,
    textblocks: Vec<TextBlock>,
    warnings: Vec<Warning>,
    cmd_seq: usize,
    text_seq: usize,
}

impl ParseCtx {
    fn new() -> Self {
        Self {
            state: ParserState::Idle,
            current_text: None,
            commands: Vec::new(),
            textblocks: Vec::new(),
            warnings: Vec::new(),
            cmd_seq: 0,
            text_seq: 0,
        }
    }

    fn into_result(self) -> ParseResult {
        ParseResult {
            version: SPEC_VERSION.to_owned(),
            commands: self.commands,
            textblocks: self.textblocks,
            warnings: self.warnings,
        }
    }
}

// --- Public entry point ---

pub fn parse_document(input: &str) -> ParseResult {
    let normalized = normalize(input);
    let physical_lines = split_physical_lines(&normalized);
    let owned: Vec<String> = physical_lines.iter().map(|s| s.to_string()).collect();
    let mut joiner = LineJoiner::new(owned);
    let mut ctx = ParseCtx::new();

    while step(&mut ctx, &mut joiner, &physical_lines) == LoopAction::Continue {}

    flush_text(&mut ctx);
    ctx.into_result()
}

// --- Pipeline ---

fn split_physical_lines(normalized: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = normalized.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

fn step(ctx: &mut ParseCtx, joiner: &mut LineJoiner, phys: &[&str]) -> LoopAction {
    let state = std::mem::replace(&mut ctx.state, ParserState::Idle);
    match state {
        ParserState::Idle => step_idle(ctx, joiner, phys),
        ParserState::InFence(cmd) => step_in_fence(ctx, joiner, cmd),
    }
}

// --- State handlers ---

fn step_idle(ctx: &mut ParseCtx, joiner: &mut LineJoiner, phys: &[&str]) -> LoopAction {
    let Some(ll) = joiner.next_logical() else {
        return LoopAction::Break;
    };
    match classify_line(&ll.text) {
        LineKind::Command(mut header) => {
            header.raw = phys[ll.first_physical..=ll.last_physical].join("\n");
            flush_text(ctx);
            start_new_command(ctx, header, ll.first_physical, ll.last_physical);
        }
        LineKind::Text => {
            accumulate_text(ctx, ll.first_physical, ll.last_physical, phys);
        }
    }
    LoopAction::Continue
}

fn step_in_fence(ctx: &mut ParseCtx, joiner: &mut LineJoiner, fence: PendingFence) -> LoopAction {
    let Some((line_idx, line)) = joiner.next_physical() else {
        let (cmd, warnings) = finalize_fence(fence, true);
        ctx.commands.push(cmd);
        ctx.warnings.extend(warnings);
        return LoopAction::Break;
    };
    let (updated, result) = accept_fence_line(fence, line_idx, &line);
    match result {
        FenceResult::Consumed => {
            ctx.state = ParserState::InFence(updated);
        }
        FenceResult::Completed => {
            let (cmd, warnings) = finalize_fence(updated, false);
            ctx.commands.push(cmd);
            ctx.warnings.extend(warnings);
        }
    }
    LoopAction::Continue
}

// --- Context helpers ---

fn start_new_command(ctx: &mut ParseCtx, header: CommandHeader, first_physical: usize, last_physical: usize) {
    match header.mode {
        ArgumentMode::SingleLine => {
            let raw = header.raw.clone();
            let range = LineRange { start_line: first_physical, end_line: last_physical };
            let cmd = finalize_single_line(header, raw, ctx.cmd_seq, range);
            ctx.commands.push(cmd);
            ctx.cmd_seq += 1;
        }
        ArgumentMode::Fence => {
            let raw = header.raw.clone();
            let range = LineRange { start_line: first_physical, end_line: last_physical };
            let fence = open_fence(header, raw, ctx.cmd_seq, range);
            ctx.cmd_seq += 1;
            ctx.state = ParserState::InFence(fence);
        }
    }
}

fn flush_text(ctx: &mut ParseCtx) {
    let Some(text) = ctx.current_text.take() else {
        return;
    };
    ctx.textblocks.push(finalize_text(text, ctx.text_seq));
    ctx.text_seq += 1;
}

fn accumulate_text(ctx: &mut ParseCtx, first: usize, last: usize, phys: &[&str]) {
    let text = match ctx.current_text.take() {
        Some(existing) => fold_physical_lines(existing, first, last, phys),
        None => {
            let started = start_text(first, phys[first]);
            fold_physical_lines(started, first + 1, last, phys)
        }
    };
    ctx.current_text = Some(text);
}

fn fold_physical_lines(mut text: PendingText, from: usize, to: usize, phys: &[&str]) -> PendingText {
    for (idx, line) in (from..=to).zip(&phys[from..=to]) {
        text = append_text(text, idx, line);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::parse_document;
    // NOTE: All imports below are from crate::domain (external to this file).
    // parse_document orchestrates: normalize, split_physical_lines, LineJoiner,
    // classify_line, finalize_single_line, open_fence, accept_fence_line,
    // finalize_fence, start_text, append_text, finalize_text.
    use crate::{ArgumentMode, SPEC_VERSION};

    // =========================================================================
    // Empty / trivial input
    // RFC §8.2 item 4 / Engine Spec §4.2
    // =========================================================================

    #[test]
    fn empty_input() {
        // RFC §8.2 item 4: "An empty input produces a result with no commands,
        // no text blocks, and no warnings."
        // Engine Spec §4.2: total function guarantee.
        let r = parse_document("");
        assert!(r.commands.is_empty());
        assert!(r.textblocks.is_empty());
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn whitespace_only_is_text() {
        // RFC §6.3: "A text line is any non-fence-body logical line that is
        // not a command line." Whitespace-only lines are text.
        let r = parse_document("   ");
        assert!(r.commands.is_empty());
        assert_eq!(r.textblocks.len(), 1);
    }

    // =========================================================================
    // Single-line command — end-to-end threading
    // RFC §5.1 / Engine Spec §8 / Engine Spec §9.3
    // =========================================================================

    #[test]
    fn single_line_command_fields() {
        // RFC §5.1: single-line mode, header == payload.
        // RFC §6.5: id is cmd-0.
        // RFC §7.1: name, raw, range, mode, payload.
        let r = parse_document("/deploy production --region us-west-2");
        assert_eq!(r.commands.len(), 1);
        let cmd = &r.commands[0];
        assert_eq!(cmd.id, "cmd-0");
        assert_eq!(cmd.name, "deploy");
        assert_eq!(cmd.arguments.mode, ArgumentMode::SingleLine);
        assert_eq!(cmd.arguments.payload, "production --region us-west-2");
        assert_eq!(cmd.arguments.header, "production --region us-west-2");
        assert_eq!(cmd.raw, "/deploy production --region us-west-2");
        assert_eq!(cmd.range.start_line, 0);
        assert_eq!(cmd.range.end_line, 0);
    }

    #[test]
    fn single_line_no_args() {
        // RFC §4.3: "The arguments portion may be empty."
        // RFC §5.1: empty args -> empty header and payload.
        let r = parse_document("/ping");
        let cmd = &r.commands[0];
        assert_eq!(cmd.arguments.header, "");
        assert_eq!(cmd.arguments.payload, "");
    }

    // =========================================================================
    // Fenced command — end-to-end threading
    // RFC §5.2 / Engine Spec §9
    // =========================================================================

    #[test]
    fn fenced_command_fields() {
        // RFC §5.2.2: body lines joined with LF.
        // RFC §5.2.1: fence_lang from opener.
        // RFC §7.1: raw includes opener, body, closer.
        // Engine Spec §3.6: range is inclusive physical lines.
        let r = parse_document("/cmd ```json\nline one\nline two\n```");
        assert_eq!(r.commands.len(), 1);
        let cmd = &r.commands[0];
        assert_eq!(cmd.arguments.payload, "line one\nline two");
        assert_eq!(cmd.arguments.mode, ArgumentMode::Fence);
        assert_eq!(cmd.arguments.fence_lang, Some("json".to_string()));
        assert_eq!(cmd.raw, "/cmd ```json\nline one\nline two\n```");
        assert_eq!(cmd.range.start_line, 0);
        assert_eq!(cmd.range.end_line, 3);
    }

    // =========================================================================
    // Unclosed fence
    // RFC §5.2.4 / Engine Spec §9.3 step 4
    // =========================================================================

    #[test]
    fn unclosed_fence_warning_and_partial_command() {
        // RFC §5.2.4: "A warning of type unclosed_fence MUST be produced."
        // RFC §5.2.4: "The command is complete with whatever payload has been
        // accumulated through EOF."
        //
        // NOTE: wtype is "unclosed-fence" (kebab-case). Engine Spec §11 and
        // RFC §7.4 require "unclosed_fence" (snake_case). This is a known
        // code bug inherited from fence.rs finalize_fence.
        let r = parse_document("/cmd ```\npartial body");
        assert_eq!(r.commands.len(), 1);
        assert_eq!(r.commands[0].arguments.payload, "partial body");
        assert_eq!(r.warnings.len(), 1);
        assert_eq!(r.warnings[0].wtype, "unclosed-fence");
    }

    // =========================================================================
    // Text block accumulation
    // RFC §6.3 / RFC §6.4 / Engine Spec §10
    // =========================================================================

    #[test]
    fn text_only() {
        // RFC §6.4: "Consecutive text lines form a single text block."
        // RFC §7.2: content is lines joined with LF.
        let r = parse_document("line one\nline two\nline three");
        assert_eq!(r.textblocks.len(), 1);
        assert_eq!(r.textblocks[0].content, "line one\nline two\nline three");
        assert_eq!(r.textblocks[0].range.start_line, 0);
        assert_eq!(r.textblocks[0].range.end_line, 2);
    }

    // =========================================================================
    // Interleaving commands and text
    // RFC §6 / RFC §6.4 / Engine Spec §5.3
    // =========================================================================

    #[test]
    fn text_before_command() {
        // RFC §6.4: text lines before a command form a text block.
        let r = parse_document("preamble\n/cmd arg");
        assert_eq!(r.textblocks.len(), 1);
        assert_eq!(r.textblocks[0].content, "preamble");
        assert_eq!(r.commands.len(), 1);
    }

    #[test]
    fn text_after_command() {
        // RFC §6.4: "A new text block begins after a command is finalized,
        // if text lines follow."
        let r = parse_document("/cmd arg\npostamble");
        assert_eq!(r.commands.len(), 1);
        assert_eq!(r.textblocks.len(), 1);
        assert_eq!(r.textblocks[0].content, "postamble");
    }

    #[test]
    fn text_between_commands() {
        // RFC §6.4: text between two commands forms its own block.
        let r = parse_document("/cmd1 a\nmiddle text\n/cmd2 b");
        assert_eq!(r.commands.len(), 2);
        assert_eq!(r.textblocks.len(), 1);
        assert_eq!(r.textblocks[0].content, "middle text");
    }

    #[test]
    fn consecutive_commands() {
        // RFC §6.5: multiple commands in document order.
        let r = parse_document("/cmd1 a\n/cmd2 b");
        assert_eq!(r.commands.len(), 2);
        assert_eq!(r.commands[0].name, "cmd1");
        assert_eq!(r.commands[1].name, "cmd2");
    }

    #[test]
    fn fence_followed_by_command() {
        // RFC Appendix C: after fence closes, parser returns to idle.
        let r = parse_document("/cmd1 ```\nbody\n```\n/cmd2 arg");
        assert_eq!(r.commands.len(), 2);
        assert_eq!(r.commands[0].arguments.mode, ArgumentMode::Fence);
        assert_eq!(r.commands[1].arguments.mode, ArgumentMode::SingleLine);
    }

    // =========================================================================
    // ID assignment — sequential, independent counters
    // RFC §6.5 / Engine Spec §3.2 / Engine Spec §3.5
    // =========================================================================

    #[test]
    fn command_ids_sequential() {
        // RFC §6.5: "cmd-0, cmd-1, cmd-2."
        let r = parse_document("/a x\n/b y\n/c z");
        assert_eq!(r.commands[0].id, "cmd-0");
        assert_eq!(r.commands[1].id, "cmd-1");
        assert_eq!(r.commands[2].id, "cmd-2");
    }

    #[test]
    fn text_ids_sequential() {
        // RFC §6.5: "text-0, text-1, text-2."
        let r = parse_document("aaa\n/cmd x\nbbb\n/cmd y\nccc");
        assert_eq!(r.textblocks[0].id, "text-0");
        assert_eq!(r.textblocks[1].id, "text-1");
        assert_eq!(r.textblocks[2].id, "text-2");
    }

    #[test]
    fn command_and_text_ids_independent() {
        // RFC §6.5: command and text block ID sequences are independent.
        let r = parse_document("prose\n/cmd arg\nmore prose");
        assert_eq!(r.commands[0].id, "cmd-0");
        assert_eq!(r.textblocks[0].id, "text-0");
        assert_eq!(r.textblocks[1].id, "text-1");
    }

    // =========================================================================
    // raw field — physical line preservation
    // RFC §7.1 / Engine Spec §9.3 step 2
    // =========================================================================

    #[test]
    fn single_line_raw() {
        // RFC §7.1: raw is "the exact source text from the normalized input."
        let r = parse_document("/echo hello world");
        assert_eq!(r.commands[0].raw, "/echo hello world");
    }

    #[test]
    fn joined_command_raw_preserves_backslashes() {
        // RFC §7.1: "For joined commands, this includes all physical lines
        // with their backslashes and LF separators."
        let r = parse_document("/deploy prod \\\n --region us-west-2");
        assert_eq!(r.commands[0].raw, "/deploy prod \\\n --region us-west-2");
    }

    #[test]
    fn fenced_raw_includes_all_lines() {
        // RFC §7.1: "For fenced commands, this includes the opener line,
        // all body lines, and the closer line if present."
        let r = parse_document("/cmd ```\nbody\n```");
        assert_eq!(r.commands[0].raw, "/cmd ```\nbody\n```");
    }

    // =========================================================================
    // Trailing newline edge case
    // Engine Spec §5.2 / RFC §3.1
    // =========================================================================

    #[test]
    fn trailing_newline_no_empty_text_block() {
        // Engine Spec §5.2: "the current engine pops a trailing empty element
        // when the input ends with LF."
        let r = parse_document("/cmd arg\n");
        assert_eq!(r.commands.len(), 1);
        assert!(r.textblocks.is_empty());
    }

    // =========================================================================
    // Version field
    // Engine Spec §14 / Engine Spec §3.1
    // =========================================================================

    #[test]
    fn version_set() {
        // Engine Spec §14: SPEC_VERSION populated into ParseResult.version.
        let r = parse_document("");
        assert_eq!(r.version, SPEC_VERSION);
    }

    // =========================================================================
    // Property tests
    // =========================================================================

    use proptest::prelude::*;

    proptest! {
        // RFC §8.2 item 4: "always produces a valid result for any input."
        // Engine Spec §4.2: total function, never panics.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn never_panics(input in "\\PC{0,500}") {
            let _ = parse_document(&input);
        }

        // Engine Spec §14: version is always SPEC_VERSION.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn version_always_spec_version(input in "\\PC{0,200}") {
            let r = parse_document(&input);
            prop_assert_eq!(r.version, SPEC_VERSION);
        }
    }
}

// =============================================================================
// TEST GAPS: spec areas this file's functions touch but are not tested
// =============================================================================
//
// | Spec Section                      | Gap                                              | Severity |
// |-----------------------------------|--------------------------------------------------|----------|
// | Engine Spec §11 / RFC §7.4        | WARNING TYPE STRING: code emits "unclosed-fence"  | CRITICAL |
// |                                   | (kebab-case) but Engine Spec §11 requires          |          |
// |                                   | "unclosed_fence" (snake_case). This is a code bug  |          |
// |                                   | in finalize_fence, observable at integration level. |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | Engine Spec §7.1 vs RFC §3.2      | SPACE INSERTION: join.rs inserts a space between   | CRITICAL |
// |                                   | joined lines. Engine Spec §7.1 says "No separator  |          |
// |                                   | character is inserted." RFC §3.2 step 3 says       |          |
// |                                   | "separated by a single SPACE." Specs contradict.   |          |
// |                                   | No integration test asserts the exact join result.  |          |
// |                                   | joined_command_raw_preserves_backslashes tests raw  |          |
// |                                   | but not the joined logical line / payload content.  |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC §6.1 / Engine Spec §10        | TEXT BLOCK CONTENT IS PHYSICAL: Engine Spec §10    | HIGH     |
// |                                   | says "A logical line formed by backslash            |          |
// |                                   | continuation contributes all of its constituent     |          |
// |                                   | physical lines with backslashes intact." No test    |          |
// |                                   | verifies that a text block containing a joined line |          |
// |                                   | preserves the physical lines (with backslashes) in  |          |
// |                                   | its content, not the joined logical text.           |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC Appendix B.5                  | MIXED SCENARIO: RFC Appendix B.5 has a full        | MEDIUM   |
// |                                   | worked example with blank line in text, two         |          |
// |                                   | commands, and trailing text. No integration test    |          |
// |                                   | replicates this exact scenario end-to-end.          |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC Appendix B.6                  | INVALID SLASH LINES: RFC Appendix B.6 shows        | MEDIUM   |
// |                                   | "/123", "/ bare slash", "/Hello" as text lines.     |          |
// |                                   | No integration test verifies these pass through     |          |
// |                                   | parse_document as text blocks.                      |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC Appendix B.4                  | BACKSLASH JOIN INTO FENCE: A command line           | MEDIUM   |
// |                                   | split across physical lines via backslash that      |          |
// |                                   | joins into a fence opener (e.g., "/mcp \" + "```"). |          |
// |                                   | No integration test for this scenario.              |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC §5.2.3 / Appendix B.8-B.9    | FENCE CLOSER WITH TRAILING BACKSLASH: B.8 shows    | MEDIUM   |
// |                                   | "```\" is NOT a closer (backslash makes it non-     |          |
// |                                   | solely-backtick). B.9 shows "```" as a valid closer |          |
// |                                   | followed by content that joins. Neither scenario    |          |
// |                                   | has an integration test.                            |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC §6.4                          | BLANK LINES IN TEXT BLOCKS: No test for blank       | LOW      |
// |                                   | lines within a text block (e.g., "a\n\nb" producing |          |
// |                                   | content "a\n\nb"). Covered in text.rs unit tests    |          |
// |                                   | but not at integration level.                       |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC §8.3                          | ROUNDTRIP FIDELITY: No test asserts that            | LOW      |
// |                                   | P(F(P(I))) == P(I) for any input. This would        |          |
// |                                   | require a formatter, which is an SDK concern, but   |          |
// |                                   | the engine should preserve enough data to enable it.|          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC §8.4                          | DETERMINISM: No property test asserts that          | LOW      |
// |                                   | parse_document(x) == parse_document(x) for same    |          |
// |                                   | input. Trivially true for pure functions but would  |          |
// |                                   | guard against accidental HashMap usage.             |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | Engine Spec §6                    | WHITESPACE DEFINITION: step_idle calls              | HIGH     |
// |                                   | classify_line which uses .trim_start() and          |          |
// |                                   | .is_whitespace() internally. Engine Spec §6         |          |
// |                                   | mandates SP+HTAB only. No integration test with     |          |
// |                                   | U+00A0 or other Unicode WSP verifying that such     |          |
// |                                   | lines are NOT treated as having leading whitespace. |          |
// |                                   | This is a classify.rs bug observable here.          |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | Engine Spec §5.2                  | SPLIT_PHYSICAL_LINES EDGE CASES: split_physical_   | LOW      |
// |                                   | lines is a private fn in this file. No direct test  |          |
// |                                   | for inputs like "\n" (single LF), "\n\n" (two LFs),|          |
// |                                   | or "no newline" (no LF). The trailing-newline test  |          |
// |                                   | covers one case only.                               |          |
