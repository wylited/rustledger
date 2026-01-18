//! Editor integration helpers for LSP-like functionality.
//!
//! This module provides completion, hover, go-to-definition, and document symbols
//! functionality adapted from rustledger-lsp for use in web editors like `CodeMirror`.

use rustledger_core::Directive;
use rustledger_parser::ParseResult;

use crate::types::{
    CompletionKind, EditorCompletion, EditorCompletionResult, EditorDocumentSymbol,
    EditorHoverInfo, EditorLocation, EditorRange, EditorReference, EditorReferencesResult,
    ReferenceKind, SymbolKind,
};

// =============================================================================
// Editor Cache
// =============================================================================

/// Cached data for editor features to avoid repeated extraction.
///
/// This is built once when a `ParsedLedger` is created and reused for all
/// completion, hover, and other editor requests.
#[derive(Debug, Clone)]
pub struct EditorCache {
    /// All unique account names in the document.
    pub accounts: Vec<String>,
    /// All unique currencies in the document.
    pub currencies: Vec<String>,
    /// All unique payees in the document.
    pub payees: Vec<String>,
    /// Line index for efficient offset-to-position conversion.
    pub line_index: LineIndex,
}

impl EditorCache {
    /// Build the editor cache from source and parse result.
    pub fn new(source: &str, parse_result: &ParseResult) -> Self {
        Self {
            accounts: extract_accounts(parse_result),
            currencies: extract_currencies(parse_result),
            payees: extract_payees(parse_result),
            line_index: LineIndex::new(source),
        }
    }
}

/// Line index for efficient offset-to-position conversion.
///
/// Building the index is O(n), but lookups are O(log(lines)).
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the start of each line.
    line_starts: Vec<usize>,
    /// Total length of the source.
    len: usize,
}

impl LineIndex {
    /// Build a line index from source text.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, ch) in source.char_indices() {
            if ch == '\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            line_starts,
            len: source.len(),
        }
    }

    /// Convert a byte offset to (line, column) position (0-based).
    pub fn offset_to_position(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.len);
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };
        let line_start = self.line_starts[line];
        let col = offset - line_start;
        (line as u32, col as u32)
    }
}

// =============================================================================
// Constants
// =============================================================================

/// Standard Beancount account types.
const ACCOUNT_TYPES: &[&str] = &["Assets", "Liabilities", "Equity", "Income", "Expenses"];

/// Standard Beancount directives.
const DIRECTIVES: &[(&str, &str)] = &[
    ("open", "Open an account"),
    ("close", "Close an account"),
    ("commodity", "Define a commodity/currency"),
    ("balance", "Assert account balance"),
    ("pad", "Pad account to target"),
    ("event", "Record an event"),
    ("query", "Define a named query"),
    ("note", "Add a note to an account"),
    ("document", "Link a document"),
    ("custom", "Custom directive"),
    ("price", "Record a price"),
    ("txn", "Transaction (complete)"),
    ("*", "Transaction (complete)"),
    ("!", "Transaction (incomplete)"),
];

// =============================================================================
// Completion Context Detection
// =============================================================================

/// Completion context detected from cursor position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionContext {
    /// At the start of a line (expecting date or directive).
    LineStart,
    /// After a date (expecting directive keyword or flag).
    AfterDate,
    /// Expecting an account name.
    ExpectingAccount,
    /// Inside an account name (after colon).
    AccountSegment {
        /// The prefix typed so far (e.g., "Assets:").
        prefix: String,
    },
    /// After an amount (expecting currency).
    ExpectingCurrency,
    /// Inside a string (payee/narration).
    InsideString,
    /// Unknown context.
    Unknown,
}

