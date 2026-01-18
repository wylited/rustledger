//! Rename handler for refactoring accounts and currencies.
//!
//! Supports renaming:
//! - Account names (updates all usages in the file)
//! - Currency names (updates all usages in the file)

use lsp_types::{
    Position, PrepareRenameResponse, Range, RenameParams, TextDocumentPositionParams, TextEdit,
    WorkspaceEdit,
};
use rustledger_core::Directive;
use rustledger_parser::ParseResult;
use std::collections::HashMap;

use super::utils::{
    byte_offset_to_position, get_word_at_position, is_account_like, is_currency_like,
};

/// Handle a prepare rename request (check if rename is valid at position).
pub fn handle_prepare_rename(
    params: &TextDocumentPositionParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<PrepareRenameResponse> {
    let position = params.position;
    let line_idx = position.line as usize;

    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(line_idx)?;

    // Get the word at the cursor position
    let (word, start_col, end_col) = get_word_at_position(line, position.character as usize)?;

    // Check if it's a valid renameable symbol
    if is_account_like(&word) || is_currency_like(&word, parse_result) {
        Some(PrepareRenameResponse::Range(Range {
            start: Position::new(position.line, start_col as u32),
            end: Position::new(position.line, end_col as u32),
        }))
    } else {
        None
    }
}

/// Handle a rename request.
#[allow(clippy::mutable_key_type)] // Uri is required as key by LSP WorkspaceEdit API
pub fn handle_rename(
    params: &RenameParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<WorkspaceEdit> {
    let position = params.text_document_position.position;
    let new_name = &params.new_name;
    let uri = params.text_document_position.text_document.uri.clone();

    let line_idx = position.line as usize;
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(line_idx)?;

    // Get the word at the cursor position
    let (old_name, _, _) = get_word_at_position(line, position.character as usize)?;

    // Collect all edits
    let mut edits = Vec::new();

    if is_account_like(&old_name) {
        // Rename account
        collect_account_rename_edits(source, parse_result, &old_name, new_name, &mut edits);
    } else if is_currency_like(&old_name, parse_result) {
        // Rename currency
        collect_currency_rename_edits(source, parse_result, &old_name, new_name, &mut edits);
    }

    if edits.is_empty() {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(uri, edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

/// Collect all edits needed to rename an account.
fn collect_account_rename_edits(
    source: &str,
    parse_result: &ParseResult,
    old_name: &str,
    new_name: &str,
    edits: &mut Vec<TextEdit>,
) {
    for spanned in &parse_result.directives {
        match &spanned.value {
            Directive::Open(open) => {
                if open.account.as_ref() == old_name {
                    if let Some(edit) = find_and_create_edit(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        old_name,
                        new_name,
                    ) {
                        edits.push(edit);
                    }
                }
            }
            Directive::Close(close) => {
                if close.account.as_ref() == old_name {
                    if let Some(edit) = find_and_create_edit(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        old_name,
                        new_name,
                    ) {
                        edits.push(edit);
                    }
                }
            }
            Directive::Balance(bal) => {
                if bal.account.as_ref() == old_name {
                    if let Some(edit) = find_and_create_edit(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        old_name,
                        new_name,
                    ) {
                        edits.push(edit);
                    }
                }
            }
            Directive::Pad(pad) => {
                if pad.account.as_ref() == old_name {
                    if let Some(edit) = find_and_create_edit(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        old_name,
                        new_name,
                    ) {
                        edits.push(edit);
                    }
                }
                if pad.source_account.as_ref() == old_name {
                    if let Some(edit) = find_and_create_edit(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        old_name,
                        new_name,
                    ) {
                        edits.push(edit);
                    }
                }
            }
            Directive::Note(note) => {
                if note.account.as_ref() == old_name {
                    if let Some(edit) = find_and_create_edit(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        old_name,
                        new_name,
                    ) {
                        edits.push(edit);
                    }
                }
            }
            Directive::Document(doc) => {
                if doc.account.as_ref() == old_name {
                    if let Some(edit) = find_and_create_edit(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        old_name,
                        new_name,
                    ) {
                        edits.push(edit);
                    }
                }
            }
            Directive::Transaction(txn) => {
                for posting in &txn.postings {
                    if posting.account.as_ref() == old_name {
                        // Find the posting line and create edit
                        let directive_text = &source[spanned.span.start..spanned.span.end];
                        if let Some(edit) = find_and_create_edit(
                            source,
                            spanned.span.start,
                            spanned.span.end,
                            old_name,
                            new_name,
                        ) {
                            // Check if we already have an edit for this range
                            if !edits.iter().any(|e| e.range == edit.range) {
                                edits.push(edit);
                            }
                        }
                        // For transactions with multiple matching postings, we need all of them
                        let _ = directive_text; // suppress unused warning
                    }
                }
            }
            _ => {}
        }
    }
}

/// Collect all edits needed to rename a currency.
fn collect_currency_rename_edits(
    source: &str,
    parse_result: &ParseResult,
    old_name: &str,
    new_name: &str,
    edits: &mut Vec<TextEdit>,
) {
    for spanned in &parse_result.directives {
        let directive_text = &source[spanned.span.start..spanned.span.end];

        // Check if this directive contains the currency
        if directive_text.contains(old_name) {
            // Find all occurrences in this directive
            let (start_line, _) = byte_offset_to_position(source, spanned.span.start);

            for (line_offset, line) in directive_text.lines().enumerate() {
                let mut search_start = 0;
                while let Some(pos) = line[search_start..].find(old_name) {
                    let actual_pos = search_start + pos;

                    // Verify it's a word boundary (not part of a longer identifier)
                    let before_ok = actual_pos == 0
                        || !line
                            .chars()
                            .nth(actual_pos - 1)
                            .unwrap_or(' ')
                            .is_alphanumeric();
                    let after_ok = actual_pos + old_name.len() >= line.len()
                        || !line
                            .chars()
                            .nth(actual_pos + old_name.len())
                            .unwrap_or(' ')
                            .is_alphanumeric();

                    if before_ok && after_ok {
                        let edit_line = start_line + line_offset as u32;
                        edits.push(TextEdit {
                            range: Range {
                                start: Position::new(edit_line, actual_pos as u32),
                                end: Position::new(edit_line, (actual_pos + old_name.len()) as u32),
                            },
                            new_text: new_name.to_string(),
                        });
                    }

                    search_start = actual_pos + old_name.len();
                }
            }
        }
    }

    // Deduplicate edits by range
    edits.sort_by(|a, b| {
        a.range
            .start
            .line
            .cmp(&b.range.start.line)
            .then(a.range.start.character.cmp(&b.range.start.character))
    });
    edits.dedup_by(|a, b| a.range == b.range);
}

/// Find a string in the source and create a text edit.
fn find_and_create_edit(
    source: &str,
    start_offset: usize,
    end_offset: usize,
    old_name: &str,
    new_name: &str,
) -> Option<TextEdit> {
    let directive_text = &source[start_offset..end_offset];
    let (start_line, start_col) = byte_offset_to_position(source, start_offset);

    // Find the old name in the directive
    for (line_offset, line) in directive_text.lines().enumerate() {
        if let Some(col) = line.find(old_name) {
            let edit_line = start_line + line_offset as u32;
            let edit_col = if line_offset == 0 {
                start_col + col as u32
            } else {
                col as u32
            };

            return Some(TextEdit {
                range: Range {
                    start: Position::new(edit_line, edit_col),
                    end: Position::new(edit_line, edit_col + old_name.len() as u32),
                },
                new_text: new_name.to_string(),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse;

    #[test]
    fn test_get_word_at_position() {
        let line = "  Assets:Bank  -5.00 USD";
        let (word, start, end) = get_word_at_position(line, 5).unwrap();
        assert_eq!(word, "Assets:Bank");
        assert_eq!(start, 2);
        assert_eq!(end, 13);
    }

    #[test]
    fn test_is_account_like() {
        assert!(is_account_like("Assets:Bank"));
        assert!(is_account_like("Expenses:Food:Coffee"));
        assert!(!is_account_like("USD"));
        assert!(!is_account_like("Bank"));
    }

    #[test]
    #[allow(clippy::mutable_key_type)] // Uri in HashMap is required by LSP API
    fn test_rename_account() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-15 * "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
"#;
        let result = parse(source);
        let uri: lsp_types::Uri = "file:///test.beancount".parse().unwrap();

        let params = RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                position: Position::new(0, 16), // On "Assets:Bank"
            },
            new_name: "Assets:Checking".to_string(),
            work_done_progress_params: Default::default(),
        };

        let edit = handle_rename(&params, source, &result);
        assert!(edit.is_some());

        let edit = edit.unwrap();
        let changes = edit.changes.unwrap();
        let edits: Vec<_> = changes.values().next().unwrap().clone();

        // Should have 2 edits: one for open, one for posting
        assert_eq!(edits.len(), 2);
    }
}
