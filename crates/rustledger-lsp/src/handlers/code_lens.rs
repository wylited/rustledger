//! Code lens handler for showing inline information.
//!
//! Provides code lenses above:
//! - Account open directives (showing transaction count)
//! - Transactions (showing posting count and currencies)
//! - Balance assertions (with verification status)
//!
//! Supports resolve for lazy-loading expensive balance calculations.

use lsp_types::{CodeLens, CodeLensParams, Command, Position, Range};
use rustledger_core::{Decimal, Directive};
use rustledger_parser::ParseResult;
use std::collections::HashMap;

use super::utils::LineIndex;

/// Handle a code lens request.
pub fn handle_code_lens(
    params: &CodeLensParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<Vec<CodeLens>> {
    let line_index = LineIndex::new(source);
    let mut lenses = Vec::new();
    let uri = params.text_document.uri.as_str();

    // Collect account usage statistics
    let account_stats = collect_account_stats(parse_result);

    for spanned in &parse_result.directives {
        let (line, _) = line_index.offset_to_position(spanned.span.start);

        match &spanned.value {
            Directive::Open(open) => {
                let account = open.account.to_string();
                let stats = account_stats.get(&account);

                let txn_count = stats.map(|s| s.transaction_count).unwrap_or(0);
                let currencies: Vec<String> =
                    open.currencies.iter().map(|c| c.to_string()).collect();

                let title = if txn_count > 0 {
                    if currencies.is_empty() {
                        format!("{} transactions", txn_count)
                    } else {
                        format!("{} transactions | {}", txn_count, currencies.join(", "))
                    }
                } else if !currencies.is_empty() {
                    currencies.join(", ")
                } else {
                    "No transactions".to_string()
                };

                lenses.push(CodeLens {
                    range: Range {
                        start: Position::new(line, 0),
                        end: Position::new(line, 0),
                    },
                    command: Some(Command {
                        title,
                        command: "rledger.showAccountDetails".to_string(),
                        arguments: Some(vec![serde_json::json!(account)]),
                    }),
                    data: Some(serde_json::json!({ "uri": uri })),
                });
            }
            Directive::Transaction(txn) => {
                let posting_count = txn.postings.len();
                let currencies: Vec<String> = txn
                    .postings
                    .iter()
                    .filter_map(|p| {
                        p.units
                            .as_ref()
                            .and_then(|u| u.currency().map(String::from))
                    })
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                let title = if currencies.is_empty() {
                    format!("{} postings", posting_count)
                } else {
                    format!("{} postings | {}", posting_count, currencies.join(", "))
                };

                lenses.push(CodeLens {
                    range: Range {
                        start: Position::new(line, 0),
                        end: Position::new(line, 0),
                    },
                    command: Some(Command {
                        title,
                        command: "rledger.showTransactionDetails".to_string(),
                        arguments: None,
                    }),
                    data: Some(serde_json::json!({ "uri": uri })),
                });
            }
            Directive::Balance(bal) => {
                // Store data for resolve - verification is deferred
                let data = serde_json::json!({
                    "uri": uri,
                    "kind": "balance",
                    "account": bal.account.to_string(),
                    "date": bal.date.to_string(),
                    "expected_amount": bal.amount.number.to_string(),
                    "expected_currency": bal.amount.currency.to_string(),
                });

                lenses.push(CodeLens {
                    range: Range {
                        start: Position::new(line, 0),
                        end: Position::new(line, 0),
                    },
                    command: None, // Resolved lazily
                    data: Some(data),
                });
            }
            _ => {}
        }
    }

    if lenses.is_empty() {
        None
    } else {
        Some(lenses)
    }
}

/// Handle a code lens resolve request.
/// Computes expensive balance verification on demand.
pub fn handle_code_lens_resolve(lens: CodeLens, parse_result: &ParseResult) -> CodeLens {
    let mut resolved = lens.clone();

    if let Some(data) = &lens.data {
        if data.get("kind").and_then(|v| v.as_str()) == Some("balance") {
            let account = data.get("account").and_then(|v| v.as_str()).unwrap_or("");
            let date_str = data.get("date").and_then(|v| v.as_str()).unwrap_or("");
            let expected_amount = data
                .get("expected_amount")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<Decimal>().ok())
                .unwrap_or_default();
            let expected_currency = data
                .get("expected_currency")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Parse the date
            let date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok();

            // Calculate actual balance up to this date
            let actual_balance = calculate_balance_at_date(parse_result, account, date);
            let actual_amount = actual_balance
                .get(expected_currency)
                .copied()
                .unwrap_or_default();

            // Check if balance matches
            let (title, status) = if actual_amount == expected_amount {
                (
                    format!("✓ Balance: {} {}", expected_amount, expected_currency),
                    "verified",
                )
            } else {
                let diff = actual_amount - expected_amount;
                (
                    format!(
                        "✗ Balance: expected {} {}, actual {} {} (diff: {})",
                        expected_amount, expected_currency, actual_amount, expected_currency, diff
                    ),
                    "mismatch",
                )
            };

            resolved.command = Some(Command {
                title,
                command: "rledger.showBalanceDetails".to_string(),
                arguments: Some(vec![serde_json::json!({
                    "account": account,
                    "status": status,
                    "expected": format!("{} {}", expected_amount, expected_currency),
                    "actual": format!("{} {}", actual_amount, expected_currency),
                })]),
            });
        }
    }

    // If no data to resolve, return as-is (already has command)
    resolved
}

