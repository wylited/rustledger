use crate::models::{
    AccountBalance, AccountNode, CashFlowPoint, NetWorthPoint, RecentTransaction,
    TransactionPosting,
};
use chrono::Datelike;
use rust_decimal::Decimal;
use rustledger_core::Directive;
use rustledger_parser::Spanned;
use std::collections::{BTreeMap, BTreeSet, HashMap};
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

/// Extracts unique payees from transactions.
pub fn extract_payees(directives: &[Spanned<Directive>]) -> Vec<String> {
    let mut payees = BTreeSet::new();

    for directive in directives {
        if let Directive::Transaction(txn) = &directive.value {
            if let Some(payee) = &txn.payee {
                let payee_str = payee.to_string();
                if !payee_str.is_empty() {
                    payees.insert(payee_str);
                }
            }
        }
    }

    payees.into_iter().collect()
}

/// Calculates account balances from directives.
/// Returns a map of account name -> (balance, currency).
pub fn calculate_balances(directives: &[Spanned<Directive>]) -> HashMap<String, (Decimal, String)> {
    let mut balances: HashMap<String, HashMap<String, Decimal>> = HashMap::new();

    for directive in directives {
        if let Directive::Transaction(txn) = &directive.value {
            for posting in &txn.postings {
                if let Some(units) = &posting.units {
                    if let (Some(number), Some(currency)) = (units.number(), units.currency()) {
                        let account = posting.account.to_string();
                        let currency_balances = balances.entry(account).or_default();
                        let balance = currency_balances
                            .entry(currency.to_string())
                            .or_insert(Decimal::ZERO);
                        *balance += number;
                    }
                }
            }
        }
    }

    // Flatten to single currency per account (take the first/primary)
    balances
        .into_iter()
        .filter_map(|(account, currencies)| {
            currencies
                .into_iter()
                .next()
                .map(|(currency, balance)| (account, (balance, currency)))
        })
        .collect()
}

