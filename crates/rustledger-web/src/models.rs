use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Request payload for creating a new transaction.
#[derive(Deserialize, Debug)]
pub struct CreateTransactionRequest {
    /// Date of the transaction (YYYY-MM-DD).
    pub date: String,
    /// Optional payee name.
    pub payee: Option<String>,
    /// Narration or description of the transaction.
    pub narration: String,
    /// Cleared status checkbox ("on" if checked).
    pub cleared: Option<String>,
    /// First account name.
    pub account_1: String,
    /// First posting amount.
    pub amount_1: String,
    /// Second account name (optional).
    pub account_2: Option<String>,
    /// Second posting amount (optional).
    pub amount_2: Option<String>,
}

/// Request payload for toggling transaction cleared status.
#[derive(Deserialize, Debug)]
pub struct ToggleStatusRequest {
    /// Byte offset of the transaction in the file.
    pub offset: usize,
    /// Path to the source file containing the transaction.
    pub source_path: Option<String>,
}

/// A node in the account tree hierarchy.
#[derive(Serialize, Debug)]
pub struct AccountNode {
    /// Short name of the account (leaf segment).
    pub name: String,
    /// Full account name (e.g. Assets:Cash).
    pub full_name: String,
    /// Child accounts.
    pub children: BTreeMap<String, AccountNode>,
}

/// A single posting within a transaction.
#[derive(Serialize, Debug)]
pub struct TransactionPosting {
    /// Account name.
    pub account: String,
    /// Formatted amount string.
    pub amount: String,
}

/// Representation of a recent transaction for display.
#[derive(Serialize, Debug)]
pub struct RecentTransaction {
    /// Date string.
    pub date: String,
    /// Status flag (* or !).
    pub flag: String,
    /// Payee name.
    pub payee: String,
    /// Narration text.
    pub narration: String,
    /// List of postings.
    pub postings: Vec<TransactionPosting>,
    /// Byte offset in source file.
    pub offset: usize,
    /// Length in bytes.
    pub length: usize,
    /// Source file path.
    pub source_path: String,
}

/// Request payload for deleting a transaction.
#[derive(Deserialize, Debug)]
pub struct DeleteTransactionRequest {
    /// Byte offset of the transaction.
    pub offset: usize,
    /// Length of the transaction in bytes.
    pub length: usize,
    /// Source file path.
    pub source_path: String,
}

/// Request payload for updating an existing transaction.
#[derive(Deserialize, Debug)]
pub struct EditTransactionRequest {
    /// Original byte offset.
    pub original_offset: usize,
    /// Original length in bytes.
    pub original_length: usize,
    /// Original source file path.
    pub original_source_path: String,
    /// New date.
    pub date: String,
    /// New payee.
    pub payee: Option<String>,
    /// New narration.
    pub narration: String,
    /// New cleared status.
    pub cleared: Option<String>,
    /// New first account.
    pub account_1: String,
    /// New first amount.
    pub amount_1: String,
    /// New second account.
    pub account_2: Option<String>,
    /// New second amount.
    pub amount_2: Option<String>,
}

/// Query parameters for fetching the edit form.
#[derive(Deserialize, Debug)]
pub struct GetEditFormRequest {
    /// Byte offset of the transaction.
    pub offset: usize,
    /// Length of the transaction.
    pub length: usize,
    /// Source file path.
    pub source_path: String,
}

/// Net worth statistics.
#[derive(Serialize, Debug)]
pub struct NetWorthStats {
    /// Total assets value.
    pub assets: String,
    /// Total liabilities value.
    pub liabilities: String,
    /// Net worth (assets - liabilities).
    pub net_worth: String,
    /// Operating currency.
    pub currency: String,
}

/// Income and expenses for a period.
#[derive(Serialize, Debug)]
pub struct IncomeExpenseStats {
    /// Total income.
    pub income: String,
    /// Total expenses.
    pub expenses: String,
    /// Net (income - expenses).
    pub net: String,
    /// Period description.
    pub period: String,
    /// Operating currency.
    pub currency: String,
}

/// Monthly cash flow data point.
#[derive(Serialize, Debug)]
pub struct CashFlowPoint {
    /// Month label (e.g., "2024-01").
    pub month: String,
    /// Income for the month.
    pub income: f64,
    /// Expenses for the month.
    pub expenses: f64,
}

/// Net worth history data point.
#[derive(Serialize, Debug)]
pub struct NetWorthPoint {
    /// Date string.
    pub date: String,
    /// Net worth value.
    pub net_worth: f64,
}

/// Top account balance.
#[derive(Serialize, Debug)]
pub struct AccountBalance {
    /// Account name.
    pub account: String,
    /// Balance value.
    pub balance: String,
    /// Numeric balance for sorting.
    pub balance_numeric: f64,
}

/// Request payload for opening an account.
#[derive(Deserialize, Debug)]
pub struct OpenAccountRequest {
    /// Date to open the account (YYYY-MM-DD).
    pub date: String,
    /// Full account name (e.g., Assets:Bank:Checking).
    pub account: String,
    /// Currencies/commodities the account can hold (comma-separated).
    pub currencies: Option<String>,
}

/// Request payload for closing an account.
#[derive(Deserialize, Debug)]
pub struct CloseAccountRequest {
    /// Date to close the account (YYYY-MM-DD).
    pub date: String,
    /// Full account name to close.
    pub account: String,
}

/// Recurring transaction template (for future use).
#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RecurringTemplate {
    /// Template name/identifier.
    pub name: String,
    /// Description of what this template is for.
    pub description: String,
    /// Default payee.
    pub payee: Option<String>,
    /// Default narration.
    pub narration: String,
    /// Account 1.
    pub account_1: String,
    /// Amount 1 (can include currency).
    pub amount_1: String,
    /// Account 2.
    pub account_2: Option<String>,
    /// Amount 2.
    pub amount_2: Option<String>,
    /// Tags to apply.
    pub tags: Vec<String>,
}
