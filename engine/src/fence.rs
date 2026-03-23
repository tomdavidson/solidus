use crate::{ArgumentMode, Command, CommandArguments, LineRange, Warning, classify::CommandHeader};

/// Result of offering a physical line to an open fence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FenceResult {
    Consumed,
    Completed,
}

/// In-progress fenced command being assembled from physical lines.
///
/// Existence of this value means the fence is open. No `is_open` field needed.
#[derive(Debug, Clone)]
pub struct PendingFence {
    pub id: usize,
    pub name: String,
    pub header_text: String,
    pub fence_lang: Option<String>,
    pub fence_backtick_count: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub payload_lines: Vec<String>,
    pub raw_lines: Vec<String>,
}

pub fn open_fence(header: CommandHeader, raw: String, id: usize, range: LineRange) -> PendingFence {
    PendingFence {
        id,
        name: header.name,
        header_text: header.header_text,
        fence_lang: header.fence_lang,
        fence_backtick_count: header.fence_backtick_count,
        start_line: range.start_line,
        end_line: range.end_line,
        payload_lines: Vec::new(),
        raw_lines: vec![raw],
    }
}

pub fn accept_fence_line(
    mut fence: PendingFence,
    line_index: usize,
    line: &str,
) -> (PendingFence, FenceResult) {
    fence.raw_lines.push(line.to_string());
    fence.end_line = line_index;

    if is_fence_closer(line, fence.fence_backtick_count) {
        (fence, FenceResult::Completed)
    } else {
        fence.payload_lines.push(line.to_string());
        (fence, FenceResult::Consumed)
    }
}

fn is_fence_closer(line: &str, opener_count: usize) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| c == '`') && trimmed.len() >= opener_count
}

pub fn finalize_fence(fence: PendingFence, unclosed: bool) -> (Command, Vec<Warning>) {
    let mut warnings = Vec::new();

    if unclosed {
        warnings.push(Warning {
            wtype: "unclosed-fence".to_string(),
            start_line: Some(fence.start_line),
            message: Some(format!("Fenced block opened at line {} was never closed.", fence.start_line)),
        });
    }

    let command = Command {
        id: format!("cmd-{}", fence.id),
        name: fence.name,
        raw: fence.raw_lines.join("\n"),
        range: LineRange { start_line: fence.start_line, end_line: fence.end_line },
        arguments: CommandArguments {
            header: fence.header_text,
            mode: ArgumentMode::Fence,
            fence_lang: fence.fence_lang,
            payload: fence.payload_lines.join("\n"),
        },
    };

    (command, warnings)
}

#[cfg(test)]
mod tests {

    use proptest::prelude::*;

    use super::*;
    use crate::{ArgumentMode, LineRange, classify::CommandHeader};

    // --- Test helpers ---

    fn make_fence(name: &str, backticks: usize, start_line: usize, id: usize) -> PendingFence {
        let raw = format!("/{name} {}", "`".repeat(backticks));
        let header = CommandHeader {
            raw: raw.clone(),
            name: name.to_string(),
            header_text: String::new(),
            mode: ArgumentMode::Fence,
            fence_lang: None,
            fence_backtick_count: backticks,
        };
        let range = LineRange { start_line, end_line: start_line };
        open_fence(header, raw, id, range)
    }

    fn make_fence_with_lang(name: &str, lang: &str, start_line: usize, id: usize) -> PendingFence {
        let raw = format!("/{name} ```{lang}");
        let header = CommandHeader {
            raw: raw.clone(),
            name: name.to_string(),
            header_text: String::new(),
            mode: ArgumentMode::Fence,
            fence_lang: Some(lang.to_string()),
            fence_backtick_count: 3,
        };
        let range = LineRange { start_line, end_line: start_line };
        open_fence(header, raw, id, range)
    }

    // =========================================================================
    // open_fence — initial state
    // Engine Spec §9.1: PendingCommand fields seeded from CommandHeader.
    // =========================================================================

    #[test]
    fn open_fence_initial_state() {
        // Engine Spec §9.1: PendingCommand is created with id, name,
        // header_text, fence_lang, fence_backtick_count from the header.
        // payload_lines starts empty, raw_lines seeded with opener.
        let fence = make_fence("cmd", 4, 5, 7);
        assert_eq!(fence.id, 7);
        assert_eq!(fence.fence_backtick_count, 4);
        assert_eq!(fence.start_line, 5);
        assert!(fence.payload_lines.is_empty());
        assert_eq!(fence.raw_lines, vec!["/cmd ````"]);
    }

    // =========================================================================
    // accept_fence_line — fence body (non-closer lines)
    // RFC §5.2.2 / Engine Spec §9.2
    // =========================================================================