impl std::fmt::Display for CompletionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LineStart => write!(f, "line_start"),
            Self::AfterDate => write!(f, "after_date"),
            Self::ExpectingAccount => write!(f, "expecting_account"),
            Self::AccountSegment { prefix } => write!(f, "account_segment:{prefix}"),
            Self::ExpectingCurrency => write!(f, "expecting_currency"),
            Self::InsideString => write!(f, "inside_string"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

// =============================================================================
// Completions
// =============================================================================

/// Get completions at the given position (using cached data).
pub fn get_completions_cached(
    source: &str,
    line: u32,
    character: u32,
    cache: &EditorCache,
) -> EditorCompletionResult {
    let context = detect_context(source, line, character);
    let completions = match &context {
        CompletionContext::LineStart => complete_line_start(),
        CompletionContext::AfterDate => complete_after_date(),
        CompletionContext::ExpectingAccount => complete_account_start_cached(&cache.accounts),
        CompletionContext::AccountSegment { prefix } => {
            complete_account_segment_cached(prefix, &cache.accounts)
        }
        CompletionContext::ExpectingCurrency => complete_currency_cached(&cache.currencies),
        CompletionContext::InsideString => complete_payee_cached(&cache.payees),
        CompletionContext::Unknown => Vec::new(),
    };

    EditorCompletionResult {
        completions,
        context: context.to_string(),
    }
}

/// Get completions at the given position (legacy, extracts data each time).
#[allow(dead_code)] // Used by tests
pub fn get_completions(
    source: &str,
    line: u32,
    character: u32,
    parse_result: &ParseResult,
) -> EditorCompletionResult {
    let cache = EditorCache::new(source, parse_result);
    get_completions_cached(source, line, character, &cache)
}

/// Detect the completion context from cursor position.
fn detect_context(source: &str, line: u32, character: u32) -> CompletionContext {
    let line_text = get_line(source, line as usize);
    let col = character as usize;
    let before_cursor = if col <= line_text.len() {
        &line_text[..col]
    } else {
        line_text
    };

    let trimmed = before_cursor.trim_start();

    // Check if we're at the start of a posting (indented line)
    if before_cursor.starts_with("  ") || before_cursor.starts_with('\t') {
        if trimmed.is_empty() {
            return CompletionContext::ExpectingAccount;
        }

        let posting_content = trimmed;

        // Check if there's already an account (contains colon and space after)
        if posting_content.contains(':') && posting_content.contains(' ') {
            let parts: Vec<&str> = posting_content.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Some(last) = parts.last() {
                    if last.parse::<f64>().is_ok() || last.ends_with('.') {
                        return CompletionContext::ExpectingCurrency;
                    }
                }
            }
            return CompletionContext::Unknown;
        }

        // Check if typing an account segment
        if let Some(colon_pos) = posting_content.rfind(':') {
            let prefix = &posting_content[..=colon_pos];
            return CompletionContext::AccountSegment {
                prefix: prefix.to_string(),
            };
        }

        return CompletionContext::ExpectingAccount;
    }

    // Empty or whitespace only at line start (not indented)
    if trimmed.is_empty() {
        return CompletionContext::LineStart;
    }

    // Check for date at line start (YYYY-MM-DD pattern)
    if trimmed.len() >= 10 && is_date_like(&trimmed[..10]) {
        let after_date = trimmed[10..].trim_start();
        if after_date.is_empty() {
            return CompletionContext::AfterDate;
        }

        // Check for directive keywords
        for (directive, _) in DIRECTIVES {
            if let Some(rest) = after_date.strip_prefix(directive) {
                let after_directive = rest.trim_start();
                if after_directive.is_empty() || !after_directive.contains(' ') {
                    match *directive {
                        "open" | "close" | "balance" | "pad" | "note" | "document" => {
                            if let Some(colon_pos) = after_directive.rfind(':') {
                                return CompletionContext::AccountSegment {
                                    prefix: after_directive[..=colon_pos].to_string(),
                                };
                            }
                            return CompletionContext::ExpectingAccount;
                        }
                        _ => return CompletionContext::Unknown,
                    }
                }
            }
        }

        return CompletionContext::AfterDate;
    }

    // Check if inside a quoted string
    let quote_count = before_cursor.chars().filter(|&c| c == '"').count();
    if quote_count % 2 == 1 {
        return CompletionContext::InsideString;
    }

    CompletionContext::Unknown
}

/// Complete at line start (date template).
fn complete_line_start() -> Vec<EditorCompletion> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    vec![EditorCompletion {
        label: today.clone(),
        kind: CompletionKind::Date,
        detail: Some("Today's date".to_string()),
        insert_text: Some(format!("{today} ")),
    }]
}

/// Complete after a date (directive keywords).
fn complete_after_date() -> Vec<EditorCompletion> {
    DIRECTIVES
        .iter()
        .map(|(name, description)| EditorCompletion {
            label: (*name).to_string(),
            kind: CompletionKind::Keyword,
            detail: Some((*description).to_string()),
            insert_text: Some(format!("{name} ")),
        })
        .collect()
}

/// Complete account name start (account types) - cached version.
fn complete_account_start_cached(accounts: &[String]) -> Vec<EditorCompletion> {
    let mut items: Vec<EditorCompletion> = ACCOUNT_TYPES
        .iter()
        .map(|&t| EditorCompletion {
            label: format!("{t}:"),
            kind: CompletionKind::AccountSegment,
            detail: Some(format!("{t} account type")),
            insert_text: None,
        })
        .collect();

    // Also offer known accounts from the file
    for account in accounts.iter().take(20) {
        items.push(EditorCompletion {
            label: account.clone(),
            kind: CompletionKind::Account,
            detail: Some("Known account".to_string()),
            insert_text: None,
        });
    }

    items
}

/// Complete account segment after colon - cached version.
fn complete_account_segment_cached(prefix: &str, accounts: &[String]) -> Vec<EditorCompletion> {
    let matching: Vec<_> = accounts.iter().filter(|a| a.starts_with(prefix)).collect();

    let mut segments: Vec<String> = matching
        .iter()
        .filter_map(|a| {
            let after_prefix = &a[prefix.len()..];
            let next_segment = after_prefix.split(':').next()?;
            if next_segment.is_empty() {
                None
            } else {
                Some(next_segment.to_string())
            }
        })
        .collect();

    segments.sort();
    segments.dedup();

    segments
        .into_iter()
        .map(|seg| {
            let full = format!("{prefix}{seg}");
            let has_more = matching.iter().any(|a| a.starts_with(&format!("{full}:")));
            EditorCompletion {
                label: seg.clone(),
                kind: if has_more {
                    CompletionKind::AccountSegment
                } else {
                    CompletionKind::Account
                },
                detail: Some(if has_more {
                    "Account segment".to_string()
                } else {
                    "Account".to_string()
                }),
                insert_text: Some(if has_more { format!("{seg}:") } else { seg }),
            }
        })
        .collect()
}

/// Complete currency after amount - cached version.
fn complete_currency_cached(currencies: &[String]) -> Vec<EditorCompletion> {
    currencies
        .iter()
        .map(|c| EditorCompletion {
            label: c.clone(),
            kind: CompletionKind::Currency,
            detail: Some("Currency".to_string()),
            insert_text: None,
        })
        .collect()
}

/// Complete payee/narration inside string - cached version.
fn complete_payee_cached(payees: &[String]) -> Vec<EditorCompletion> {
    payees
        .iter()
        .take(20)
        .map(|p| EditorCompletion {
            label: p.clone(),
            kind: CompletionKind::Payee,
            detail: Some("Known payee".to_string()),
            insert_text: None,
        })
        .collect()
}

// =============================================================================
// Hover
// =============================================================================

