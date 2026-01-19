//! Document formatting handler for Beancount files.
//!
//! Provides formatting for:
//! - Consistent indentation (2 spaces for postings)
//! - Aligned amounts in transactions
//! - Consistent spacing around operators

use lsp_types::{DocumentFormattingParams, Position, Range, TextEdit};
use rustledger_core::Directive;
use rustledger_parser::ParseResult;

use super::utils::byte_offset_to_position;

/// Default column for amount alignment.
const AMOUNT_COLUMN: usize = 50;

/// Handle a document formatting request.
pub fn handle_formatting(
    _params: &DocumentFormattingParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<Vec<TextEdit>> {
    let mut edits = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for spanned in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned.value {
            let (start_line, _) = byte_offset_to_position(source, spanned.span.start);

            // Format each posting
            for (i, posting) in txn.postings.iter().enumerate() {
                let posting_line = start_line + 1 + i as u32;

                if let Some(line) = lines.get(posting_line as usize) {
                    if let Some(edit) = format_posting_line(line, posting_line, posting) {
                        edits.push(edit);
                    }
                }
            }
        }
    }

    // Also format standalone lines (non-directive lines that might need cleanup)
    for (line_num, line) in lines.iter().enumerate() {
        // Fix tabs to spaces
        if line.contains('\t') {
            let new_line = line.replace('\t', "  ");
            if new_line != *line {
                edits.push(TextEdit {
                    range: Range {
                        start: Position::new(line_num as u32, 0),
                        end: Position::new(line_num as u32, line.len() as u32),
                    },
                    new_text: new_line,
                });
            }
        }

        // Trim trailing whitespace
        let trimmed = line.trim_end();
        if trimmed.len() < line.len() {
            edits.push(TextEdit {
                range: Range {
                    start: Position::new(line_num as u32, trimmed.len() as u32),
                    end: Position::new(line_num as u32, line.len() as u32),
                },
                new_text: String::new(),
            });
        }
    }

    // Remove duplicate edits and sort
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

    // Parse the line to find account and amount positions
    let account = posting.account.to_string();

    // Check if line starts with proper indentation
    let current_indent = line.len() - line.trim_start().len();
    let expected_indent = 2;

    // Build the formatted line
    let mut formatted = String::new();

    // Add indentation
    formatted.push_str(&" ".repeat(expected_indent));

    // Add account
    formatted.push_str(&account);

    // Add amount if present
    if let Some(ref units) = posting.units {
        if let (Some(num), Some(curr)) = (units.number(), units.currency()) {
            let num_str = num.to_string();
            let curr_str = curr.to_string();
            let amount_str = format!("{} {}", num_str, curr_str);

            // Calculate padding to align amount at AMOUNT_COLUMN
            let current_len = expected_indent + account.len();
            let padding = if current_len < AMOUNT_COLUMN - amount_str.len() {
                AMOUNT_COLUMN - amount_str.len() - current_len
            } else {
                2 // Minimum 2 spaces
            };

            formatted.push_str(&" ".repeat(padding));
            formatted.push_str(&amount_str);
        }
    }

    // Check if formatting changed anything significant
    let line_trimmed_end = line.trim_end();
    if formatted.trim_end() != line_trimmed_end
        && (current_indent != expected_indent || needs_alignment(line, &formatted))
    {
        Some(TextEdit {
            range: Range {
                start: Position::new(line_num, 0),
                end: Position::new(line_num, line.len() as u32),
            },
            new_text: formatted,
        })
    } else {
        None
    }
}

/// Check if line needs amount alignment.
fn needs_alignment(original: &str, formatted: &str) -> bool {
    // Simple heuristic: if the formatted version has different spacing, align
    let orig_parts: Vec<&str> = original.split_whitespace().collect();
    let fmt_parts: Vec<&str> = formatted.split_whitespace().collect();

    // If content is the same but spacing is different, we need alignment
    orig_parts == fmt_parts && original.trim() != formatted.trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse;

    #[test]
    fn test_formatting_removes_trailing_whitespace() {
        let source = "2024-01-01 open Assets:Bank USD   \n";
        let result = parse(source);
        let params = DocumentFormattingParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            options: Default::default(),
            work_done_progress_params: Default::default(),
        };

        let edits = handle_formatting(&params, source, &result);
        assert!(edits.is_some());
    }

    #[test]
    fn test_formatting_converts_tabs() {
        let source = "2024-01-01 * \"Test\"\n\tAssets:Bank\n";
        let result = parse(source);
        let params = DocumentFormattingParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            options: Default::default(),
            work_done_progress_params: Default::default(),
        };

        let edits = handle_formatting(&params, source, &result);
        assert!(edits.is_some());

        let edits = edits.unwrap();
        // Should have edit to replace tab
        assert!(edits.iter().any(|e| e.new_text.contains("  ")));
    }
}