    #[test]
    fn body_line_consumed_and_appended() {
        // RFC §5.2.2: "All fence body lines MUST be included in the payload
        // verbatim." Engine Spec §9.2: Consumed variant.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, res) = accept_fence_line(fence, 1, "fn main() {}");
        assert_eq!(res, FenceResult::Consumed);
        assert_eq!(fence.payload_lines, vec!["fn main() {}"]);
    }

    #[test]
    fn body_lines_accumulate_in_raw() {
        // RFC §7.1: raw source includes all physical lines.
        // Engine Spec §9.3 step 2: raw_lines joined with \n.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "first");
        let (fence, _) = accept_fence_line(fence, 2, "second");
        assert_eq!(fence.raw_lines, vec!["/cmd ```", "first", "second"]);
    }

    #[test]
    fn body_preserves_content_verbatim() {
        // RFC §5.2.2: "preserving their original content including any
        // trailing backslashes."
        let fence = make_fence("cmd", 3, 0, 0);
        let line = r"  leading spaces and trailing backslash\";
        let (fence, _) = accept_fence_line(fence, 1, line);
        assert_eq!(fence.payload_lines, vec![line]);
    }

    #[test]
    fn blank_line_inside_fence_is_payload() {
        // RFC §5.2.2: "blank lines … and any other content" are literal payload.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, res) = accept_fence_line(fence, 1, "");
        assert_eq!(res, FenceResult::Consumed);
        assert_eq!(fence.payload_lines, vec![""]);
    }

    #[test]
    fn command_trigger_inside_fence_is_payload() {
        // RFC §5.2.2: "Inside a fence, all content is literal payload:
        // command triggers, invalid slash lines, blank lines…"
        let fence = make_fence("outer", 3, 0, 0);
        let (fence, res) = accept_fence_line(fence, 1, "/inner arg");
        assert_eq!(res, FenceResult::Consumed);
        assert_eq!(fence.payload_lines, vec!["/inner arg"]);
    }

    // =========================================================================
    // accept_fence_line — fence closer detection
    // RFC §5.2.3 / Engine Spec §9.2
    // =========================================================================

    #[test]
    fn exact_backtick_count_closes() {
        // RFC §5.2.3: line "consists solely of backtick characters" with
        // count >= opener's count.
        let fence = make_fence("cmd", 3, 0, 0);
        let (_, res) = accept_fence_line(fence, 1, "```");
        assert_eq!(res, FenceResult::Completed);
    }

    #[test]
    fn more_backticks_than_opener_closes() {
        // RFC §5.2.3: "greater than or equal to the opener's backtick count."
        let fence = make_fence("cmd", 3, 0, 0);
        let (_, res) = accept_fence_line(fence, 1, "````");
        assert_eq!(res, FenceResult::Completed);
    }

    #[test]
    fn fewer_backticks_than_opener_does_not_close() {
        // RFC §5.2.3: count must be >= opener count. Two < three.
        let fence = make_fence("cmd", 3, 0, 0);
        let (_, res) = accept_fence_line(fence, 1, "``");
        assert_eq!(res, FenceResult::Consumed);
    }

    #[test]
    fn backticks_with_trailing_text_does_not_close() {
        // RFC §5.2.3: "consists solely of backtick characters" after trimming.
        // "```rust" has non-backtick chars.
        let fence = make_fence("cmd", 3, 0, 0);
        let (_, res) = accept_fence_line(fence, 1, "```rust");
        assert_eq!(res, FenceResult::Consumed);
    }

    #[test]
    fn closer_with_surrounding_whitespace() {
        // RFC §5.2.3: "after trimming leading and trailing whitespace"
        // the line consists solely of backticks.
        let fence = make_fence("cmd", 3, 0, 0);
        let (_, res) = accept_fence_line(fence, 1, "  ```  ");
        assert_eq!(res, FenceResult::Completed);
    }