/// Get hover information at the given position (using cached data).
pub fn get_hover_info_cached(
    source: &str,
    line: u32,
    character: u32,
    parse_result: &ParseResult,
    _cache: &EditorCache,
) -> Option<EditorHoverInfo> {
    // Hover doesn't benefit much from caching, but we keep the API consistent
    get_hover_info(source, line, character, parse_result)
}

/// Get hover information at the given position.
pub fn get_hover_info(
    source: &str,
    line: u32,
    character: u32,
    parse_result: &ParseResult,
) -> Option<EditorHoverInfo> {
    let word = get_word_at_position(source, line, character)?;

    // Check if it's an account name
    if word.contains(':') || is_account_type(&word) {
        if let Some(info) = get_account_hover_info(&word, parse_result) {
            return Some(EditorHoverInfo {
                contents: info,
                range: None,
            });
        }
    }

    // Check if it's a currency
    if is_currency_like(&word) {
        if let Some(info) = get_currency_hover_info(&word, parse_result) {
            return Some(EditorHoverInfo {
                contents: info,
                range: None,
            });
        }
    }

    // Check if it's a directive keyword
    if let Some(info) = get_directive_hover_info(&word) {
        return Some(EditorHoverInfo {
            contents: info,
            range: None,
        });
    }

    None
}

/// Get hover information about an account.
fn get_account_hover_info(account: &str, parse_result: &ParseResult) -> Option<String> {
    for spanned_directive in &parse_result.directives {
        if let Directive::Open(open) = &spanned_directive.value {
            let open_account = open.account.to_string();
            if open_account == account || account.starts_with(&format!("{open_account}:")) {
                let mut info = format!("## Account: `{open_account}`\n\n");
                let date = open.date;
                info.push_str(&format!("**Opened:** {date}\n\n"));

                if !open.currencies.is_empty() {
                    let currencies: Vec<String> = open
                        .currencies
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect();
                    let joined = currencies.join(", ");
                    info.push_str(&format!("**Currencies:** {joined}\n\n"));
                }

                let usage_count = count_account_usages(account, parse_result);
                info.push_str(&format!("**Used in:** {usage_count} postings"));

                return Some(info);
            }
        }
    }

    // Account not found in open directives
    let usage_count = count_account_usages(account, parse_result);
    if usage_count > 0 {
        return Some(format!(
            "## Account: `{account}`\n\n**Note:** No `open` directive found\n\n**Used in:** {usage_count} postings"
        ));
    }

    None
}

/// Get hover information about a currency.
fn get_currency_hover_info(currency: &str, parse_result: &ParseResult) -> Option<String> {
    for spanned_directive in &parse_result.directives {
        if let Directive::Commodity(comm) = &spanned_directive.value {
            if comm.currency.as_ref() == currency {
                let mut info = format!("## Currency: `{currency}`\n\n");
                let date = comm.date;
                info.push_str(&format!("**Defined:** {date}\n"));

                let usage_count = count_currency_usages(currency, parse_result);
                info.push_str(&format!("\n**Used in:** {usage_count} amounts"));

                return Some(info);
            }
        }
    }

    let usage_count = count_currency_usages(currency, parse_result);
    if usage_count > 0 {
        return Some(format!(
            "## Currency: `{currency}`\n\n**Note:** No `commodity` directive found\n\n**Used in:** {usage_count} amounts"
        ));
    }

    None
}

/// Get hover information about a directive keyword.
fn get_directive_hover_info(keyword: &str) -> Option<String> {
    let info = match keyword {
        "open" => "## `open` Directive\n\nOpens an account for use in transactions.\n\n```beancount\n2024-01-01 open Assets:Bank USD\n```",
        "close" => "## `close` Directive\n\nCloses an account. No transactions allowed after this date.\n\n```beancount\n2024-12-31 close Assets:OldBank\n```",
        "commodity" => "## `commodity` Directive\n\nDefines a currency or commodity.\n\n```beancount\n2024-01-01 commodity USD\n```",
        "balance" => "## `balance` Directive\n\nAsserts the balance of an account at a given date.\n\n```beancount\n2024-01-01 balance Assets:Bank 1000.00 USD\n```",
        "pad" => "## `pad` Directive\n\nAutomatically pads an account to match a balance assertion.\n\n```beancount\n2024-01-01 pad Assets:Bank Equity:Opening-Balances\n```",
        "event" => "## `event` Directive\n\nRecords a named event with a value.\n\n```beancount\n2024-01-01 event \"location\" \"New York\"\n```",
        "note" => "## `note` Directive\n\nAttaches a note to an account.\n\n```beancount\n2024-01-01 note Assets:Bank \"Account opened\"\n```",
        "document" => "## `document` Directive\n\nLinks a document to an account.\n\n```beancount\n2024-01-01 document Assets:Bank \"/path/to/statement.pdf\"\n```",
        "query" => "## `query` Directive\n\nDefines a named BQL query.\n\n```beancount\n2024-01-01 query \"expenses\" \"SELECT account, sum(amount)\"\n```",
        "custom" => "## `custom` Directive\n\nA custom directive for extensions.\n\n```beancount\n2024-01-01 custom \"budget\" Expenses:Food 500.00 USD\n```",
        "price" => "## `price` Directive\n\nRecords a price for a commodity.\n\n```beancount\n2024-01-01 price BTC 45000.00 USD\n```",
        "txn" | "*" => "## Transaction\n\nA complete (balanced) transaction.\n\n```beancount\n2024-01-01 * \"Payee\" \"Description\"\n  Assets:Bank  -100.00 USD\n  Expenses:Food\n```",
        "!" => "## Transaction (Incomplete)\n\nAn incomplete or flagged transaction.\n\n```beancount\n2024-01-01 ! \"Payee\" \"Needs review\"\n  Assets:Bank  -100.00 USD\n  Expenses:Unknown\n```",
        "include" => "## `include` Directive\n\nIncludes another Beancount file.\n\n```beancount\ninclude \"other-file.beancount\"\n```",
        "option" => "## `option` Directive\n\nSets a Beancount option.\n\n```beancount\noption \"operating_currency\" \"USD\"\n```",
        "plugin" => "## `plugin` Directive\n\nLoads a plugin.\n\n```beancount\nplugin \"beancount.plugins.auto_accounts\"\n```",
        _ => return None,
    };

    Some(info.to_string())
}