/// Calculate the balance of an account at a specific date.
fn calculate_balance_at_date(
    parse_result: &ParseResult,
    account: &str,
    date: Option<chrono::NaiveDate>,
) -> HashMap<String, Decimal> {
    let mut balances: HashMap<String, Decimal> = HashMap::new();

    for spanned in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned.value {
            // Only include transactions before the balance date
            if let Some(d) = date {
                if txn.date >= d {
                    continue;
                }
            }

            for posting in &txn.postings {
                if posting.account.as_ref() == account {
                    if let Some(units) = &posting.units {
                        if let Some(number) = units.number() {
                            let currency = units.currency().unwrap_or("???").to_string();
                            *balances.entry(currency).or_default() += number;
                        }
                    }
                }
            }
        }
    }

    balances
}

/// Statistics for an account.
#[derive(Default)]
struct AccountStats {
    transaction_count: usize,
}

/// Collect statistics about account usage.
fn collect_account_stats(parse_result: &ParseResult) -> HashMap<String, AccountStats> {
    let mut stats: HashMap<String, AccountStats> = HashMap::new();

    for spanned in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned.value {
            for posting in &txn.postings {
                let account = posting.account.to_string();
                stats.entry(account).or_default().transaction_count += 1;
            }
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse;

    #[test]
    fn test_code_lens_accounts() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-15 * "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
2024-01-16 * "Lunch"
  Assets:Bank  -10.00 USD
  Expenses:Food
"#;
        let result = parse(source);
        let params = CodeLensParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let lenses = handle_code_lens(&params, source, &result);
        assert!(lenses.is_some());

        let lenses = lenses.unwrap();
        // Should have: 1 open + 2 transactions = 3 lenses
        assert_eq!(lenses.len(), 3);

        // First lens is for the open directive
        assert!(
            lenses[0]
                .command
                .as_ref()
                .unwrap()
                .title
                .contains("2 transactions")
        );
    }

    #[test]
    fn test_code_lens_balance() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-31 balance Assets:Bank 100 USD
"#;
        let result = parse(source);
        let params = CodeLensParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let lenses = handle_code_lens(&params, source, &result);
        assert!(lenses.is_some());

        let lenses = lenses.unwrap();
        // Balance lens should have data but no command (resolved lazily)
        let balance_lens = lenses.iter().find(|l| {
            l.data
                .as_ref()
                .and_then(|d| d.get("kind"))
                .and_then(|v| v.as_str())
                == Some("balance")
        });
        assert!(balance_lens.is_some());
        assert!(balance_lens.unwrap().command.is_none());
    }

    #[test]
    fn test_code_lens_resolve_balance_match() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-15 * "Deposit"
  Assets:Bank  100.00 USD
  Income:Salary
2024-01-31 balance Assets:Bank 100 USD
"#;
        let result = parse(source);

        // Create a code lens like what handle_code_lens would return
        let lens = CodeLens {
            range: Range {
                start: Position::new(4, 0),
                end: Position::new(4, 0),
            },
            command: None,
            data: Some(serde_json::json!({
                "kind": "balance",
                "account": "Assets:Bank",
                "date": "2024-01-31",
                "expected_amount": "100",
                "expected_currency": "USD",
            })),
        };

        let resolved = handle_code_lens_resolve(lens, &result);
        assert!(resolved.command.is_some());

        let cmd = resolved.command.unwrap();
        assert!(cmd.title.contains("✓")); // Should show checkmark for match
        assert!(cmd.title.contains("100"));
    }

    #[test]
    fn test_code_lens_resolve_balance_mismatch() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-15 * "Deposit"
  Assets:Bank  50.00 USD
  Income:Salary
2024-01-31 balance Assets:Bank 100 USD
"#;
        let result = parse(source);

        let lens = CodeLens {
            range: Range {
                start: Position::new(4, 0),
                end: Position::new(4, 0),
            },
            command: None,
            data: Some(serde_json::json!({
                "kind": "balance",
                "account": "Assets:Bank",
                "date": "2024-01-31",
                "expected_amount": "100",
                "expected_currency": "USD",
            })),
        };

        let resolved = handle_code_lens_resolve(lens, &result);
        assert!(resolved.command.is_some());

        let cmd = resolved.command.unwrap();
        assert!(cmd.title.contains("✗")); // Should show X for mismatch
        assert!(cmd.title.contains("diff"));
    }
}
