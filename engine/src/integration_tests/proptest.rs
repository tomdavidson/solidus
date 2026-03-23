use proptest::prelude::*;

use crate::{
    domain::{ArgumentMode, LineRange, SPEC_VERSION},
    engine::{
        classify::{LineKind, classify_line},
        fence::{accept_fence_line, finalize_fence, open_fence},
        text::{append_text, finalize_text, start_text},
    },
};

fn valid_command_name() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9\\-]{0,15}".prop_filter("no trailing hyphen", |s| !s.ends_with('-'))
}

proptest! {
    #[test]
    #[cfg_attr(feature = "tdd", ignore)]
    fn classify_open_accept_finalize_roundtrip(
        name in valid_command_name(),
        body_lines in prop::collection::vec("[a-zA-Z0-9]{1,30}", 0..8)
    ) {
        let input = format!("/{name} ```");
        let header = match classify_line(&input) {
            LineKind::Command(h) => h,
            LineKind::Text => panic!("expected command"),
        };
        let raw = header.raw.clone();
        let range = LineRange { start_line: 0, end_line: 0 };
        let fence = open_fence(header, raw, 0, range);
        let fence = body_lines.iter().enumerate().fold(fence, |f, (i, line)| {
            let (next, _) = accept_fence_line(f, i + 1, line);
            next
        });
        let (fence, _) = accept_fence_line(fence, body_lines.len() + 1, "```");
        let (cmd, warnings) = finalize_fence(fence, false);
        prop_assert_eq!(cmd.name, name);
        prop_assert_eq!(cmd.arguments.mode, ArgumentMode::Fence);
        prop_assert!(warnings.is_empty());
    }

    #[test]
    #[cfg_attr(feature = "tdd", ignore)]
    fn text_lines_through_classify_and_collect(
        lines in prop::collection::vec("[a-zA-Z0-9 !.,]{1,40}", 1..15)
    ) {
        for line in &lines {
            prop_assert!(matches!(classify_line(line), LineKind::Text));
        }
        let pending = lines.iter().enumerate().skip(1).fold(
            start_text(0, &lines[0]),
            |text, (i, line)| append_text(text, i, line),
        );
        let block = finalize_text(pending, 0);
        prop_assert_eq!(block.content, lines.join("\n"));
    }
}
