use crate::{ArgumentMode, Command, CommandArguments, LineRange, classify::CommandHeader};

/// Finalize a single-line command. No accumulation needed.
/// Extracted from the single-line paths of start_command + finalize_command.
pub fn finalize_single_line(header: CommandHeader, raw: String, id: usize, range: LineRange) -> Command {
    let payload = header.header_text.clone();
    Command {
        id: format!("cmd-{}", id),
        name: header.name,
        raw,
        range,
        arguments: CommandArguments {
            header: header.header_text,
            mode: ArgumentMode::SingleLine,
            fence_lang: None,
            payload,
        },
    }
}

#[cfg(test)]
mod tests {

    use crate::{ArgumentMode, LineRange, classify::CommandHeader, single_line::finalize_single_line};

    fn make_header(name: &str, header_text: &str) -> CommandHeader {
        CommandHeader {
            raw: format!("/{name} {header_text}"),
            name: name.to_string(),
            header_text: header_text.to_string(),
            mode: ArgumentMode::SingleLine,
            fence_lang: None,
            fence_backtick_count: 0,
        }
    }

    // =========================================================================
    // Output field mapping
    // Engine Spec §9.3 / RFC §7.1 / RFC §6.5
    // =========================================================================

    #[test]
    fn id_and_name() {
        // RFC §6.5: sequential zero-based id "cmd-{n}".
        // Engine Spec §3.2: name from header.
        let h = make_header("deploy", "prod");
        let cmd = finalize_single_line(h, "/deploy prod".into(), 5, LineRange { start_line: 0, end_line: 0 });
        assert_eq!(cmd.id, "cmd-5");
        assert_eq!(cmd.name, "deploy");
    }

    #[test]
    fn raw_and_range_passed_through() {
        // RFC §7.1: raw source is "the exact source text from the normalized
        // input (before line joining)."
        // Engine Spec §3.6: LineRange inclusive on both ends.
        let h = make_header("cmd", "arg");
        let range = LineRange { start_line: 3, end_line: 5 };
        let cmd = finalize_single_line(h, "/cmd arg".into(), 0, range);
        assert_eq!(cmd.raw, "/cmd arg");
        assert_eq!(cmd.range.start_line, 3);
        assert_eq!(cmd.range.end_line, 5);
    }

    // =========================================================================
    // Single-line mode: header == payload
    // RFC §4.4 / RFC §5.1 / Engine Spec §3.3
    // =========================================================================

    #[test]
    fn payload_equals_header() {
        // RFC §4.4: "In single-line mode, the header and payload are identical
        // (the full arguments text)."
        // RFC §5.1: mode is "single-line", no fence language.
        let h = make_header("deploy", "production --region us-west-2");
        let cmd = finalize_single_line(
            h,
            "/deploy production --region us-west-2".into(),
            0,
            LineRange { start_line: 0, end_line: 0 },
        );
        assert_eq!(cmd.arguments.payload, "production --region us-west-2");
        assert_eq!(cmd.arguments.header, "production --region us-west-2");
        assert_eq!(cmd.arguments.mode, ArgumentMode::SingleLine);
        assert_eq!(cmd.arguments.fence_lang, None);
    }

    #[test]
    fn empty_args() {
        // RFC §4.3: "The arguments portion may be empty (command with no arguments)."
        // RFC §5.1: empty args -> header and payload are both empty strings.
        let h = make_header("ping", "");
        let cmd = finalize_single_line(h, "/ping".into(), 0, LineRange { start_line: 0, end_line: 0 });
        assert_eq!(cmd.arguments.payload, "");
        assert_eq!(cmd.arguments.header, "");
        assert_eq!(cmd.arguments.mode, ArgumentMode::SingleLine);
        assert_eq!(cmd.arguments.fence_lang, None);
    }
}

// =============================================================================
// TEST GAPS: spec areas this file's functions touch but are not tested
// =============================================================================
//
// | Spec Section                    | Gap                                             | Severity |
// |---------------------------------|-------------------------------------------------|----------|
// | Engine Spec §9.3                | NO WARNINGS VECTOR: finalize_single_line returns| LOW      |
// |                                 | a Command, not (Command, Vec<Warning>). The     |          |
// |                                 | fence path returns warnings but single-line      |          |
// |                                 | cannot produce warnings per spec, so this is     |          |
// |                                 | correct. However the asymmetric return types     |          |
// |                                 | between finalize_single_line and finalize_fence  |          |
// |                                 | should be documented.                            |          |
// |---------------------------------|-------------------------------------------------|----------|
// | RFC §7.1                        | RAW FOR JOINED COMMANDS: When a single-line     | MEDIUM   |
// |                                 | command spans multiple physical lines via         |          |
// |                                 | backslash joining (RFC Appendix B.2), the raw    |          |
// |                                 | field should contain all physical lines with      |          |
// |                                 | backslashes and LF separators. This function     |          |
// |                                 | accepts raw as a parameter (the caller builds    |          |
// |                                 | it), so it's not this function's concern, but    |          |
// |                                 | no test verifies multi-physical-line raw input.  |          |
// |---------------------------------|-------------------------------------------------|----------|
// | RFC §7.1                        | RAW WITH LEADING WHITESPACE: No test verifies   | LOW      |
// |                                 | that raw containing leading whitespace (e.g.,    |          |
// |                                 | "  /cmd arg") is passed through unmodified.      |          |
// |---------------------------------|-------------------------------------------------|----------|
// | Engine Spec §3.4                | ARGUMENT MODE SERIALIZATION: Engine Spec says   | INFO     |
// |                                 | "String serialization is the SDK's               |          |
// |                                 | responsibility." The enum value                  |          |
// |                                 | ArgumentMode::SingleLine is set correctly, but   |          |
// |                                 | no test confirms the enum variant name maps to   |          |
// |                                 | "single-line" (SDK concern, not engine).         |          |
// |---------------------------------|-------------------------------------------------|----------|
// | (none)                          | NO PROPERTY TESTS: This is a simple mapping      | LOW      |
// |                                 | function with no branching logic. Property tests  |          |
// |                                 | would add minimal value, but one could assert    |          |
// |                                 | payload == header for all inputs.                |          |