// =============================================================================
// Go-to-Definition
// =============================================================================

/// Get the definition location for the symbol at the given position (using cached data).
pub fn get_definition_cached(
    source: &str,
    line: u32,
    character: u32,
    parse_result: &ParseResult,
    cache: &EditorCache,
) -> Option<EditorLocation> {
    let word = get_word_at_position(source, line, character)?;

    // Check if it's an account name
    if word.contains(':') || is_account_type(&word) {
        if let Some(location) =
            find_account_definition_cached(&word, parse_result, &cache.line_index)
        {
            return Some(location);
        }
    }

    // Check if it's a currency
    if is_currency_like(&word) {
        if let Some(location) =
            find_currency_definition_cached(&word, parse_result, &cache.line_index)
        {
            return Some(location);
        }
    }

    None
}

/// Find the definition of an account (the Open directive) - cached version using `LineIndex`.
fn find_account_definition_cached(
    account: &str,
    parse_result: &ParseResult,
    line_index: &LineIndex,
) -> Option<EditorLocation> {
    for spanned_directive in &parse_result.directives {
        if let Directive::Open(open) = &spanned_directive.value {
            let open_account = open.account.to_string();
            if open_account == account || account.starts_with(&format!("{open_account}:")) {
                let (line, character) = line_index.offset_to_position(spanned_directive.span.start);
                return Some(EditorLocation { line, character });
            }
        }
    }
    None
}

/// Get the definition location for the symbol at the given position (legacy, creates cache each time).
#[allow(dead_code)] // Used by tests
pub fn get_definition(
    source: &str,
    line: u32,
    character: u32,
    parse_result: &ParseResult,
) -> Option<EditorLocation> {
    let cache = EditorCache::new(source, parse_result);
    get_definition_cached(source, line, character, parse_result, &cache)
}

/// Find the definition of a currency (the Commodity directive) - cached version using `LineIndex`.
fn find_currency_definition_cached(
    currency: &str,
    parse_result: &ParseResult,
    line_index: &LineIndex,
) -> Option<EditorLocation> {
    for spanned_directive in &parse_result.directives {
        if let Directive::Commodity(comm) = &spanned_directive.value {
            if comm.currency.as_ref() == currency {
                let (line, character) = line_index.offset_to_position(spanned_directive.span.start);
                return Some(EditorLocation { line, character });
            }
        }
    }
    None
}

// =============================================================================
// Document Symbols
// =============================================================================

/// Get all document symbols (for outline view) - cached version using `LineIndex`.
pub fn get_document_symbols_cached(
    parse_result: &ParseResult,
    cache: &EditorCache,
) -> Vec<EditorDocumentSymbol> {
    parse_result
        .directives
        .iter()
        .filter_map(|spanned| {
            directive_to_symbol_cached(
                &spanned.value,
                spanned.span.start,
                spanned.span.end,
                &cache.line_index,
            )
        })
        .collect()
}

/// Get all document symbols (for outline view) - legacy version.
#[allow(dead_code)] // Used by tests
pub fn get_document_symbols(source: &str, parse_result: &ParseResult) -> Vec<EditorDocumentSymbol> {
    let line_index = LineIndex::new(source);
    parse_result
        .directives
        .iter()
        .filter_map(|spanned| {
            directive_to_symbol_cached(
                &spanned.value,
                spanned.span.start,
                spanned.span.end,
                &line_index,
            )
        })
        .collect()
}

