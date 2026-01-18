//! Diagnostics handler for publishing parse errors.

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use rustledger_parser::{ParseError, ParseResult};

use super::utils::LineIndex;

/// Convert parse errors to LSP diagnostics.
pub fn parse_errors_to_diagnostics(result: &ParseResult, source: &str) -> Vec<Diagnostic> {
    let line_index = LineIndex::new(source);
    result
        .errors
        .iter()
        .map(|e| parse_error_to_diagnostic(e, &line_index))
        .collect()
}

/// Convert a single parse error to an LSP diagnostic.
pub fn parse_error_to_diagnostic(error: &ParseError, line_index: &LineIndex) -> Diagnostic {
    let (start_line, start_col) = line_index.offset_to_position(error.span.start);
    let (end_line, end_col) = line_index.offset_to_position(error.span.end);

    Diagnostic {
        range: Range {
            start: Position::new(start_line, start_col),
            end: Position::new(end_line, end_col),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String(format!(
            "P{:04}",
            error.kind_code()
        ))),
        source: Some("rustledger".to_string()),
        message: error.message(),
        related_information: None,
        tags: None,
        code_description: None,
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_index_offset_to_position() {
        let source = "line1\nline2\nline3";
        let line_index = LineIndex::new(source);

        assert_eq!(line_index.offset_to_position(0), (0, 0));
        assert_eq!(line_index.offset_to_position(5), (0, 5));
        assert_eq!(line_index.offset_to_position(6), (1, 0));
        assert_eq!(line_index.offset_to_position(12), (2, 0));
    }
}
