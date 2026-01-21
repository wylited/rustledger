use crate::models::{AccountNode, RecentTransaction, TransactionPosting};
use rustledger_core::Directive;
use rustledger_parser::Spanned;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Extracts a sorted list of unique account names from directives.
///
/// Iterates through all directives and collects account names from
/// Open, Close, Transaction, Balance, Note, Document, and Pad directives.
pub fn extract_accounts(directives: &[Spanned<Directive>]) -> Vec<String> {
    let mut accounts = BTreeSet::new();

    for directive in directives {
        match &directive.value {
            Directive::Open(open) => {
                accounts.insert(open.account.to_string());
            }
            Directive::Transaction(txn) => {
                for posting in &txn.postings {
                    accounts.insert(posting.account.to_string());
                }
            }
            Directive::Balance(bal) => {
                accounts.insert(bal.account.to_string());
            }
            Directive::Close(close) => {
                accounts.insert(close.account.to_string());
            }
            Directive::Note(note) => {
                accounts.insert(note.account.to_string());
            }
            Directive::Document(doc) => {
                accounts.insert(doc.account.to_string());
            }
            Directive::Pad(pad) => {
                accounts.insert(pad.account.to_string());
                accounts.insert(pad.source_account.to_string());
            }
            _ => {}
        }
    }

    accounts.into_iter().collect()
}

/// Builds a hierarchical tree structure from a flat list of account names.
///
/// Accounts like "Assets:Cash" are split into nested nodes: "Assets" -> "Cash".
pub fn build_account_tree(accounts: &[String]) -> BTreeMap<String, AccountNode> {
    let mut root: BTreeMap<String, AccountNode> = BTreeMap::new();

    for account in accounts {
        let parts: Vec<&str> = account.split(':').collect();
        let mut current_level = &mut root;
        let mut full_name_acc = String::new();

        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                full_name_acc.push(':');
            }
            full_name_acc.push_str(part);

            current_level = &mut current_level
                .entry(part.to_string())
                .or_insert_with(|| AccountNode {
                    name: part.to_string(),
                    full_name: full_name_acc.clone(),
                    children: BTreeMap::new(),
                })
                .children;
        }
    }

    root
}

/// Extracts the most recent transactions from the directive list.
///
/// Returns a list of `RecentTransaction` structs, limited by `limit`.
/// Also populates source file information for editing support.
pub fn extract_recent_transactions(
    directives: &[Spanned<Directive>],
    sources: &[PathBuf],
    limit: usize,
) -> Vec<RecentTransaction> {
    directives
        .iter()
        .zip(sources.iter())
        .filter_map(|(d, source)| {
            if let Directive::Transaction(txn) = &d.value {
                let postings = txn
                    .postings
                    .iter()
                    .map(|p| {
                        let amount_str = if let Some(units) = &p.units {
                            let number = units.number().map(|d| d.to_string()).unwrap_or_default();
                            let currency = units.currency().unwrap_or("");
                            format!("{} {}", number, currency)
                        } else {
                            String::new()
                        };

                        TransactionPosting {
                            account: p.account.to_string(),
                            amount: amount_str,
                        }
                    })
                    .collect();

                Some(RecentTransaction {
                    date: txn.date.to_string(),
                    flag: txn.flag.to_string(),
                    payee: txn.payee.clone().unwrap_or_default().to_string(),
                    narration: txn.narration.to_string(),
                    postings,
                    offset: d.span.start,
                    length: d.span.len(),
                    source_path: source.to_string_lossy().to_string(),
                })
            } else {
                None
            }
        })
        .rev()
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_account_tree() {
        let accounts = vec![
            "Assets:Cash".to_string(),
            "Assets:Bank:Checking".to_string(),
            "Expenses:Food".to_string(),
        ];

        let tree = build_account_tree(&accounts);

        assert!(tree.contains_key("Assets"));
        assert!(tree.contains_key("Expenses"));

        let assets = &tree["Assets"];
        assert_eq!(assets.full_name, "Assets");
        assert!(assets.children.contains_key("Cash"));
        assert!(assets.children.contains_key("Bank"));

        let bank = &assets.children["Bank"];
        assert_eq!(bank.full_name, "Assets:Bank");
        assert!(bank.children.contains_key("Checking"));
        assert_eq!(bank.children["Checking"].full_name, "Assets:Bank:Checking");
    }
}
