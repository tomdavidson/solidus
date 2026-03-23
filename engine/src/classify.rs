use crate::ArgumentMode;

/// Raw classification of a single logical input line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    Command(CommandHeader),
    Text,
}

/// Extracted header fields from a line that opens a slash command.
///
/// `fence_backtick_count` is the length of the backtick run that opened the
/// fence (e.g. 3 for ` ``` `, 4 for ` ```` `). It is 0 for single-line commands.
/// The state machine uses this to recognise the matching closer (RFC §5.2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandHeader {
    pub raw: String,
    pub name: String,
    pub header_text: String,
    pub mode: ArgumentMode,
    pub fence_lang: Option<String>,
    pub fence_backtick_count: usize,
}

/// Classify a single logical line as either a command header or plain text.
pub fn classify_line(line: &str) -> LineKind {
    match try_parse_command(line) {
        Some(header) => LineKind::Command(header),
        None => LineKind::Text,
    }
}

/// Find the first run of 3 or more consecutive backtick characters in `s`.
///
/// Returns `(start_byte_offset, backtick_count)`.
fn find_fence_opener(s: &str) -> Option<(usize, usize)> {
    s.as_bytes()
        .chunk_by(|a, b| a == b)
        .scan(0usize, |offset, chunk| {
            let start = *offset;
            *offset += chunk.len();
            Some((start, chunk))
        })
        .find(|(_, chunk)| chunk.first() == Some(&b'`') && chunk.len() >= 3)
        .map(|(start, chunk)| (start, chunk.len()))
}