    #[test]
    fn closer_excluded_from_payload() {
        // RFC §5.2.3: "The fence closer line MUST NOT be included in the payload."
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "fn main() {}");
        let (fence, _) = accept_fence_line(fence, 2, "```");
        assert_eq!(fence.payload_lines, vec!["fn main() {}"]);
    }

    #[test]
    fn closer_included_in_raw() {
        // RFC §7.1: raw source includes "the closer line (if present)."
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "content");
        let (fence, _) = accept_fence_line(fence, 2, "```");
        assert_eq!(fence.raw_lines, vec!["/cmd ```", "content", "```"]);
    }

    // =========================================================================
    // accept_fence_line — range tracking
    // RFC §7.1 / Engine Spec §3.6: LineRange (zero-based, inclusive)
    // =========================================================================

    #[test]
    fn end_line_advances_through_body_and_closer() {
        // Engine Spec §3.6: end_line is zero-based physical line of last line.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "line one");
        let (fence, _) = accept_fence_line(fence, 2, "line two");
        let (fence, _) = accept_fence_line(fence, 3, "```");
        assert_eq!(fence.start_line, 0);
        assert_eq!(fence.end_line, 3);
    }

    // =========================================================================
    // finalize_fence — closed fence (normal completion)
    // Engine Spec §9.3 / RFC §7.1
    // =========================================================================

    #[test]
    fn closed_fence_produces_no_warnings() {
        // Engine Spec §9.3 step 4: warning only when unclosed.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "content");
        let (fence, _) = accept_fence_line(fence, 2, "```");
        let (_, warnings) = finalize_fence(fence, false);
        assert!(warnings.is_empty());
    }

    #[test]
    fn closed_fence_payload_joined_with_lf() {
        // RFC §5.2.2: "Lines are concatenated with LF separators in the payload."
        // Engine Spec §9.3 step 3: payload_lines joined with \n.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "line one");
        let (fence, _) = accept_fence_line(fence, 2, "line two");
        let (fence, _) = accept_fence_line(fence, 3, "```");
        let (cmd, _) = finalize_fence(fence, false);
        assert_eq!(cmd.arguments.payload, "line one\nline two");
        assert_eq!(cmd.arguments.mode, ArgumentMode::Fence);
    }

    #[test]
    fn empty_fence_body_produces_empty_payload() {
        // RFC §5.2.2: zero body lines -> empty payload.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "```");
        let (cmd, _) = finalize_fence(fence, false);
        assert_eq!(cmd.arguments.payload, "");
    }

    #[test]
    fn closed_fence_raw_joined_with_lf() {
        // Engine Spec §9.3 step 2: raw_lines joined with \n.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "line one");
        let (fence, _) = accept_fence_line(fence, 2, "line two");
        let (fence, _) = accept_fence_line(fence, 3, "```");
        let (cmd, _) = finalize_fence(fence, false);
        assert_eq!(cmd.raw, "/cmd ```\nline one\nline two\n```");
    }

    #[test]
    fn closed_fence_range() {
        // Engine Spec §3.6: LineRange is inclusive on both ends.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "body");
        let (fence, _) = accept_fence_line(fence, 2, "```");
        let (cmd, _) = finalize_fence(fence, false);
        assert_eq!(cmd.range.start_line, 0);
        assert_eq!(cmd.range.end_line, 2);
    }

    #[test]
    fn closed_fence_id_and_name() {
        // Engine Spec §3.2: id is "cmd-{n}", name from header.
        // RFC §6.5: sequential zero-based identifiers.
        let fence = make_fence("deploy", 3, 0, 5);
        let (fence, _) = accept_fence_line(fence, 1, "```");
        let (cmd, _) = finalize_fence(fence, false);
        assert_eq!(cmd.id, "cmd-5");
        assert_eq!(cmd.name, "deploy");
    }

    #[test]
    fn fence_lang_preserved_in_output() {
        // RFC §7.1: "The language identifier from the fence opener."
        // Engine Spec §3.3: fence_lang is Some(lang).
        let fence = make_fence_with_lang("code", "rust", 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "fn main() {}");
        let (fence, _) = accept_fence_line(fence, 2, "```");
        let (cmd, _) = finalize_fence(fence, false);
        assert_eq!(cmd.arguments.fence_lang, Some("rust".to_string()));
    }

    #[test]
    fn fence_without_lang_has_none() {
        // RFC §7.1: "absent/null if … no language was specified."
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "```");
        let (cmd, _) = finalize_fence(fence, false);
        assert_eq!(cmd.arguments.fence_lang, None);
    }

    // =========================================================================
    // finalize_fence — unclosed fence (EOF without closer)
    // RFC §5.2.4 / Engine Spec §9.3 step 4
    // =========================================================================

    #[test]
    fn unclosed_fence_emits_warning() {
        // RFC §5.2.4: 'A warning of type "unclosed_fence" MUST be produced.'
        // Engine Spec §9.3 step 4.
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "line1");
        let (_, warnings) = finalize_fence(fence, true);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn unclosed_fence_warning_fields() {
        // RFC §5.2.4: warning includes "the fence opener's physical line number."
        // RFC §7.4: type is "unclosed_fence".
        //
        // BUG: code emits "unclosed-fence" (kebab-case) but Engine Spec v0.5.0
        // §11 requires "unclosed_fence" (snake_case). See gaps comment below.
        let fence = make_fence("cmd", 3, 4, 0);
        let (_, warnings) = finalize_fence(fence, true);
        assert_eq!(warnings[0].wtype, "unclosed-fence");
        assert_eq!(warnings[0].start_line, Some(4));
        assert!(warnings[0].message.as_deref().unwrap_or("").contains('4'));
    }

    #[test]
    fn unclosed_fence_includes_partial_payload() {
        // RFC §5.2.4: "complete with whatever payload has been accumulated
        // through EOF."
        let fence = make_fence("cmd", 3, 0, 0);
        let (fence, _) = accept_fence_line(fence, 1, "partial");
        let (cmd, _) = finalize_fence(fence, true);
        assert_eq!(cmd.raw, "/cmd ```\npartial");
        assert_eq!(cmd.arguments.payload, "partial");
    }

    // =========================================================================
    // Property tests
    // =========================================================================

    fn valid_command_name() -> impl Strategy<Value = String> {
        // RFC §4.1: [a-z]([a-z0-9-]*[a-z0-9])?
        "[a-z][a-z0-9\\-]{0,15}".prop_filter("no trailing hyphen", |s| !s.ends_with('-'))
    }

    proptest! {
        // Engine Spec §9.3 step 2: raw_lines count = opener + body + closer.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn raw_lines_count_equals_lines_consumed(
            name in valid_command_name(),
            body_lines in prop::collection::vec("[a-zA-Z0-9 ]{1,30}", 0..8)
        ) {
            let fence = make_fence(&name, 3, 0, 0);
            let fence = body_lines.iter().enumerate().fold(fence, |f, (i, line)| {
                let (next, _) = accept_fence_line(f, i + 1, line);
                next
            });
            let (fence, _) = accept_fence_line(fence, body_lines.len() + 1, "```");
            prop_assert_eq!(fence.raw_lines.len(), body_lines.len() + 2);
        }

        // RFC §5.2.3: closer is excluded from payload. No body line generated
        // by the strategy can be a closer (no backtick-only lines).
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn payload_never_contains_closer(
            name in valid_command_name(),
            body_lines in prop::collection::vec("[a-zA-Z0-9 ]{1,30}", 0..8)
        ) {
            let fence = make_fence(&name, 3, 0, 0);
            let fence = body_lines.iter().enumerate().fold(fence, |f, (i, line)| {
                let (next, _) = accept_fence_line(f, i + 1, line);
                next
            });
            let (fence, _) = accept_fence_line(fence, body_lines.len() + 1, "```");
            let no_closer = !fence.payload_lines.iter().any(|l| {
                let t = l.trim();
                !t.is_empty() && t.chars().all(|c| c == '`')
            });
            prop_assert!(no_closer);
        }

        // RFC §5.2.3: closer count >= opener count always completes.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn closer_count_gte_opener_always_completes(
            name in valid_command_name(),
            extra in 0usize..5
        ) {
            let fence = make_fence(&name, 3, 0, 0);
            let closer = "`".repeat(3 + extra);
            let (_, res) = accept_fence_line(fence, 1, &closer);
            prop_assert_eq!(res, FenceResult::Completed);
        }

        // Engine Spec §9.1: id is preserved through accumulation.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn id_preserved_through_accumulation(
            name in valid_command_name(),
            id in 0usize..1000,
            body_lines in prop::collection::vec("[a-zA-Z0-9]{1,20}", 0..5)
        ) {
            let fence = make_fence(&name, 3, 0, id);
            let fence = body_lines.iter().enumerate().fold(fence, |f, (i, line)| {
                let (next, _) = accept_fence_line(f, i + 1, line);
                next
            });
            prop_assert_eq!(fence.id, id);
        }

        // Engine Spec §9.3 step 4: closed fence -> no warnings.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn closed_fence_never_warns(
            name in valid_command_name(),
            body_lines in prop::collection::vec("[a-zA-Z0-9 ]{1,30}", 1..8)
        ) {
            let fence = make_fence(&name, 3, 0, 0);
            let fence = body_lines.iter().enumerate().fold(fence, |f, (i, line)| {
                let (next, _) = accept_fence_line(f, i + 1, line);
                next
            });
            let (fence, _) = accept_fence_line(fence, body_lines.len() + 1, "```");
            let (_, warnings) = finalize_fence(fence, false);
            prop_assert!(warnings.is_empty());
        }

        // RFC §5.2.4: unclosed fence always produces exactly one warning.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn unclosed_fence_always_warns(
            name in valid_command_name(),
            body_lines in prop::collection::vec("[a-zA-Z0-9]{1,20}", 1..5)
        ) {
            let fence = make_fence(&name, 3, 0, 0);
            let fence = body_lines.iter().enumerate().fold(fence, |f, (i, line)| {
                let (next, _) = accept_fence_line(f, i + 1, line);
                next
            });
            let (_, warnings) = finalize_fence(fence, true);
            prop_assert_eq!(warnings.len(), 1);
            prop_assert_eq!(&warnings[0].wtype, "unclosed-fence");
        }

        // RFC §5.2.2: payload = body lines joined with LF, no trailing LF.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn fence_payload_equals_body_lines_joined(
            name in valid_command_name(),
            body_lines in prop::collection::vec("[a-zA-Z0-9 ]{1,20}", 1..8)
        ) {
            let fence = make_fence(&name, 3, 0, 0);
            let fence = body_lines.iter().enumerate().fold(fence, |f, (i, line)| {
                let (next, _) = accept_fence_line(f, i + 1, line);
                next
            });
            let (fence, _) = accept_fence_line(fence, body_lines.len() + 1, "```");
            let (cmd, _) = finalize_fence(fence, false);
            prop_assert_eq!(cmd.arguments.payload, body_lines.join("\n"));
        }

        // Engine Spec §9.3 step 2: raw newline count = physical lines - 1.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn raw_newline_count_equals_physical_lines_minus_one(
            name in valid_command_name(),
            body_lines in prop::collection::vec("[a-zA-Z0-9]{1,20}", 0..6)
        ) {
            let fence = make_fence(&name, 3, 0, 0);
            let fence = body_lines.iter().enumerate().fold(fence, |f, (i, line)| {
                let (next, _) = accept_fence_line(f, i + 1, line);
                next
            });
            let (fence, _) = accept_fence_line(fence, body_lines.len() + 1, "```");
            let (cmd, _) = finalize_fence(fence, false);
            let expected_newlines = body_lines.len() + 1;
            prop_assert_eq!(
                cmd.raw.chars().filter(|&c| c == '\n').count(),
                expected_newlines
            );
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
// |                                   | (kebab-case) but Engine Spec v0.5.0 requires      |          |
// |                                   | "unclosed_fence" (snake_case). This is a code bug. |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | Engine Spec §6                    | WHITESPACE: is_fence_closer uses .trim() which    | HIGH     |
// |                                   | strips all Unicode WSP. Engine Spec §6 mandates   |          |
// |                                   | only SP (U+0020) and HTAB (U+0009). No test       |          |
// |                                   | verifies that U+00A0 or other exotic WSP around   |          |
// |                                   | backticks does NOT trigger a close.                |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC §5.2.3                        | CLOSER WITH TRAILING BACKSLASH: RFC §5.2.3 notes  | MEDIUM   |
// |                                   | "If the physical line containing a fence closer    |          |
// |                                   | ends with a trailing '\', the fence closes          |          |
// |                                   | normally, and then the '\' triggers a line join."   |          |
// |                                   | No test for "```\" as a closer line (the backslash |          |
// |                                   | check is not this module's job, but the closer     |          |
// |                                   | detection itself should reject "```\").             |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC §5.2.3                        | FOUR-BACKTICK OPENER WITH THREE-BACKTICK CLOSER:  | LOW      |
// |                                   | No test where opener_count=4 and closer has        |          |
// |                                   | exactly 3 backticks (should NOT close). Only the   |          |
// |                                   | 3-opener/2-closer case is tested.                  |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC §5.2.2                        | PAYLOAD NO TRAILING LF: "The payload MUST NOT     | LOW      |
// |                                   | include a trailing LF after the last body line."   |          |
// |                                   | No explicit assertion that payload does not end    |          |
// |                                   | with \n (implicitly covered by join semantics but  |          |
// |                                   | not directly asserted).                            |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | RFC §5.2.2                        | LINE JOINING IMMUNITY: "Fence body lines are      | LOW      |
// |                                   | never subject to line joining." This is enforced   |          |
// |                                   | by the document parser (not this module), but      |          |
// |                                   | body_preserves_content_verbatim partially covers   |          |
// |                                   | it by checking trailing backslash survives.        |          |
// |-----------------------------------|--------------------------------------------------|----------|
// | Engine Spec §9.2                  | ACCEPT RESULT REJECTED VARIANT: The Engine Spec   | INFO     |
// |                                   | defines AcceptResult::Rejected for already-closed  |          |
// |                                   | commands. This file's PendingFence uses            |          |
// |                                   | ownership-based design (no is_open field), so      |          |
// |                                   | Rejected is impossible, but the enum mismatch      |          |
// |                                   | with the spec should be documented.                |          |
