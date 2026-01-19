//! Range formatting handler for formatting selections.
//!
//! Formats only the selected range of the document.

use lsp_types::{DocumentRangeFormattingParams, Position, Range, TextEdit};
use rustledger_core::Directive;
use rustledger_parser::ParseResult;

use super::utils::byte_offset_to_position;

/// Handle a range formatting request.
pub fn handle_range_formatting(
    params: &DocumentRangeFormattingParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<Vec<TextEdit>> {
    let range = params.range;
    let mut edits = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    // Process lines within the range
    for line_num in range.start.line..=range.end.line {
        if let Some(line) = lines.get(line_num as usize) {
            // Fix tabs to spaces
            if line.contains('\t') {
                let new_line = line.replace('\t', "  ");
                if new_line != *line {
                    edits.push(TextEdit {
                        range: Range {
                            start: Position::new(line_num, 0),
                            end: Position::new(line_num, line.len() as u32),
                        },
                        new_text: new_line,
                    });
                    continue; // Skip other edits for this line
                }
            }

            // Trim trailing whitespace
            let trimmed = line.trim_end();
            if trimmed.len() < line.len() {
                edits.push(TextEdit {
                    range: Range {
                        start: Position::new(line_num, trimmed.len() as u32),
                        end: Position::new(line_num, line.len() as u32),
                    },
                    new_text: String::new(),
                });
            }
        }
    }

    // Format postings within transactions in the range
    for spanned in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned.value {
            let (start_line, _) = byte_offset_to_position(source, spanned.span.start);

            // Check if transaction overlaps with range
            if start_line > range.end.line {
                continue;
            }

            for (i, posting) in txn.postings.iter().enumerate() {
                let posting_line = start_line + 1 + i as u32;

                // Check if posting is within range
                if posting_line >= range.start.line && posting_line <= range.end.line {
                    if let Some(line) = lines.get(posting_line as usize) {
                        if let Some(edit) = format_posting_line(line, posting_line, posting) {
                            // Don't duplicate edits
                            if !edits.iter().any(|e| e.range.start.line == posting_line) {
                                edits.push(edit);
                            }
                        }
                    }
                }
            }
        }
    }

    // Sort and deduplicate
    edits.sort_by(|a, b| {
        a.range
            .start
            .line
            .cmp(&b.range.start.line)
            .then(a.range.start.character.cmp(&b.range.start.character))
    });
    edits.dedup_by(|a, b| a.range == b.range);

    if edits.is_empty() { None } else { Some(edits) }
}

/// Format a posting line for alignment.
fn format_posting_line(
    line: &str,
    line_num: u32,
    posting: &rustledger_core::Posting,
) -> Option<TextEdit> {
    let trimmed = line.trim();

    // Skip if empty or comment
    if trimmed.is_empty() || trimmed.starts_with(';') {
        return None;
    }

    let account = posting.account.to_string();
    let current_indent = line.len() - line.trim_start().len();
    let expected_indent = 2;

    // Only fix indentation issues
    if current_indent != expected_indent {
        let mut formatted = String::new();
        formatted.push_str(&" ".repeat(expected_indent));
        formatted.push_str(trimmed);

        return Some(TextEdit {
            range: Range {
                start: Position::new(line_num, 0),
                end: Position::new(line_num, line.len() as u32),
            },
            new_text: formatted,
        });
    }

    let _ = account; // suppress unused warning
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse;

    #[test]
    fn test_range_formatting() {
        let source = "2024-01-01 open Assets:Bank USD   \n";
        let result = parse(source);
        let params = DocumentRangeFormattingParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 35),
            },
            options: Default::default(),
            work_done_progress_params: Default::default(),
        };

        let edits = handle_range_formatting(&params, source, &result);
        assert!(edits.is_some());
    }
}
