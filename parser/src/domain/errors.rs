/// Non-fatal conditions detected during parsing.
///
/// Warnings are collected in `ParseResult.warnings` rather than
/// causing the parse to fail. The parser is intentionally permissive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub wtype: String,
    pub start_line: Option<usize>,
    pub message: Option<String>,
}