fn try_parse_command(line: &str) -> Option<CommandHeader> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }

    let without_slash = &trimmed[1..];

    let mut parts = without_slash.splitn(2, char::is_whitespace);
    let name_raw = parts.next().filter(|n| !n.is_empty())?;

    // RFC §4.1: [a-z]([a-z0-9-]*[a-z0-9])?
    if !name_raw.starts_with(|c: char| c.is_ascii_lowercase()) {
        return None;
    }

    if !name_raw.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return None;
    }

    let name = name_raw.to_string();
    let rest = parts.next().unwrap_or("").trim_start();

    // RFC §5.2.1: detect first occurrence of 3+ backticks anywhere in the args.
    if let Some((fence_start, fence_count)) = find_fence_opener(rest) {
        let header_text = rest[..fence_start].trim_end().to_string();
        let after_ticks = &rest[fence_start + fence_count..];
        let after_trimmed = after_ticks.trim();
        let fence_lang = if !after_trimmed.is_empty() && !after_trimmed.contains(char::is_whitespace) {
            Some(after_trimmed.to_string())
        } else {
            None
        };

        return Some(CommandHeader {
            raw: line.to_string(),
            name,
            header_text,
            mode: ArgumentMode::Fence,
            fence_lang,
            fence_backtick_count: fence_count,
        });
    }

    // RFC §5.1: single-line mode — no fence opener present.
    Some(CommandHeader {
        raw: line.to_string(),
        name,
        header_text: rest.to_string(),
        mode: ArgumentMode::SingleLine,
        fence_lang: None,
        fence_backtick_count: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // find_fence_opener — internal helper
    // =========================================================================

    // --- Boundary: minimum backtick threshold ---

    #[test]
    fn find_fence_opener_below_threshold_returns_none() {
        // RFC §5.2.1: opener requires three or more consecutive backticks.
        assert!(find_fence_opener("").is_none());
        assert!(find_fence_opener("hello world").is_none());
        assert!(find_fence_opener("`").is_none());
        assert!(find_fence_opener("``").is_none());
    }

    #[test]
    fn find_fence_opener_exactly_three() {
        // RFC §5.2.1: three consecutive backticks form a valid opener.
        assert_eq!(find_fence_opener("```"), Some((0, 3)));
    }

    #[test]
    fn find_fence_opener_four_backticks() {
        // RFC §5.2.1: variable-length fence (three or more). Four ticks
        // produce count 4, not two separate runs.
        assert_eq!(find_fence_opener("````"), Some((0, 4)));
    }

    // --- Position and first-occurrence ---

    #[test]
    fn find_fence_opener_offset_after_prefix() {
        // RFC §5.2.1: "first occurrence" — offset reflects actual position.
        assert_eq!(find_fence_opener("call_tool write_file ```"), Some((21, 3)));
    }

    #[test]
    fn find_fence_opener_returns_first_run_only() {
        // RFC §5.2.1: "first occurrence" — second qualifying run is ignored.
        assert_eq!(find_fence_opener("``` and ```"), Some((0, 3)));
    }

    // --- Non-qualifying patterns ---

    #[test]
    fn find_fence_opener_non_adjacent_pairs_do_not_combine() {
        // RFC §5.2.1: two separate runs of two do not combine.
        assert!(find_fence_opener("`` ``").is_none());
    }

    #[test]
    fn find_fence_opener_tildes_rejected() {
        // RFC §5.2: "Only backtick (`) fences are recognized. Tilde (~)
        // fences MUST NOT be treated as fence openers."
        assert!(find_fence_opener("~~~").is_none());
    }

    // =========================================================================
    // try_parse_command — command name validation
    // =========================================================================

    // --- Lines that are not commands ---

    #[test]
    fn no_slash_returns_none() {
        // RFC §4.2: command line requires first non-WSP char to be "/".
        assert!(try_parse_command("hello").is_none());
    }

    #[test]
    fn bare_slash_returns_none() {
        // RFC §4.5: a bare "/" is an invalid slash line.
        assert!(try_parse_command("/").is_none());
    }

    #[test]
    fn slash_then_space_returns_none() {
        // RFC §4.5: "/ space" — no command name follows the slash.
        assert!(try_parse_command("/ ").is_none());
    }

    #[test]
    fn uppercase_start_returns_none() {
        // RFC §4.1: name must begin with LCALPHA; RFC §4.5: "/Hello" is invalid.
        assert!(try_parse_command("/Hello").is_none());
    }

    #[test]
    fn digit_start_returns_none() {
        // RFC §4.1: name must begin with LCALPHA; RFC §4.5: "/123" is invalid.
        assert!(try_parse_command("/123").is_none());
    }

    #[test]
    fn underscore_in_name_returns_none() {
        // RFC §4.1: pattern allows only [a-z0-9-], not underscores.
        assert!(try_parse_command("/cmd_foo").is_none());
    }

    #[test]
    fn trailing_hyphen_returns_none() {
        // RFC §4.1: "Multi-character names MUST NOT end with a hyphen."
        // RFC §4.5: "/cmd-" is listed as an invalid slash line example.
        // Engine Spec §8 step 3: names ending with hyphen -> Text.
        assert!(try_parse_command("/cmd-").is_none());
    }

    #[test]
    fn trailing_hyphen_with_args_returns_none() {
        // RFC §4.1: trailing hyphen prohibition applies even with args after.
        assert!(try_parse_command("/cmd- args").is_none());
    }

    // --- Valid command names ---

    #[test]
    fn single_letter_name() {
        // RFC §4.1: "A single lowercase letter is a valid command name."
        let h = try_parse_command("/x").unwrap();
        assert_eq!(h.name, "x");
    }

    #[test]
    fn hyphenated_name() {
        // RFC §4.1: hyphens allowed between alphanumerics.
        let h = try_parse_command("/call-tool args").unwrap();
        assert_eq!(h.name, "call-tool");
    }

    #[test]
    fn name_with_digits() {
        // RFC §4.1: digits allowed after the initial letter.
        let h = try_parse_command("/v2").unwrap();
        assert_eq!(h.name, "v2");
    }

    // =========================================================================
    // try_parse_command — leading whitespace and raw field
    // =========================================================================

    #[test]
    fn leading_whitespace_stripped_for_detection() {
        // RFC §4.2: "first non-whitespace character is /"
        let h = try_parse_command("   /cmd arg").unwrap();
        assert_eq!(h.name, "cmd");
    }

    #[test]
    fn raw_preserves_original_line() {
        // RFC §7.1 / Engine Spec §3.2: raw is the exact source text as it
        // appeared in the normalized input, including leading whitespace.
        let h = try_parse_command("  /cmd arg").unwrap();
        assert_eq!(h.raw, "  /cmd arg");
    }

    // =========================================================================
    // try_parse_command — single-line mode
    // =========================================================================

    #[test]
    fn no_args_single_line() {
        // RFC §4.3: "The arguments portion may be empty."
        // RFC §5.1: mode is single-line when no fence opener present.
        let h = try_parse_command("/help").unwrap();
        assert_eq!(h.name, "help");
        assert_eq!(h.header_text, "");
        assert_eq!(h.mode, ArgumentMode::SingleLine);
        assert_eq!(h.fence_lang, None);
        assert_eq!(h.fence_backtick_count, 0);
    }

    #[test]
    fn single_line_header_is_full_args() {
        // RFC §4.4: in single-line mode "header and payload contain the same
        // string (the full arguments text)".
        // RFC §4.3: separator whitespace is not included in arguments.
        let h = try_parse_command("/deploy production --region us-west-2").unwrap();
        assert_eq!(h.header_text, "production --region us-west-2");
        assert_eq!(h.mode, ArgumentMode::SingleLine);
        assert_eq!(h.fence_backtick_count, 0);
    }

    #[test]
    fn tildes_treated_as_single_line_content() {
        // RFC §5.2: tildes are not fence openers, so the line stays single-line.
        let h = try_parse_command("/cmd ~~~").unwrap();
        assert_eq!(h.mode, ArgumentMode::SingleLine);
        assert_eq!(h.header_text, "~~~");
    }

    // =========================================================================
    // try_parse_command — fence mode
    // =========================================================================

    #[test]
    fn fence_at_start_of_args_empty_header() {
        // RFC §5.2.1: "Text before the backtick run … becomes the header."
        // When backticks are first, header is empty.
        let h = try_parse_command("/cmd ```json").unwrap();
        assert_eq!(h.header_text, "");
        assert_eq!(h.mode, ArgumentMode::Fence);
        assert_eq!(h.fence_lang, Some("json".to_string()));
        assert_eq!(h.fence_backtick_count, 3);
    }

    #[test]
    fn fence_with_preceding_header() {
        // RFC §5.2.1: text before backtick run, trimmed trailing WSP, is header.
        // RFC §4.4: header is the dispatch/routing portion.
        let h = try_parse_command("/mcp call_tool write_file ```json").unwrap();
        assert_eq!(h.header_text, "call_tool write_file");
        assert_eq!(h.mode, ArgumentMode::Fence);
        assert_eq!(h.fence_lang, Some("json".to_string()));
        assert_eq!(h.fence_backtick_count, 3);
    }

    #[test]
    fn fence_without_lang() {
        // RFC §5.2.1: fence_lang is absent when nothing follows the backticks.
        let h = try_parse_command("/cmd ```").unwrap();
        assert_eq!(h.mode, ArgumentMode::Fence);
        assert_eq!(h.fence_lang, None);
    }

    #[test]
    fn fence_lang_none_when_multiple_tokens() {
        // RFC §5.2.1: "single token (no internal whitespace)" — multiple
        // tokens disqualify the language identifier.
        let h = try_parse_command("/cmd ``` foo bar").unwrap();
        assert_eq!(h.mode, ArgumentMode::Fence);
        assert_eq!(h.fence_lang, None);
    }

    #[test]
    fn fence_four_backticks() {
        // RFC §5.2.1: variable-length fence. Count must match the run length
        // so the closer check (RFC §5.2.3) can match correctly.
        let h = try_parse_command("/cmd ````json").unwrap();
        assert_eq!(h.fence_backtick_count, 4);
        assert_eq!(h.mode, ArgumentMode::Fence);
    }

    #[test]
    fn fence_opener_mid_args() {
        // RFC §5.2.1: "first occurrence of three or more consecutive backtick
        // characters" — opener does not need to be at the start of args.
        let h = try_parse_command("/mcp call_tool write_file -c```json").unwrap();
        assert_eq!(h.header_text, "call_tool write_file -c");
        assert_eq!(h.mode, ArgumentMode::Fence);
        assert_eq!(h.fence_lang, Some("json".to_string()));
    }

    // =========================================================================
    // classify_line — public API (composition of try_parse_command)
    // =========================================================================

    #[test]
    fn text_line_produces_text() {
        // RFC §4.2/§6.3: line whose first non-WSP is not "/" -> text.
        assert_eq!(classify_line("hello world"), LineKind::Text);
    }

    #[test]
    fn valid_command_produces_command() {
        // RFC §4.2: valid command line -> LineKind::Command.
        assert!(matches!(classify_line("/cmd args"), LineKind::Command(_)));
    }

    #[test]
    fn invalid_slash_lines_produce_text() {
        // RFC §4.5: invalid slash lines are text. Engine Spec §8 step 3.
        assert_eq!(classify_line("/Hello"), LineKind::Text);
        assert_eq!(classify_line("/123"), LineKind::Text);
        assert_eq!(classify_line("/"), LineKind::Text);
        assert_eq!(classify_line("/ "), LineKind::Text);
        assert_eq!(classify_line("/cmd-"), LineKind::Text);
    }

    // =========================================================================
    // Property tests
    // =========================================================================

    use proptest::prelude::*;

    // RFC §4.1: [a-z]([a-z0-9-]*[a-z0-9])?
    fn valid_command_name() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9\\-]{0,20}".prop_map(|s| s)
    }

    fn arbitrary_line() -> impl Strategy<Value = String> {
        prop_oneof![
            "[a-zA-Z0-9 !.,]{0,80}",
            valid_command_name().prop_flat_map(|name| {
                "[a-zA-Z0-9 ]{0,40}".prop_map(move |args| format!("/{name} {args}"))
            }),
            valid_command_name().prop_flat_map(|name| {
                (1usize..5, "[a-zA-Z0-9 ]{0,40}")
                    .prop_map(move |(spaces, args)| format!("{}/{} {}", " ".repeat(spaces), name, args))
            }),
        ]
    }

    proptest! {
        // RFC §8.2 item 4: parser is a total function. classify_line must
        // never panic on any input.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn classify_never_panics(line in "[\\x00-\\x7F]{0,200}") {
            let _ = classify_line(&line);
        }

        // RFC §4.2 + §4.1: "/" + valid name -> always Command.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn valid_name_always_produces_command(
            name in valid_command_name(),
            args in "[a-z0-9 ]{0,40}"
        ) {
            let input = format!("/{name} {args}");
            match classify_line(&input) {
                LineKind::Command(h) => prop_assert_eq!(h.name, name),
                LineKind::Text => panic!("expected Command for input: {input}"),
            }
        }

        // RFC §4.2: first non-WSP must be "/" for a command.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn text_without_slash_is_never_command(line in "[a-zA-Z0-9 !.,]{1,80}") {
            prop_assert!(matches!(classify_line(&line), LineKind::Text));
        }

        // RFC §7.1 / Engine Spec §3.2: raw preserves original input.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn raw_preserves_original_input(line in arbitrary_line()) {
            if let LineKind::Command(h) = classify_line(&line) {
                prop_assert_eq!(h.raw, line);
            }
        }

        // RFC §5.2.1: 3+ backticks in args -> fence mode.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn fence_mode_iff_three_or_more_backticks(
            name in valid_command_name(),
            lang in "[a-z]{0,10}"
        ) {
            let input = format!("/{name} ```{lang}");
            match classify_line(&input) {
                LineKind::Command(h) => prop_assert_eq!(h.mode, ArgumentMode::Fence),
                _ => panic!("expected Command"),
            }
        }

        // RFC §5.1: single-line -> fence_backtick_count == 0.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn single_line_fence_count_is_zero(
            name in valid_command_name(),
            args in "[a-zA-Z0-9 ]{0,40}"
        ) {
            let input = format!("/{name} {args}");
            let LineKind::Command(h) = classify_line(&input) else { return Ok(()); };
            prop_assert_eq!(h.mode, ArgumentMode::SingleLine);
            prop_assert_eq!(h.fence_backtick_count, 0);
        }

        // RFC §5.2.1: backtick run length is recorded as fence marker length.
        // RFC §5.2.3: closer must have >= this many backticks.
        #[test]
        #[cfg_attr(feature = "tdd", ignore)]
        fn fence_backtick_count_matches_opener_length(
            name in valid_command_name(),
            extra in 0usize..5
        ) {
            let ticks = "`".repeat(3 + extra);
            let input = format!("/{name} {ticks}json");
            if let LineKind::Command(h) = classify_line(&input) {
                prop_assert_eq!(h.fence_backtick_count, 3 + extra);
            }
        }
    }
}

