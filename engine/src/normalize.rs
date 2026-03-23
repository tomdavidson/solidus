/// Normalize line endings in `input` to LF only.
///
/// Step 1: replace all CRLF (`\r\n`) with LF (`\n`).
/// Step 2: replace any remaining bare CR (`\r`) with LF (`\n`).
///
/// All other bytes, including literal `\n` escape sequences inside content,
/// are preserved verbatim.
pub fn normalize(input: &str) -> String { input.replace("\r\n", "\n").replace('\r', "\n") }

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    // =========================================================================
    // No-op cases (input already normalized or empty)
    // RFC §3.1 / Engine Spec §5.1
    // =========================================================================

    #[test]
    fn empty_input_is_unchanged() {
        // RFC §3.1 (implied): normalize is a pure function; empty -> empty.
        // Engine Spec §5.1: "pure string transformation with no state."
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn lf_only_input_is_unchanged() {
        // RFC §3.1: LF is the normalized form; LF-only input needs no changes.
        let input = "line one\nline two\nline three";
        assert_eq!(normalize(input), input);
    }

    #[test]
    fn no_line_endings_unchanged() {
        // RFC §3.1: content with no CR or LF is unaffected.
        let input = "no line endings here";
        assert_eq!(normalize(input), input);
    }

    // =========================================================================
    // CRLF replacement (step 1)
    // RFC §3.1 step 1: "Replace all CRLF (U+000D U+000A) sequences with LF."
    // Engine Spec §5.1 step 1.
    // =========================================================================

    #[test]
    fn crlf_becomes_lf() {
        // RFC §3.1 step 1.
        assert_eq!(normalize("a\r\nb"), "a\nb");
    }

    #[test]
    fn multiple_crlf_all_converted() {
        // RFC §3.1 step 1: all occurrences replaced, not just the first.
        assert_eq!(normalize("a\r\nb\r\nc"), "a\nb\nc");
    }

    #[test]
    fn crlf_at_boundaries() {
        // RFC §3.1 step 1: replacement applies at start and end of input.
        assert_eq!(normalize("\r\nhello\r\n"), "\nhello\n");
    }

    // =========================================================================
    // Bare CR replacement (step 2)
    // RFC §3.1 step 2: "Replace all remaining bare CR (U+000D) with LF."
    // Engine Spec §5.1 step 2.
    // =========================================================================

    #[test]
    fn bare_cr_becomes_lf() {
        // RFC §3.1 step 2.
        assert_eq!(normalize("a\rb"), "a\nb");
    }

    #[test]
    fn consecutive_bare_cr() {
        // RFC §3.1 step 2: each bare CR individually becomes LF.
        assert_eq!(normalize("\r\r\r"), "\n\n\n");
    }

    // =========================================================================
    // Ordering: CRLF before bare CR (steps 1 then 2)
    // RFC §3.1: step 1 runs before step 2 to avoid double-conversion.
    // =========================================================================

    #[test]
    fn mixed_endings_no_double_conversion() {
        // RFC §3.1 steps 1-2: CRLF replaced first, so the CR in \r\n is not
        // re-matched as bare CR (which would produce \n\n).
        assert_eq!(normalize("a\r\nb\rc\nd"), "a\nb\nc\nd");
    }

    // =========================================================================
    // Literal escape sequences preserved
    // RFC §3.1: "Literal escape sequences inside content (e.g., the
    // two-character sequence '\' 'n') are ordinary characters."
    // =========================================================================

    #[test]
    fn literal_backslash_n_preserved() {
        // RFC §3.1: two-char sequence backslash + 'n' is not a line terminator.
        let input = "before\\nafter";
        assert_eq!(normalize(input), "before\\nafter");
    }

    // =========================================================================
    // Property tests
    // =========================================================================

    proptest! {
        // RFC §3.1: "After normalization, all line terminators are LF."
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn output_never_contains_cr(input in "[\\x00-\\x7F]{0,500}") {
            let result = normalize(&input);
            prop_assert!(!result.contains('\r'));
        }

        // RFC §3.1 (implied): normalizing already-normalized output is a no-op.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn idempotent(input in "[\\x00-\\x7F]{0,500}") {
            let once = normalize(&input);
            let twice = normalize(&once);
            prop_assert_eq!(once, twice);
        }

        // RFC §3.1: input containing no CR requires no changes.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn clean_input_is_unchanged(input in "[\\x20-\\x7E\\n]{0,500}") {
            prop_assert_eq!(normalize(&input), input);
        }

        // RFC §3.1 step 2: each bare CR becomes LF, so LF count can only
        // stay the same or increase.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn lf_count_gte_original_lf_count(input in "[\\x00-\\x7F]{0,500}") {
            let original_lf = input.chars().filter(|&c| c == '\n').count();
            let result = normalize(&input);
            let result_lf = result.chars().filter(|&c| c == '\n').count();
            prop_assert!(result_lf >= original_lf);
        }
    }
}

// =============================================================================
// TEST GAPS: spec areas this file's functions touch but are not tested
// =============================================================================
//
// | Spec Section                    | Gap                                             | Severity |
// |---------------------------------|-------------------------------------------------|----------|
// | RFC §3.1                        | SPLIT_LINES: The spec says "The normalized      | HIGH     |
// |                                 | input is split on LF to produce a sequence of   |          |
// |                                 | physical lines." Engine Spec §5.2 defines this  |          |
// |                                 | as a separate stage. This module only covers     |          |
// |                                 | normalization, not splitting. If split_lines     |          |
// |                                 | lives elsewhere, that's fine, but if it's        |          |
// |                                 | missing entirely it's a gap.                     |          |
// |---------------------------------|-------------------------------------------------|----------|
// | RFC §3.1                        | TRAILING LF: "A trailing LF at the end of       | MEDIUM   |
// |                                 | input produces a trailing empty line." No test   |          |
// |                                 | verifies that normalize preserves a trailing     |          |
// |                                 | \n (e.g., "hello\n" -> "hello\n"). This is       |          |
// |                                 | trivially true for LF input but should be        |          |
// |                                 | verified for trailing \r and trailing \r\n.      |          |
// |---------------------------------|-------------------------------------------------|----------|
// | RFC §3.1                        | UNICODE / NON-ASCII: All property tests use      | LOW      |
// |                                 | ASCII [\x00-\x7F]. No test verifies that         |          |
// |                                 | multi-byte UTF-8 content (e.g., emoji, CJK)     |          |
// |                                 | passes through normalization unmodified. The     |          |
// |                                 | implementation (.replace) handles this            |          |
// |                                 | correctly, but a test would guard regressions.   |          |
// |---------------------------------|-------------------------------------------------|----------|
// | RFC §3.1                        | LENGTH INVARIANT: normalize can only shrink      | LOW      |
// |                                 | (CRLF -> LF removes one byte) or preserve       |          |
// |                                 | length. No property test asserts                 |          |
// |                                 | result.len() <= input.len().                     |          |
// |---------------------------------|-------------------------------------------------|----------|
// | Engine Spec §5.1                | PURE FUNCTION: "This stage is a pure string     | INFO     |
// |                                 | transformation with no state." No test verifies  |          |
// |                                 | that calling normalize multiple times on         |          |
// |                                 | different inputs doesn't leak state (trivially   |          |
// |                                 | true for a free function, but documenting the    |          |
// |                                 | intent is useful).                               |          |
