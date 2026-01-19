//! Inlay hints handler for inline annotations.
//!
//! Provides inlay hints for:
//! - Inferred amounts on postings without explicit amounts
//! - Running balances (future enhancement)
//!
//! Supports resolve for lazy-loading rich tooltips with account details.

use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams, Position};
use rustledger_core::{Decimal, Directive};
use rustledger_parser::ParseResult;
use std::collections::HashMap;

use super::utils::byte_offset_to_position;

/// Handle an inlay hints request.
pub fn handle_inlay_hints(
    params: &InlayHintParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<Vec<InlayHint>> {
    let range = params.range;
    let uri = params.text_document.uri.as_str();
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

                            // Store data for resolve - include account for rich tooltip
                            let data = serde_json::json!({
                                "uri": uri,
                                "kind": "inferred_amount",
                                "account": posting.account.to_string(),
                                "amount": amount.to_string(),
                                "currency": currency,
                            });

                            hints.push(InlayHint {
                                position: Position::new(posting_line, end_col as u32),
                                label: InlayHintLabel::String(format!("  {} {}", amount, currency)),
                                kind: Some(InlayHintKind::TYPE),
                                text_edits: None,
                                tooltip: None, // Resolved lazily
                                padding_left: Some(true),
                                padding_right: None,
                                data: Some(data),
                            });
                        }
                    }
                }
            }
        }
    }

    if hints.is_empty() { None } else { Some(hints) }
}

/// Handle an inlay hint resolve request.
/// Adds rich tooltip with account balance information.
pub fn handle_inlay_hint_resolve(hint: InlayHint, parse_result: &ParseResult) -> InlayHint {
    let mut resolved = hint.clone();

    // Check if we have data to resolve
    if let Some(data) = &hint.data {
        if let Some(kind) = data.get("kind").and_then(|v| v.as_str()) {
            if kind == "inferred_amount" {
                let account = data.get("account").and_then(|v| v.as_str()).unwrap_or("");
                let amount = data.get("amount").and_then(|v| v.as_str()).unwrap_or("");
                let currency = data.get("currency").and_then(|v| v.as_str()).unwrap_or("");

                // Build rich tooltip with account information
                let tooltip = build_account_tooltip(account, amount, currency, parse_result);
                resolved.tooltip = Some(lsp_types::InlayHintTooltip::MarkupContent(
                    lsp_types::MarkupContent {
                        kind: lsp_types::MarkupKind::Markdown,
                        value: tooltip,
                    },
                ));
            }
        }
    }

    resolved
}

/// Build a rich tooltip for an inferred amount hint.
fn build_account_tooltip(
    account: &str,
    inferred_amount: &str,
    currency: &str,
    parse_result: &ParseResult,
) -> String {
    let mut balances: HashMap<String, Decimal> = HashMap::new();
    let mut transaction_count = 0;

    // Calculate running balance for this account
    for spanned in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned.value {
            for posting in &txn.postings {
                if posting.account.as_ref() == account {
                    transaction_count += 1;
                    if let Some(units) = &posting.units {
                        if let Some(number) = units.number() {
                            let curr = units.currency().unwrap_or("???").to_string();
                            *balances.entry(curr).or_default() += number;
                        }
                    }
                }
            }
        }
    }

    let mut tooltip = format!("**Inferred:** {} {}\n\n", inferred_amount, currency);
    tooltip.push_str(&format!("**Account:** `{}`\n\n", account));

    if transaction_count > 0 {
        tooltip.push_str(&format!("📊 {} transactions\n\n", transaction_count));

        if !balances.is_empty() {
            tooltip.push_str("**Current Balance:**\n");
            for (curr, amount) in &balances {
                tooltip.push_str(&format!("- {} {}\n", amount, curr));
            }
        }
    } else {
        tooltip.push_str("_First transaction for this account_");
    }

    tooltip
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

    #[test]
    fn test_inlay_hint_resolve() {
        let source = r#"2024-01-15 * "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
2024-01-20 * "Lunch"
  Assets:Bank  -10.00 USD
  Expenses:Food
"#;
        let result = parse(source);

        // Create a hint with data that would be resolved
        let hint = InlayHint {
            position: Position::new(2, 15),
            label: InlayHintLabel::String("  5.00 USD".to_string()),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: None,
            data: Some(serde_json::json!({
                "kind": "inferred_amount",
                "account": "Expenses:Food",
                "amount": "5.00",
                "currency": "USD",
            })),
        };

        let resolved = handle_inlay_hint_resolve(hint, &result);

        // Should now have a tooltip
        assert!(resolved.tooltip.is_some());

        if let Some(lsp_types::InlayHintTooltip::MarkupContent(content)) = resolved.tooltip {
            assert!(content.value.contains("Expenses:Food"));
            assert!(content.value.contains("2 transactions"));
        }
    }
}