/*
| Spec Section                 | Gap                                                                                                                                                                                                                                                                                                         |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| RFC §4.1 (ABNF command-name) | The code does not reject trailing hyphens. The regex in try_parse_command still uses the old v0.3.1 pattern [a-z][a-z0-9-]*. The new tests I added (trailing_hyphen_returns_none) will FAIL until the implementation is fixed to match [a-z]([a-z0-9-]*[a-z0-9])?. This is a code bug, not just a test gap. |
| RFC §4.2                     | Tab (HTAB) as leading whitespace. The code uses trim_start() which trims all Unicode whitespace. Engine Spec §6 requires only SP and HTAB. No test uses \\t/cmd to verify tab handling, and no test checks that exotic Unicode whitespace (e.g. U+00A0) is NOT stripped.                                    |
| RFC §4.3                     | Whitespace separator between name and args uses char::is_whitespace (splits on any Unicode WSP). Engine Spec §6 mandates only SP/HTAB. No test verifies that a non-breaking space between name and args is handled correctly.                                                                               |
| RFC §5.2.1                   | No test for fence opener with exactly the backticks touching the header text with no space (e.g., /cmd header```json where header runs directly into backticks). The mid-args test uses -c```json but not a pure alpha header.                                                                              |
| RFC §5.2.1                   | No test for lang-id that contains digits or hyphens (e.g., utf-8, es2024).                                                                                                                                                                                                                                  |
| Engine Spec §6               | The is_wsp() helper mandate is not followed in the implementation. trim_start(), char::is_whitespace, and trim() are used throughout. This is a systemic code issue.                                                                                                                                        |
| RFC §4.5                     | No test for special characters after slash (e.g., /foo!bar, /@cmd).                                                                                                                                                                                                                                         |*/