/// Detects the most commonly used currency in the ledger.
pub fn detect_operating_currency(directives: &[Spanned<Directive>]) -> String {
    let mut currency_counts: HashMap<String, usize> = HashMap::new();

    for directive in directives {
        if let Directive::Transaction(txn) = &directive.value {
            for posting in &txn.postings {
                if let Some(units) = &posting.units {
                    if let Some(currency) = units.currency() {
                        *currency_counts.entry(currency.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    // Return the most common currency, or USD as fallback
    currency_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(currency, _)| currency)
        .unwrap_or_else(|| "USD".to_string())
}

/// Calculates net worth (Assets - Liabilities).
pub fn calculate_net_worth(
    directives: &[Spanned<Directive>],
    operating_currency: &str,
) -> (Decimal, Decimal, Decimal) {
    let mut assets = Decimal::ZERO;
    let mut liabilities = Decimal::ZERO;

    for directive in directives {
        if let Directive::Transaction(txn) = &directive.value {
            for posting in &txn.postings {
                if let Some(units) = &posting.units {
                    if let (Some(number), Some(currency)) = (units.number(), units.currency()) {
                        // Only count operating currency for now
                        if currency == operating_currency {
                            let account = posting.account.to_string();
                            if account.starts_with("Assets:") {
                                assets += number;
                            } else if account.starts_with("Liabilities:") {
                                liabilities += number;
                            }
                        }
                    }
                }
            }
        }
    }

    // Liabilities are typically negative in beancount, so net worth = assets + liabilities
    let net_worth = assets + liabilities;
    (assets, liabilities.abs(), net_worth)
}

/// Calculates income and expenses for the current month.
pub fn calculate_monthly_income_expenses(
    directives: &[Spanned<Directive>],
    operating_currency: &str,
) -> (Decimal, Decimal) {
    let now = chrono::Local::now();
    let current_year = now.year();
    let current_month = now.month();

    let mut income = Decimal::ZERO;
    let mut expenses = Decimal::ZERO;

    for directive in directives {
        if let Directive::Transaction(txn) = &directive.value {
            // Check if transaction is in current month
            if txn.date.year() == current_year && txn.date.month() == current_month {
                for posting in &txn.postings {
                    if let Some(units) = &posting.units {
                        if let (Some(number), Some(currency)) = (units.number(), units.currency()) {
                            if currency == operating_currency {
                                let account = posting.account.to_string();
                                if account.starts_with("Income:") {
                                    // Income postings are negative (credit)
                                    income += number.abs();
                                } else if account.starts_with("Expenses:") {
                                    expenses += number;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    (income, expenses)
}

/// Calculates cash flow (income vs expenses) for the last N months.
pub fn calculate_cash_flow_history(
    directives: &[Spanned<Directive>],
    operating_currency: &str,
    months: usize,
) -> Vec<CashFlowPoint> {
    use chrono::{Datelike, Months, NaiveDate};

    let now = chrono::Local::now().date_naive();
    let start_date = now - Months::new(months as u32 - 1);

    // Initialize months
    let mut monthly_data: BTreeMap<String, (Decimal, Decimal)> = BTreeMap::new();
    let mut current = NaiveDate::from_ymd_opt(start_date.year(), start_date.month(), 1).unwrap();
    let end = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap();

    while current <= end {
        let key = format!("{:04}-{:02}", current.year(), current.month());
        monthly_data.insert(key, (Decimal::ZERO, Decimal::ZERO));
        current = current + Months::new(1);
    }

    // Aggregate transactions
    for directive in directives {
        if let Directive::Transaction(txn) = &directive.value {
            let key = format!("{:04}-{:02}", txn.date.year(), txn.date.month());
            if let Some((income, expenses)) = monthly_data.get_mut(&key) {
                for posting in &txn.postings {
                    if let Some(units) = &posting.units {
                        if let (Some(number), Some(currency)) = (units.number(), units.currency()) {
                            if currency == operating_currency {
                                let account = posting.account.to_string();
                                if account.starts_with("Income:") {
                                    *income += number.abs();
                                } else if account.starts_with("Expenses:") {
                                    *expenses += number;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    monthly_data
        .into_iter()
        .map(|(month, (income, expenses))| CashFlowPoint {
            month,
            income: income.to_string().parse().unwrap_or(0.0),
            expenses: expenses.to_string().parse().unwrap_or(0.0),
        })
        .collect()
}

/// Calculates net worth over time (monthly snapshots).
pub fn calculate_net_worth_history(
    directives: &[Spanned<Directive>],
    operating_currency: &str,
    months: usize,
) -> Vec<NetWorthPoint> {
    use chrono::{Datelike, Months, NaiveDate};

    let now = chrono::Local::now().date_naive();

    // Collect all transactions sorted by date
    let mut transactions: Vec<_> = directives
        .iter()
        .filter_map(|d| {
            if let Directive::Transaction(txn) = &d.value {
                Some(txn)
            } else {
                None
            }
        })
        .collect();
    transactions.sort_by_key(|t| t.date);

    // Calculate running balance per month
    let mut assets = Decimal::ZERO;
    let mut liabilities = Decimal::ZERO;
    let mut monthly_net_worth: BTreeMap<String, Decimal> = BTreeMap::new();

    for txn in transactions {
        for posting in &txn.postings {
            if let Some(units) = &posting.units {
                if let (Some(number), Some(currency)) = (units.number(), units.currency()) {
                    if currency == operating_currency {
                        let account = posting.account.to_string();
                        if account.starts_with("Assets:") {
                            assets += number;
                        } else if account.starts_with("Liabilities:") {
                            liabilities += number;
                        }
                    }
                }
            }
        }
        let key = format!("{:04}-{:02}", txn.date.year(), txn.date.month());
        monthly_net_worth.insert(key, assets + liabilities);
    }

    // Get the last N months
    let start_date = now - Months::new(months as u32 - 1);
    let mut result = Vec::new();
    let mut current = NaiveDate::from_ymd_opt(start_date.year(), start_date.month(), 1).unwrap();
    let end = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap();
    let mut last_value = Decimal::ZERO;

    while current <= end {
        let key = format!("{:04}-{:02}", current.year(), current.month());
        if let Some(&nw) = monthly_net_worth.get(&key) {
            last_value = nw;
        }
        result.push(NetWorthPoint {
            date: key,
            net_worth: last_value.to_string().parse().unwrap_or(0.0),
        });
        current = current + Months::new(1);
    }

    result
}

/// Gets top accounts by absolute balance.
pub fn get_top_accounts(
    directives: &[Spanned<Directive>],
    operating_currency: &str,
    limit: usize,
) -> Vec<AccountBalance> {
    let balances = calculate_balances(directives);

    let mut account_list: Vec<_> = balances
        .into_iter()
        .filter(|(_, (_, currency))| currency == operating_currency)
        .filter(|(account, _)| {
            account.starts_with("Assets:") || account.starts_with("Liabilities:")
        })
        .map(|(account, (balance, currency))| {
            let balance_f64: f64 = balance.to_string().parse().unwrap_or(0.0);
            AccountBalance {
                account,
                balance: format!("{:.2} {}", balance, currency),
                balance_numeric: balance_f64,
            }
        })
        .collect();

    account_list.sort_by(|a, b| {
        b.balance_numeric
            .abs()
            .partial_cmp(&a.balance_numeric.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    account_list.truncate(limit);
    account_list
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

/// Extracts transactions for a specific account or account prefix.
///
/// If `account_filter` is "Assets:Bank", it will match transactions
/// with postings to "Assets:Bank", "Assets:Bank:Checking", etc.
pub fn extract_account_transactions(
    directives: &[Spanned<Directive>],
    sources: &[PathBuf],
    account_filter: &str,
    limit: usize,
) -> Vec<RecentTransaction> {
    directives
        .iter()
        .zip(sources.iter())
        .filter_map(|(d, source)| {
            if let Directive::Transaction(txn) = &d.value {
                // Check if any posting matches the filter
                let matches = txn.postings.iter().any(|p| {
                    let acc = p.account.to_string();
                    acc == account_filter || acc.starts_with(&format!("{}:", account_filter))
                });

                if !matches {
                    return None;
                }

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

/// Calculate balance for a specific account or prefix.
pub fn calculate_account_balance(
    directives: &[Spanned<Directive>],
    account_filter: &str,
) -> HashMap<String, Decimal> {
    let mut balances: HashMap<String, Decimal> = HashMap::new();

    for directive in directives {
        if let Directive::Transaction(txn) = &directive.value {
            for posting in &txn.postings {
                let account = posting.account.to_string();
                if account == account_filter || account.starts_with(&format!("{}:", account_filter))
                {
                    if let Some(units) = &posting.units {
                        if let Some(number) = units.number() {
                            let currency = units.currency().unwrap_or("").to_string();
                            *balances.entry(currency).or_insert(Decimal::ZERO) += number;
                        }
                    }
                }
            }
        }
    }

    balances
}

/// Get sub-accounts for an account prefix.
pub fn get_sub_accounts(accounts: &[String], prefix: &str) -> Vec<String> {
    accounts
        .iter()
        .filter(|a| *a == prefix || a.starts_with(&format!("{}:", prefix)))
        .cloned()
        .collect()
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
