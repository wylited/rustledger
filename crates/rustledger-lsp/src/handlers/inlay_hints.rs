//! Inlay hints handler for inline annotations.
//!
//! Provides inlay hints for:
//! - Inferred amounts on postings without explicit amounts
//! - Running balances (future enhancement)

use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams, Position};
use rustledger_core::{Decimal, Directive};
use rustledger_parser::ParseResult;
use std::collections::HashMap;

/// Handle an inlay hints request.
pub fn handle_inlay_hints(
    params: &InlayHintParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<Vec<InlayHint>> {
    let range = params.range;
    let mut hints = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for spanned in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned.value {
            let (start_line, _) = byte_offset_to_position(source, spanned.span.start);

            // Skip if transaction is outside the requested range
            if start_line > range.end.line {
                continue;
            }

            // Calculate the inferred amount for postings without amounts
            let inferred = calculate_inferred_amount(txn);

            for (i, posting) in txn.postings.iter().enumerate() {
                let posting_line = start_line + 1 + i as u32;

                // Skip if outside range
                if posting_line < range.start.line || posting_line > range.end.line {
                    continue;
                }

                // Only show hint for postings without explicit amount
                if posting.units.is_none() {
                    if let Some((amount, currency)) = &inferred {
                        if let Some(line) = lines.get(posting_line as usize) {
                            // Position hint at the end of the account name
                            let trimmed = line.trim();
                            let indent = line.len() - line.trim_start().len();
                            let end_col = indent + trimmed.len();

                            hints.push(InlayHint {
                                position: Position::new(posting_line, end_col as u32),
                                label: InlayHintLabel::String(format!("  {} {}", amount, currency)),
                                kind: Some(InlayHintKind::TYPE),
                                text_edits: None,
                                tooltip: Some(lsp_types::InlayHintTooltip::String(
                                    "Inferred amount to balance transaction".to_string(),
                                )),
                                padding_left: Some(true),
                                padding_right: None,
                                data: None,
                            });
                        }
                    }
                }
            }
        }
    }

    if hints.is_empty() {
        None
    } else {
        Some(hints)
    }
}

/// Calculate the inferred amount for a transaction with one empty posting.
fn calculate_inferred_amount(txn: &rustledger_core::Transaction) -> Option<(Decimal, String)> {
    // Count postings with and without amounts
    let mut amounts_by_currency: HashMap<String, Decimal> = HashMap::new();
    let mut empty_posting_count = 0;

    for posting in &txn.postings {
        if let Some(ref units) = posting.units {
            if let (Some(num), Some(curr)) = (units.number(), units.currency()) {
                let currency = curr.to_string();
                *amounts_by_currency.entry(currency).or_insert(Decimal::ZERO) += num;
            }
        } else {
            empty_posting_count += 1;
        }
    }

    // Only infer if exactly one posting has no amount and we have a single currency
    if empty_posting_count == 1 && amounts_by_currency.len() == 1 {
        let (currency, total) = amounts_by_currency.into_iter().next()?;
        // The inferred amount is the negation of the sum
        Some((-total, currency))
    } else {
        None
    }
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
    fn test_inlay_hints_inferred_amount() {
        let source = r#"2024-01-15 * "Coffee Shop"
  Assets:Bank  -5.00 USD
  Expenses:Food
"#;
        let result = parse(source);
        let params = InlayHintParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            range: lsp_types::Range {
                start: Position::new(0, 0),
                end: Position::new(3, 0),
            },
            work_done_progress_params: Default::default(),
        };

        let hints = handle_inlay_hints(&params, source, &result);
        assert!(hints.is_some());

        let hints = hints.unwrap();
        assert_eq!(hints.len(), 1);

        // The hint should show the inferred amount (5.00 USD)
        if let InlayHintLabel::String(label) = &hints[0].label {
            assert!(label.contains("5.00"));
            assert!(label.contains("USD"));
        }
    }

    #[test]
    fn test_calculate_inferred_amount() {
        let source = r#"2024-01-15 * "Test"
  Assets:Bank  -10.00 USD
  Expenses:Food
"#;
        let result = parse(source);

        if let Some(spanned) = result.directives.first() {
            if let Directive::Transaction(txn) = &spanned.value {
                let inferred = calculate_inferred_amount(txn);
                assert!(inferred.is_some());

                let (amount, currency) = inferred.unwrap();
                assert_eq!(amount, Decimal::new(1000, 2)); // 10.00
                assert_eq!(currency, "USD");
            }
        }
    }
}