/// Convert a directive to a document symbol - cached version using `LineIndex`.
fn directive_to_symbol_cached(
    directive: &Directive,
    start_offset: usize,
    end_offset: usize,
    line_index: &LineIndex,
) -> Option<EditorDocumentSymbol> {
    let (start_line, start_col) = line_index.offset_to_position(start_offset);
    let (end_line, end_col) = line_index.offset_to_position(end_offset);

    let range = EditorRange {
        start_line,
        start_character: start_col,
        end_line,
        end_character: end_col,
    };

    match directive {
        Directive::Transaction(txn) => {
            let date = txn.date;
            let name = if let Some(ref payee) = txn.payee {
                format!("{date} {payee}")
            } else if !txn.narration.is_empty() {
                let narration = &txn.narration;
                format!("{date} {narration}")
            } else {
                format!("{date} Transaction")
            };

            let detail = if txn.narration.is_empty() {
                None
            } else {
                Some(txn.narration.to_string())
            };

            let children: Vec<EditorDocumentSymbol> = txn
                .postings
                .iter()
                .enumerate()
                .map(|(i, posting)| {
                    let posting_name = posting.account.to_string();
                    let posting_detail = posting.units.as_ref().map(|u| {
                        if let (Some(num), Some(curr)) = (u.number(), u.currency()) {
                            format!("{num} {curr}")
                        } else if let Some(num) = u.number() {
                            num.to_string()
                        } else {
                            String::new()
                        }
                    });

                    let posting_line = start_line + 1 + i as u32;
                    let posting_range = EditorRange {
                        start_line: posting_line,
                        start_character: 2,
                        end_line: posting_line,
                        end_character: 50,
                    };

                    EditorDocumentSymbol {
                        name: posting_name,
                        detail: posting_detail,
                        kind: SymbolKind::Posting,
                        range: posting_range,
                        children: None,
                        deprecated: None,
                    }
                })
                .collect();

            Some(EditorDocumentSymbol {
                name,
                detail,
                kind: SymbolKind::Transaction,
                range,
                children: if children.is_empty() {
                    None
                } else {
                    Some(children)
                },
                deprecated: None,
            })
        }

        Directive::Open(open) => {
            let account = &open.account;
            Some(EditorDocumentSymbol {
                name: format!("open {account}"),
                detail: if open.currencies.is_empty() {
                    None
                } else {
                    Some(
                        open.currencies
                            .iter()
                            .map(std::string::ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                },
                kind: SymbolKind::Account,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Close(close) => {
            let account = &close.account;
            Some(EditorDocumentSymbol {
                name: format!("close {account}"),
                detail: None,
                kind: SymbolKind::Account,
                range,
                children: None,
                deprecated: Some(true),
            })
        }

        Directive::Balance(bal) => {
            let account = &bal.account;
            let number = &bal.amount.number;
            let currency = &bal.amount.currency;
            Some(EditorDocumentSymbol {
                name: format!("balance {account}"),
                detail: Some(format!("{number} {currency}")),
                kind: SymbolKind::Balance,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Pad(pad) => {
            let account = &pad.account;
            let source_account = &pad.source_account;
            Some(EditorDocumentSymbol {
                name: format!("pad {account}"),
                detail: Some(format!("from {source_account}")),
                kind: SymbolKind::Pad,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Commodity(comm) => {
            let currency = &comm.currency;
            Some(EditorDocumentSymbol {
                name: format!("commodity {currency}"),
                detail: None,
                kind: SymbolKind::Commodity,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Event(event) => {
            let event_type = &event.event_type;
            Some(EditorDocumentSymbol {
                name: format!("event \"{event_type}\""),
                detail: Some(event.value.clone()),
                kind: SymbolKind::Event,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Note(note) => {
            let account = &note.account;
            Some(EditorDocumentSymbol {
                name: format!("note {account}"),
                detail: Some(note.comment.clone()),
                kind: SymbolKind::Note,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Document(doc) => {
            let account = &doc.account;
            Some(EditorDocumentSymbol {
                name: format!("document {account}"),
                detail: Some(doc.path.clone()),
                kind: SymbolKind::Document,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Price(price) => {
            let currency = &price.currency;
            let number = &price.amount.number;
            let amount_currency = &price.amount.currency;
            Some(EditorDocumentSymbol {
                name: format!("price {currency}"),
                detail: Some(format!("{number} {amount_currency}")),
                kind: SymbolKind::Price,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Query(query) => {
            let name = &query.name;
            Some(EditorDocumentSymbol {
                name: format!("query \"{name}\""),
                detail: None,
                kind: SymbolKind::Query,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Custom(custom) => {
            let custom_type = &custom.custom_type;
            Some(EditorDocumentSymbol {
                name: format!("custom \"{custom_type}\""),
                detail: None,
                kind: SymbolKind::Custom,
                range,
                children: None,
                deprecated: None,
            })
        }
    }
}

// =============================================================================
// Find References
// =============================================================================

/// Find all references to the symbol at the given position (using cached data).
pub fn get_references_cached(
    source: &str,
    line: u32,
    character: u32,
    parse_result: &ParseResult,
    cache: &EditorCache,
) -> Option<EditorReferencesResult> {
    let word = get_word_at_position(source, line, character)?;

    // Check if it's an account name
    if word.contains(':') || is_account_type(&word) {
        return Some(find_account_references(
            &word,
            source,
            parse_result,
            &cache.line_index,
        ));
    }

    // Check if it's a currency
    if is_currency_like(&word) && cache.currencies.contains(&word) {
        return Some(find_currency_references(
            &word,
            source,
            parse_result,
            &cache.line_index,
        ));
    }

    // Check if it's a payee (inside quotes on a transaction line)
    if cache.payees.contains(&word) {
        return Some(find_payee_references(
            &word,
            source,
            parse_result,
            &cache.line_index,
        ));
    }

    None
}

/// Find all references to an account.
fn find_account_references(
    account: &str,
    source: &str,
    parse_result: &ParseResult,
    line_index: &LineIndex,
) -> EditorReferencesResult {
    let mut references = Vec::new();

    for spanned_directive in &parse_result.directives {
        let (start_line, _) = line_index.offset_to_position(spanned_directive.span.start);
        let directive_line = get_line(source, start_line as usize);

        match &spanned_directive.value {
            Directive::Open(open) => {
                let open_account = open.account.to_string();
                if open_account == account || account.starts_with(&format!("{open_account}:")) {
                    if let Some(range) =
                        find_word_in_line(directive_line, &open_account, start_line)
                    {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Account,
                            is_definition: true,
                            context: Some("open".to_string()),
                        });
                    }
                }
            }
            Directive::Close(close) => {
                let close_account = close.account.to_string();
                if close_account == account {
                    if let Some(range) =
                        find_word_in_line(directive_line, &close_account, start_line)
                    {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Account,
                            is_definition: false,
                            context: Some("close".to_string()),
                        });
                    }
                }
            }
            Directive::Balance(bal) => {
                let bal_account = bal.account.to_string();
                if bal_account == account {
                    if let Some(range) = find_word_in_line(directive_line, &bal_account, start_line)
                    {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Account,
                            is_definition: false,
                            context: Some("balance".to_string()),
                        });
                    }
                }
            }
            Directive::Pad(pad) => {
                let pad_account = pad.account.to_string();
                let source_account = pad.source_account.to_string();
                if pad_account == account {
                    if let Some(range) = find_word_in_line(directive_line, &pad_account, start_line)
                    {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Account,
                            is_definition: false,
                            context: Some("pad".to_string()),
                        });
                    }
                }
                if source_account == account {
                    if let Some(range) =
                        find_word_in_line(directive_line, &source_account, start_line)
                    {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Account,
                            is_definition: false,
                            context: Some("pad source".to_string()),
                        });
                    }
                }
            }
            Directive::Note(note) => {
                let note_account = note.account.to_string();
                if note_account == account {
                    if let Some(range) =
                        find_word_in_line(directive_line, &note_account, start_line)
                    {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Account,
                            is_definition: false,
                            context: Some("note".to_string()),
                        });
                    }
                }
            }
            Directive::Document(doc) => {
                let doc_account = doc.account.to_string();
                if doc_account == account {
                    if let Some(range) = find_word_in_line(directive_line, &doc_account, start_line)
                    {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Account,
                            is_definition: false,
                            context: Some("document".to_string()),
                        });
                    }
                }
            }
            Directive::Transaction(txn) => {
                // Check postings - they're on subsequent lines
                for (i, posting) in txn.postings.iter().enumerate() {
                    let posting_account = posting.account.to_string();
                    if posting_account == account {
                        let posting_line = start_line + 1 + i as u32;
                        if let Some(line_text) = source.lines().nth(posting_line as usize) {
                            if let Some(range) =
                                find_word_in_line(line_text, &posting_account, posting_line)
                            {
                                references.push(EditorReference {
                                    range,
                                    kind: ReferenceKind::Account,
                                    is_definition: false,
                                    context: Some("posting".to_string()),
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    EditorReferencesResult {
        symbol: account.to_string(),
        kind: ReferenceKind::Account,
        references,
    }
}

/// Find all references to a currency.
fn find_currency_references(
    currency: &str,
    source: &str,
    parse_result: &ParseResult,
    line_index: &LineIndex,
) -> EditorReferencesResult {
    let mut references = Vec::new();

    for spanned_directive in &parse_result.directives {
        let (start_line, _) = line_index.offset_to_position(spanned_directive.span.start);
        let directive_line = get_line(source, start_line as usize);

        match &spanned_directive.value {
            Directive::Commodity(comm) => {
                if comm.currency.as_ref() == currency {
                    if let Some(range) = find_word_in_line(directive_line, currency, start_line) {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Currency,
                            is_definition: true,
                            context: Some("commodity".to_string()),
                        });
                    }
                }
            }
            Directive::Open(open) => {
                for curr in &open.currencies {
                    if curr.as_ref() == currency {
                        if let Some(range) = find_word_in_line(directive_line, currency, start_line)
                        {
                            references.push(EditorReference {
                                range,
                                kind: ReferenceKind::Currency,
                                is_definition: false,
                                context: Some("open".to_string()),
                            });
                        }
                    }
                }
            }
            Directive::Balance(bal) => {
                if bal.amount.currency.as_ref() == currency {
                    if let Some(range) = find_word_in_line(directive_line, currency, start_line) {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Currency,
                            is_definition: false,
                            context: Some("balance".to_string()),
                        });
                    }
                }
            }
            Directive::Price(price) => {
                if price.currency.as_ref() == currency {
                    if let Some(range) = find_word_in_line(directive_line, currency, start_line) {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Currency,
                            is_definition: false,
                            context: Some("price".to_string()),
                        });
                    }
                }
                if price.amount.currency.as_ref() == currency {
                    // Find second occurrence
                    if let Some(range) =
                        find_nth_word_in_line(directive_line, currency, start_line, 1)
                    {
                        references.push(EditorReference {
                            range,
                            kind: ReferenceKind::Currency,
                            is_definition: false,
                            context: Some("price amount".to_string()),
                        });
                    }
                }
            }
            Directive::Transaction(txn) => {
                for (i, posting) in txn.postings.iter().enumerate() {
                    if let Some(ref units) = posting.units {
                        if let Some(curr) = units.currency() {
                            if curr == currency {
                                let posting_line = start_line + 1 + i as u32;
                                if let Some(line_text) = source.lines().nth(posting_line as usize) {
                                    if let Some(range) =
                                        find_word_in_line(line_text, currency, posting_line)
                                    {
                                        references.push(EditorReference {
                                            range,
                                            kind: ReferenceKind::Currency,
                                            is_definition: false,
                                            context: Some("posting".to_string()),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    EditorReferencesResult {
        symbol: currency.to_string(),
        kind: ReferenceKind::Currency,
        references,
    }
}

/// Find all references to a payee.
fn find_payee_references(
    payee: &str,
    source: &str,
    parse_result: &ParseResult,
    line_index: &LineIndex,
) -> EditorReferencesResult {
    let mut references = Vec::new();

    for spanned_directive in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned_directive.value {
            if let Some(ref txn_payee) = txn.payee {
                if txn_payee == payee {
                    let (start_line, _) =
                        line_index.offset_to_position(spanned_directive.span.start);
                    let line_text = get_line(source, start_line as usize);

                    // Find the quoted payee in the line
                    let range = if let Some(range) =
                        find_quoted_string_in_line(line_text, payee, start_line)
                    {
                        range
                    } else {
                        // Fallback: use line start to payee length
                        EditorRange {
                            start_line,
                            start_character: 0,
                            end_line: start_line,
                            end_character: payee.len() as u32,
                        }
                    };

                    references.push(EditorReference {
                        range,
                        kind: ReferenceKind::Payee,
                        is_definition: references.is_empty(), // First occurrence is "definition"
                        context: Some("transaction".to_string()),
                    });
                }
            }
        }
    }

    EditorReferencesResult {
        symbol: payee.to_string(),
        kind: ReferenceKind::Payee,
        references,
    }
}

/// Find a quoted string in a line and return its range (including quotes).
fn find_quoted_string_in_line(line: &str, text: &str, line_num: u32) -> Option<EditorRange> {
    // Look for the text within quotes
    let quoted = format!("\"{text}\"");
    if let Some(pos) = line.find(&quoted) {
        return Some(EditorRange {
            start_line: line_num,
            start_character: pos as u32,
            end_line: line_num,
            end_character: (pos + quoted.len()) as u32,
        });
    }
    None
}

/// Find a word in a line and return its range.
fn find_word_in_line(line: &str, word: &str, line_num: u32) -> Option<EditorRange> {
    find_nth_word_in_line(line, word, line_num, 0)
}

/// Find the nth occurrence of a word in a line and return its range.
fn find_nth_word_in_line(line: &str, word: &str, line_num: u32, n: usize) -> Option<EditorRange> {
    let mut count = 0;
    let mut start = 0;

    while let Some(pos) = line[start..].find(word) {
        let abs_pos = start + pos;
        // Check word boundaries
        let before_ok = abs_pos == 0 || !is_word_char(line.chars().nth(abs_pos - 1)?);
        let after_ok = abs_pos + word.len() >= line.len()
            || !is_word_char(line.chars().nth(abs_pos + word.len())?);

        if before_ok && after_ok {
            if count == n {
                return Some(EditorRange {
                    start_line: line_num,
                    start_character: abs_pos as u32,
                    end_line: line_num,
                    end_character: (abs_pos + word.len()) as u32,
                });
            }
            count += 1;
        }
        start = abs_pos + 1;
    }
    None
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get a specific line from source.
fn get_line(source: &str, line_num: usize) -> &str {
    source.lines().nth(line_num).unwrap_or("")
}

/// Check if a string looks like a date (YYYY-MM-DD).
fn is_date_like(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let chars: Vec<char> = s.chars().collect();
    chars[4] == '-'
        && chars[7] == '-'
        && chars.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == '-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// Get the word at a given position in the source.
fn get_word_at_position(source: &str, line: u32, character: u32) -> Option<String> {
    let line_text = source.lines().nth(line as usize)?;
    let col = character as usize;

    if col > line_text.len() {
        return None;
    }

    let chars: Vec<char> = line_text.chars().collect();

    // Find start of word
    let mut start = col;
    while start > 0 && is_word_char(chars.get(start - 1).copied()?) {
        start -= 1;
    }

    // Find end of word
    let mut end = col;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    Some(chars[start..end].iter().collect())
}

/// Check if a character is part of a word (including account separators).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == ':' || c == '_' || c == '-'
}

/// Check if a string looks like an account type.
fn is_account_type(s: &str) -> bool {
    matches!(
        s,
        "Assets" | "Liabilities" | "Equity" | "Income" | "Expenses"
    )
}

/// Check if a string looks like a currency (all uppercase, 2-5 chars).
fn is_currency_like(s: &str) -> bool {
    s.len() >= 2
        && s.len() <= 5
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Extract all account names from parse result.
fn extract_accounts(parse_result: &ParseResult) -> Vec<String> {
    let mut accounts = Vec::new();

    for spanned_directive in &parse_result.directives {
        match &spanned_directive.value {
            Directive::Open(open) => accounts.push(open.account.to_string()),
            Directive::Close(close) => accounts.push(close.account.to_string()),
            Directive::Balance(bal) => accounts.push(bal.account.to_string()),
            Directive::Pad(pad) => {
                accounts.push(pad.account.to_string());
                accounts.push(pad.source_account.to_string());
            }
            Directive::Transaction(txn) => {
                for posting in &txn.postings {
                    accounts.push(posting.account.to_string());
                }
            }
            _ => {}
        }
    }

    accounts.sort();
    accounts.dedup();
    accounts
}

/// Extract all currencies from parse result.
fn extract_currencies(parse_result: &ParseResult) -> Vec<String> {
    let mut currencies = Vec::new();

    for spanned_directive in &parse_result.directives {
        match &spanned_directive.value {
            Directive::Open(open) => {
                for currency in &open.currencies {
                    currencies.push(currency.to_string());
                }
            }
            Directive::Commodity(comm) => currencies.push(comm.currency.to_string()),
            Directive::Balance(bal) => currencies.push(bal.amount.currency.to_string()),
            Directive::Transaction(txn) => {
                for posting in &txn.postings {
                    if let Some(ref units) = posting.units {
                        if let Some(currency) = units.currency() {
                            currencies.push(currency.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Add common defaults
    currencies.push("USD".to_string());
    currencies.push("EUR".to_string());
    currencies.push("GBP".to_string());

    currencies.sort();
    currencies.dedup();
    currencies
}

/// Extract payees from transactions.
fn extract_payees(parse_result: &ParseResult) -> Vec<String> {
    let mut payees = Vec::new();

    for spanned_directive in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned_directive.value {
            if let Some(ref payee) = txn.payee {
                payees.push(payee.to_string());
            }
        }
    }

    payees.sort();
    payees.dedup();
    payees
}

/// Count how many times an account is used in postings.
fn count_account_usages(account: &str, parse_result: &ParseResult) -> usize {
    let mut count = 0;
    for spanned_directive in &parse_result.directives {
        if let Directive::Transaction(txn) = &spanned_directive.value {
            for posting in &txn.postings {
                if posting.account.as_ref() == account {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Count how many times a currency is used.
#[allow(clippy::cmp_owned)]
fn count_currency_usages(currency: &str, parse_result: &ParseResult) -> usize {
    let mut count = 0;
    for spanned_directive in &parse_result.directives {
        match &spanned_directive.value {
            Directive::Transaction(txn) => {
                for posting in &txn.postings {
                    if let Some(ref units) = posting.units {
                        if let Some(c) = units.currency() {
                            if c.to_string() == currency {
                                count += 1;
                            }
                        }
                    }
                }
            }
            Directive::Balance(bal) => {
                if bal.amount.currency.as_ref() == currency {
                    count += 1;
                }
            }
            _ => {}
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse;

    #[test]
    fn test_detect_context_line_start() {
        let source = "\n";
        let ctx = detect_context(source, 0, 0);
        assert_eq!(ctx, CompletionContext::LineStart);
    }

    #[test]
    fn test_detect_context_after_date() {
        let source = "2024-01-15 ";
        let ctx = detect_context(source, 0, 11);
        assert_eq!(ctx, CompletionContext::AfterDate);
    }

    #[test]
    fn test_detect_context_expecting_account() {
        let source = "  ";
        let ctx = detect_context(source, 0, 2);
        assert_eq!(ctx, CompletionContext::ExpectingAccount);
    }

    #[test]
    fn test_detect_context_account_segment() {
        let source = "  Assets:";
        let ctx = detect_context(source, 0, 9);
        assert_eq!(
            ctx,
            CompletionContext::AccountSegment {
                prefix: "Assets:".to_string()
            }
        );
    }

    #[test]
    fn test_get_word_at_position() {
        let source = "2024-01-01 open Assets:Bank USD";

        let word = get_word_at_position(source, 0, 11);
        assert_eq!(word, Some("open".to_string()));

        let word = get_word_at_position(source, 0, 20);
        assert_eq!(word, Some("Assets:Bank".to_string()));

        let word = get_word_at_position(source, 0, 28);
        assert_eq!(word, Some("USD".to_string()));
    }

    #[test]
    fn test_get_completions_line_start() {
        let source = "";
        let result = parse(source);
        let completions = get_completions(source, 0, 0, &result);
        assert!(!completions.completions.is_empty());
        assert_eq!(completions.context, "line_start");
    }

    #[test]
    fn test_get_hover_info_directive() {
        let source = "2024-01-01 open Assets:Bank USD";
        let result = parse(source);
        let hover = get_hover_info(source, 0, 11, &result);
        assert!(hover.is_some());
        assert!(hover.unwrap().contents.contains("open"));
    }

    #[test]
    fn test_get_definition_account() {
        let source = r#"2024-01-01 open Assets:Bank USD

2024-01-15 * "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
"#;
        let result = parse(source);

        // Get definition of Assets:Bank from line 3
        let location = get_definition(source, 3, 4, &result);
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.line, 0); // Open directive is on line 0
    }

    #[test]
    fn test_get_document_symbols() {
        let source = r#"2024-01-01 open Assets:Bank USD

2024-01-15 * "Coffee Shop" "Morning coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
"#;
        let result = parse(source);
        let symbols = get_document_symbols(source, &result);

        assert_eq!(symbols.len(), 2); // open + transaction

        // First is the open directive
        assert!(symbols[0].name.contains("open"));
        assert_eq!(symbols[0].kind, SymbolKind::Account);

        // Second is the transaction with children (postings)
        assert!(symbols[1].name.contains("Coffee"));
        assert_eq!(symbols[1].kind, SymbolKind::Transaction);
        assert!(symbols[1].children.is_some());
        assert_eq!(symbols[1].children.as_ref().unwrap().len(), 2);
    }

    #[test]
    #[ignore = "Manual benchmark - run with: cargo test -p rustledger-wasm --release -- --ignored --nocapture"]
    fn bench_editor_cache_performance() {
        use std::time::Instant;

        // Load test file
        let source = std::fs::read_to_string("../../spec/fixtures/examples/example.beancount")
            .expect("Failed to read example.beancount");

        let parse_result = parse(&source);
        let directive_count = parse_result.directives.len();

        println!("\n=== Editor Performance Benchmark ===");
        println!("File: example.beancount");
        println!("Lines: {}", source.lines().count());
        println!("Directives: {directive_count}");

        // Measure cache build time (one-time cost)
        let start = Instant::now();
        let cache = EditorCache::new(&source, &parse_result);
        let cache_build_time = start.elapsed();
        println!("\nCache build time: {cache_build_time:?}");
        println!("  Accounts cached: {}", cache.accounts.len());
        println!("  Currencies cached: {}", cache.currencies.len());
        println!("  Payees cached: {}", cache.payees.len());

        // Measure cached operations (multiple calls)
        let iterations = 1000;

        // Completions
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = get_completions_cached(&source, 100, 2, &cache);
        }
        let completions_time = start.elapsed();
        println!(
            "\nCompletions ({iterations}x): {:?} ({:?}/call)",
            completions_time,
            completions_time / iterations
        );

        // Document symbols
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = get_document_symbols_cached(&parse_result, &cache);
        }
        let symbols_time = start.elapsed();
        println!(
            "Document symbols ({iterations}x): {:?} ({:?}/call)",
            symbols_time,
            symbols_time / iterations
        );

        // Definition lookup
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = get_definition_cached(&source, 500, 5, &parse_result, &cache);
        }
        let definition_time = start.elapsed();
        println!(
            "Definition lookup ({iterations}x): {:?} ({:?}/call)",
            definition_time,
            definition_time / iterations
        );

        // Compare with legacy (non-cached) approach
        println!("\n--- Legacy (non-cached) comparison ---");

        let start = Instant::now();
        for _ in 0..iterations {
            let _ = get_completions(&source, 100, 2, &parse_result);
        }
        let legacy_completions_time = start.elapsed();
        println!(
            "Legacy completions ({iterations}x): {:?} ({:?}/call)",
            legacy_completions_time,
            legacy_completions_time / iterations
        );

        let speedup =
            legacy_completions_time.as_nanos() as f64 / completions_time.as_nanos() as f64;
        println!("\nSpeedup (cached vs legacy): {speedup:.1}x faster");
    }
}
