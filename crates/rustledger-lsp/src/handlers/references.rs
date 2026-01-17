//! Find references handler for locating all usages.
//!
//! Provides references for:
//! - Account names (all usages across directives)
//! - Currency names (all usages across directives)
//! - Payees (all transactions with same payee)

use lsp_types::{Location, Position, Range, ReferenceParams, Uri};
use rustledger_core::Directive;
use rustledger_parser::ParseResult;

/// Handle a find references request.
pub fn handle_references(
    params: &ReferenceParams,
    source: &str,
    parse_result: &ParseResult,
    uri: &Uri,
) -> Option<Vec<Location>> {
    let position = params.text_document_position.position;
    let include_declaration = params.context.include_declaration;

    let line_idx = position.line as usize;
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(line_idx)?;

    // Get the word at the cursor position
    let (word, _, _) = get_word_at_position(line, position.character as usize)?;

    let mut locations = Vec::new();

    // Check if it's an account
    if is_account_like(&word) {
        collect_account_references(
            source,
            parse_result,
            &word,
            uri,
            include_declaration,
            &mut locations,
        );
    }
    // Check if it's a currency
    else if is_currency_like(&word, parse_result) {
        collect_currency_references(
            source,
            parse_result,
            &word,
            uri,
            include_declaration,
            &mut locations,
        );
    }
    // Check if it's a payee (inside quotes on a transaction line)
    else if is_in_quotes(line, position.character as usize) {
        collect_payee_references(source, parse_result, &word, uri, &mut locations);
    }

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

/// Collect all references to an account.
fn collect_account_references(
    source: &str,
    parse_result: &ParseResult,
    account: &str,
    uri: &Uri,
    include_declaration: bool,
    locations: &mut Vec<Location>,
) {
    for spanned in &parse_result.directives {
        let (start_line, _) = byte_offset_to_position(source, spanned.span.start);

        match &spanned.value {
            Directive::Open(open) => {
                if open.account.as_ref() == account && include_declaration {
                    if let Some(loc) = find_in_directive(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        account,
                        uri,
                    ) {
                        locations.push(loc);
                    }
                }
            }
            Directive::Close(close) => {
                if close.account.as_ref() == account {
                    if let Some(loc) = find_in_directive(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        account,
                        uri,
                    ) {
                        locations.push(loc);
                    }
                }
            }
            Directive::Balance(bal) => {
                if bal.account.as_ref() == account {
                    if let Some(loc) = find_in_directive(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        account,
                        uri,
                    ) {
                        locations.push(loc);
                    }
                }
            }
            Directive::Pad(pad) => {
                if pad.account.as_ref() == account {
                    if let Some(loc) = find_in_directive(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        account,
                        uri,
                    ) {
                        locations.push(loc);
                    }
                }
                if pad.source_account.as_ref() == account {
                    // Find the second account mention
                    let directive_text = &source[spanned.span.start..spanned.span.end];
                    if let Some(first_pos) = directive_text.find(account) {
                        let after_first = first_pos + account.len();
                        if let Some(second_pos) = directive_text[after_first..].find(account) {
                            let actual_pos = after_first + second_pos;
                            let (line, _) = byte_offset_to_position(source, spanned.span.start);
                            locations.push(Location {
                                uri: uri.clone(),
                                range: Range {
                                    start: Position::new(line, actual_pos as u32),
                                    end: Position::new(line, (actual_pos + account.len()) as u32),
                                },
                            });
                        }
                    }
                }
            }
            Directive::Note(note) => {
                if note.account.as_ref() == account {
                    if let Some(loc) = find_in_directive(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        account,
                        uri,
                    ) {
                        locations.push(loc);
                    }
                }
            }
            Directive::Document(doc) => {
                if doc.account.as_ref() == account {
                    if let Some(loc) = find_in_directive(
                        source,
                        spanned.span.start,
                        spanned.span.end,
                        account,
                        uri,
                    ) {
                        locations.push(loc);
                    }
                }
            }
            Directive::Transaction(txn) => {
                for (i, posting) in txn.postings.iter().enumerate() {
                    if posting.account.as_ref() == account {
                        let posting_line = start_line + 1 + i as u32;
                        if let Some(line_text) = source.lines().nth(posting_line as usize) {
                            if let Some(col) = line_text.find(account) {
                                locations.push(Location {
                                    uri: uri.clone(),
                                    range: Range {
                                        start: Position::new(posting_line, col as u32),
                                        end: Position::new(
                                            posting_line,
                                            (col + account.len()) as u32,
                                        ),
                                    },
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Collect all references to a currency.
fn collect_currency_references(
    source: &str,
    parse_result: &ParseResult,
    currency: &str,
    uri: &Uri,
    include_declaration: bool,
    locations: &mut Vec<Location>,
) {
    for spanned in &parse_result.directives {
        let directive_text = &source[spanned.span.start..spanned.span.end];
        let (start_line, _) = byte_offset_to_position(source, spanned.span.start);

        // Check if directive contains this currency
        let is_declaration =
            matches!(&spanned.value, Directive::Commodity(c) if c.currency.as_ref() == currency);

        if is_declaration && !include_declaration {
            continue;
        }

        // Find all occurrences of the currency in this directive
        for (line_offset, line) in directive_text.lines().enumerate() {
            let mut search_start = 0;
            while let Some(pos) = line[search_start..].find(currency) {
                let actual_pos = search_start + pos;

                // Verify it's a word boundary
                let before_ok = actual_pos == 0
                    || !line
                        .chars()
                        .nth(actual_pos - 1)
                        .unwrap_or(' ')
                        .is_alphanumeric();
                let after_ok = actual_pos + currency.len() >= line.len()
                    || !line
                        .chars()
                        .nth(actual_pos + currency.len())
                        .unwrap_or(' ')
                        .is_alphanumeric();

                if before_ok && after_ok {
                    let ref_line = start_line + line_offset as u32;
                    locations.push(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position::new(ref_line, actual_pos as u32),
                            end: Position::new(ref_line, (actual_pos + currency.len()) as u32),
                        },
                    });
                }

                search_start = actual_pos + currency.len();
            }
        }
    }

    // Deduplicate by range
    locations.sort_by(|a, b| {
        a.range
            .start
            .line
            .cmp(&b.range.start.line)
            .then(a.range.start.character.cmp(&b.range.start.character))
    });
    locations.dedup_by(|a, b| a.range == b.range);
}

/// Collect all references to a payee.
fn collect_payee_references(
    source: &str,
    parse_result: &ParseResult,
    payee: &str,
    uri: &Uri,
    locations: &mut Vec<Location>,
) {
    for spanned in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned.value {
            if let Some(ref txn_payee) = txn.payee {
                if txn_payee.as_ref() == payee {
                    let (line, _) = byte_offset_to_position(source, spanned.span.start);
                    let line_text = source.lines().nth(line as usize).unwrap_or("");

                    // Find the payee in quotes
                    if let Some(start) = line_text.find(&format!("\"{}\"", payee)) {
                        locations.push(Location {
                            uri: uri.clone(),
                            range: Range {
                                start: Position::new(line, (start + 1) as u32),
                                end: Position::new(line, (start + 1 + payee.len()) as u32),
                            },
                        });
                    }
                }
            }
        }
    }
}

/// Find a string in a directive and create a location.
fn find_in_directive(
    source: &str,
    start_offset: usize,
    end_offset: usize,
    needle: &str,
    uri: &Uri,
) -> Option<Location> {
    let directive_text = &source[start_offset..end_offset];
    let (start_line, start_col) = byte_offset_to_position(source, start_offset);

    for (line_offset, line) in directive_text.lines().enumerate() {
        if let Some(col) = line.find(needle) {
            let ref_line = start_line + line_offset as u32;
            let ref_col = if line_offset == 0 {
                start_col + col as u32
            } else {
                col as u32
            };

            return Some(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position::new(ref_line, ref_col),
                    end: Position::new(ref_line, ref_col + needle.len() as u32),
                },
            });
        }
    }

    None
}

/// Get the word at a given position in a line.
fn get_word_at_position(line: &str, col: usize) -> Option<(String, usize, usize)> {
    if col > line.len() {
        return None;
    }

    let chars: Vec<char> = line.chars().collect();

    // Find word start
    let mut start = col;
    while start > 0 && is_word_char(chars.get(start - 1).copied().unwrap_or(' ')) {
        start -= 1;
    }

    // Find word end
    let mut end = col;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    let word: String = chars[start..end].iter().collect();
    Some((word, start, end))
}

/// Check if a character is part of a word.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == ':' || c == '-' || c == '_'
}

/// Check if a string looks like an account name.
fn is_account_like(s: &str) -> bool {
    s.contains(':')
        && (s.starts_with("Assets")
            || s.starts_with("Liabilities")
            || s.starts_with("Equity")
            || s.starts_with("Income")
            || s.starts_with("Expenses"))
}

/// Check if a string looks like a currency.
fn is_currency_like(s: &str, parse_result: &ParseResult) -> bool {
    if s.chars().all(|c| c.is_uppercase() || c.is_numeric()) && s.len() >= 2 && s.len() <= 24 {
        for spanned in &parse_result.directives {
            match &spanned.value {
                Directive::Commodity(comm) => {
                    if comm.currency.as_ref() == s {
                        return true;
                    }
                }
                Directive::Open(open) => {
                    for curr in &open.currencies {
                        if curr.as_ref() == s {
                            return true;
                        }
                    }
                }
                Directive::Balance(bal) => {
                    if bal.amount.currency.as_ref() == s {
                        return true;
                    }
                }
                Directive::Price(price) => {
                    if price.currency.as_ref() == s || price.amount.currency.as_ref() == s {
                        return true;
                    }
                }
                _ => {}
            }
        }
    }
    false
}

/// Check if position is inside quotes.
fn is_in_quotes(line: &str, col: usize) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let mut in_quotes = false;

    for (i, c) in chars.iter().enumerate() {
        if i >= col {
            break;
        }
        if *c == '"' {
            in_quotes = !in_quotes;
        }
    }

    in_quotes
}

/// Convert a byte offset to a line/column position (0-based for LSP).
fn byte_offset_to_position(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;

    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse;

    #[test]
    fn test_find_account_references() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-15 * "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
2024-01-31 balance Assets:Bank 100 USD
"#;
        let result = parse(source);
        let uri: Uri = "file:///test.beancount".parse().unwrap();

        let params = ReferenceParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: uri.clone() },
                position: Position::new(0, 16), // On "Assets:Bank"
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: lsp_types::ReferenceContext {
                include_declaration: true,
            },
        };

        let refs = handle_references(&params, source, &result, &uri);
        assert!(refs.is_some());

        let refs = refs.unwrap();
        // Should find: open, posting, balance = 3 references
        assert_eq!(refs.len(), 3);
    }

    #[test]
    fn test_find_currency_references() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-15 * "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food  5.00 USD
"#;
        let result = parse(source);
        let uri: Uri = "file:///test.beancount".parse().unwrap();

        let params = ReferenceParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: uri.clone() },
                position: Position::new(0, 28), // On "USD"
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: lsp_types::ReferenceContext {
                include_declaration: true,
            },
        };

        let refs = handle_references(&params, source, &result, &uri);
        assert!(refs.is_some());

        let refs = refs.unwrap();
        // Should find USD in: open, posting 1, posting 2 = 3 references
        assert_eq!(refs.len(), 3);
    }
}
