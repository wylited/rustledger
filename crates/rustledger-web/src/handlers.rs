use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Form, Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use tera::Context;
use tokio::sync::{Mutex, RwLock};

use rustledger_loader::{LoadResult, Loader};

use crate::models::{
    CloseAccountRequest, CreateTransactionRequest, DeleteTransactionRequest,
    EditTransactionRequest, GetEditFormRequest, IncomeExpenseStats, NetWorthStats,
    OpenAccountRequest, ToggleStatusRequest,
};
use crate::utils::{
    build_account_tree, calculate_account_balance, calculate_cash_flow_history,
    calculate_monthly_income_expenses, calculate_net_worth, calculate_net_worth_history,
    detect_operating_currency, extract_account_transactions, extract_accounts, extract_payees,
    extract_recent_transactions, get_sub_accounts, get_top_accounts,
};

/// Shared application state
pub struct AppState {
    pub ledger_path: PathBuf,
    pub tera: tera::Tera,
    /// Cached ledger data, protected by RwLock for concurrent reads
    pub cached_ledger: RwLock<Option<LoadResult>>,
    /// Mutex to serialize file write operations
    pub write_lock: Mutex<()>,
}

/// Validates that a path is safe to access (within the ledger directory).
fn validate_path(source_path: &str, ledger_path: &Path) -> Result<PathBuf, &'static str> {
    let path = Path::new(source_path);

    // Get canonical paths for comparison
    let canonical_source = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => return Err("Invalid path"),
    };

    let ledger_dir = ledger_path.parent().unwrap_or(ledger_path);
    let canonical_ledger = match ledger_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return Err("Invalid ledger path"),
    };

    // Ensure the source path is within the ledger directory
    if !canonical_source.starts_with(&canonical_ledger) {
        return Err("Path outside ledger directory");
    }

    Ok(canonical_source)
}

/// Helper function to load the ledger with caching.
/// Uses RwLock to allow concurrent reads, only reloads when cache is invalidated.
async fn load_ledger(state: &Arc<AppState>) -> anyhow::Result<LoadResult> {
    // First, try to get cached data with a read lock
    {
        let cache = state.cached_ledger.read().await;
        if let Some(ref cached) = *cache {
            // Clone the cached result - this is relatively cheap since directives
            // use Arc internally for string data
            return Ok(clone_load_result(cached));
        }
    }

    // Cache miss - acquire write lock and load
    let mut cache = state.cached_ledger.write().await;
    
    // Double-check after acquiring write lock (another task may have loaded)
    if let Some(ref cached) = *cache {
        return Ok(clone_load_result(cached));
    }

    // Actually load the ledger
    let mut loader = Loader::new();
    let result = loader.load(&state.ledger_path)?;
    
    // Store in cache
    *cache = Some(clone_load_result(&result));
    
    Ok(result)
}

/// Invalidate the cached ledger (call after file modifications)
async fn invalidate_cache(state: &Arc<AppState>) {
    let mut cache = state.cached_ledger.write().await;
    *cache = None;
}

/// Clone a LoadResult for caching purposes.
/// This is necessary because LoadResult doesn't implement Clone.
fn clone_load_result(result: &LoadResult) -> LoadResult {
    LoadResult {
        directives: result.directives.clone(),
        directive_sources: result.directive_sources.clone(),
        options: result.options.clone(),
        plugins: result.plugins.clone(),
        source_map: result.source_map.clone(),
        errors: Vec::new(), // Errors are not cloneable, but we don't need them for cached reads
    }
}

/// Handler for the main dashboard page.
pub async fn index(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(e) => return Html(format!("<h1>Error loading ledger</h1><p>{}</p>", e)),
    };

    let accounts = extract_accounts(&load_result.directives);
    let account_tree = build_account_tree(&accounts);
    let recent_txns =
        extract_recent_transactions(&load_result.directives, &load_result.directive_sources, 10);

    // Get operating currency - use configured option or detect from ledger
    let operating_currency = load_result
        .options
        .operating_currency
        .first()
        .cloned()
        .unwrap_or_else(|| detect_operating_currency(&load_result.directives));
    let operating_currency = operating_currency.as_str();

    // Calculate financial stats
    let (assets, liabilities, net_worth) =
        calculate_net_worth(&load_result.directives, operating_currency);
    let (monthly_income, monthly_expenses) =
        calculate_monthly_income_expenses(&load_result.directives, operating_currency);
    let cash_flow = calculate_cash_flow_history(&load_result.directives, operating_currency, 12);
    let net_worth_history =
        calculate_net_worth_history(&load_result.directives, operating_currency, 12);
    let top_accounts = get_top_accounts(&load_result.directives, operating_currency, 5);

    // Convert errors to strings for display
    let error_strings: Vec<String> = load_result.errors.iter().map(|e| e.to_string()).collect();

    let mut context = Context::new();
    context.insert("current_page", "dashboard");
    context.insert("directive_count", &load_result.directives.len());
    context.insert(
        "options_count",
        &load_result.options.operating_currency.len(),
    );
    context.insert("source_file", &state.ledger_path.to_string_lossy());
    context.insert("errors", &error_strings);
    context.insert("account_tree", &account_tree);
    context.insert("recent_transactions", &recent_txns);
    context.insert("accounts", &accounts);

    // Financial stats
    context.insert("operating_currency", operating_currency);
    context.insert("net_worth", &format!("{:.2}", net_worth));
    context.insert("assets", &format!("{:.2}", assets));
    context.insert("liabilities", &format!("{:.2}", liabilities));
    context.insert("monthly_income", &format!("{:.2}", monthly_income));
    context.insert("monthly_expenses", &format!("{:.2}", monthly_expenses));
    context.insert(
        "monthly_net",
        &format!("{:.2}", monthly_income - monthly_expenses),
    );
    context.insert("cash_flow_data", &cash_flow);
    context.insert("net_worth_history", &net_worth_history);
    context.insert("top_accounts", &top_accounts);

    let rendered = match state.tera.render("index.html", &context) {
        Ok(t) => t,
        Err(e) => return Html(format!("<h1>Template Error</h1><p>{}</p>", e)),
    };

    Html(rendered)
}

/// Validates a date string is in YYYY-MM-DD format.
fn validate_date(date: &str) -> bool {
    if date.len() != 10 {
        return false;
    }
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts[0].chars().all(|c| c.is_ascii_digit())
        && parts[1].chars().all(|c| c.is_ascii_digit())
        && parts[2].chars().all(|c| c.is_ascii_digit())
}

/// Validates an account name (must start with valid root and contain only valid chars).
fn validate_account(account: &str) -> bool {
    if account.is_empty() {
        return false;
    }
    let valid_roots = ["Assets", "Liabilities", "Equity", "Income", "Expenses"];
    let starts_valid = valid_roots.iter().any(|r| account.starts_with(r));
    if !starts_valid {
        return false;
    }
    // Account names should only contain alphanumeric, colons, hyphens, underscores
    account
        .chars()
        .all(|c| c.is_alphanumeric() || c == ':' || c == '-' || c == '_')
}

/// Validates a string doesn't contain characters that could break beancount syntax.
fn validate_string_field(s: &str) -> bool {
    // Disallow newlines and unescaped quotes that could inject directives
    !s.contains('\n') && !s.contains('\r')
}

/// Handler to create a new transaction.
pub async fn create_transaction(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<CreateTransactionRequest>,
) -> impl IntoResponse {
    // Validate inputs
    if !validate_date(&payload.date) {
        return Html("<div class='text-red-500'>Invalid date format. Use YYYY-MM-DD.</div>")
            .into_response();
    }

    if !validate_account(&payload.account_1) {
        return Html("<div class='text-red-500'>Invalid account name.</div>").into_response();
    }

    if !validate_string_field(&payload.narration) {
        return Html("<div class='text-red-500'>Narration contains invalid characters.</div>")
            .into_response();
    }

    if let Some(ref payee) = payload.payee {
        if !validate_string_field(payee) {
            return Html("<div class='text-red-500'>Payee contains invalid characters.</div>")
                .into_response();
        }
    }

    let flag = if payload.cleared.is_some() { "*" } else { "!" };

    // Escape quotes in payee and narration
    let payee_str = if let Some(p) = payload.payee {
        if !p.is_empty() {
            format!(" \"{}\"", p.replace('"', "\\\""))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let narration_str = format!("\"{}\"", payload.narration.replace('"', "\\\""));

    let mut txn_text = format!(
        "\n{} {} {}{}\n  {} {}\n",
        payload.date, flag, payee_str, narration_str, payload.account_1, payload.amount_1
    );

    if let (Some(acc2), Some(amt2)) = (payload.account_2, payload.amount_2) {
        if !acc2.is_empty() {
            if !validate_account(&acc2) {
                return Html("<div class='text-red-500'>Invalid second account name.</div>")
                    .into_response();
            }
            txn_text.push_str(&format!("  {} {}\n", acc2, amt2));
        }
    }

    // Acquire write lock to serialize file modifications
    let _write_guard = state.write_lock.lock().await;

    // Append to file
    let mut file = match OpenOptions::new().append(true).open(&state.ledger_path) {
        Ok(f) => f,
        Err(e) => {
            return Html(format!(
                "<div class='text-red-500'>Error opening file: {}</div>",
                e
            ))
            .into_response();
        }
    };

    if let Err(e) = file.write_all(txn_text.as_bytes()) {
        return Html(format!(
            "<div class='text-red-500'>Error writing to file: {}</div>",
            e
        ))
        .into_response();
    }

    // Invalidate cache after successful write
    invalidate_cache(&state).await;

    // Use HX-Redirect for HTMX-friendly redirect
    (
        [("HX-Redirect", "/transactions")],
        Html("<div>Transaction created! Redirecting...</div>".to_string()),
    )
        .into_response()
}

/// Handler to toggle the cleared status of a transaction.
pub async fn toggle_status(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<ToggleStatusRequest>,
) -> impl IntoResponse {
    let default_path = state.ledger_path.to_string_lossy().to_string();
    let path_str = payload.source_path.as_deref().unwrap_or(&default_path);

    // Validate path is within ledger directory
    let path = match validate_path(path_str, &state.ledger_path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    // Acquire write lock to serialize file modifications
    let _write_guard = state.write_lock.lock().await;

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response();
        }
    };

    if payload.offset >= content.len() {
        return (StatusCode::BAD_REQUEST, "Invalid offset").into_response();
    }

    // Check the character at offset + date length (approximate location of flag)
    // Beancount format: YYYY-MM-DD * "..."
    //                  012345678901
    // Flag is usually at index 11 (space at 10)

    // We need to be careful. The offset points to the start of the transaction.
    // Let's find the flag relative to the offset.
    // A simple heuristic: look for " *" or " !" within the first 15 chars after offset
    let search_window = 15;
    let end = std::cmp::min(payload.offset + search_window, content.len());
    let slice = &content[payload.offset..end];

    let new_flag;
    let flag_idx;

    if let Some(idx) = slice.find(" *") {
        flag_idx = payload.offset + idx + 1; // +1 to point to *
        new_flag = '!';
    } else if let Some(idx) = slice.find(" !") {
        flag_idx = payload.offset + idx + 1; // +1 to point to !
        new_flag = '*';
    } else {
        // Fallback: maybe just replace the first * or ! found
        return (StatusCode::BAD_REQUEST, "Could not find flag to toggle").into_response();
    }

    // Replace the char
    // Rust strings are UTF-8, but * and ! are 1 byte.
    // We can't mutate String in place safely by index if not ascii, but here we replace 1 byte with 1 byte.
    // Safer to rebuild string

    // Since we are replacing 1 byte char with 1 byte char, byte manipulation is safe here
    let mut bytes = content.into_bytes();

    // Safety check for bounds
    if flag_idx >= bytes.len() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid flag index").into_response();
    }

    bytes[flag_idx] = new_flag as u8;

    match String::from_utf8(bytes) {
        Ok(new_content) => {
            if fs::write(&path, new_content).is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write file").into_response();
            }
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "UTF8 Error").into_response(),
    }

    // Invalidate cache after successful write
    invalidate_cache(&state).await;

    // Return the new button HTML for HTMX swap
    let button_html = if new_flag == '*' {
        format!(
            r#"<button class="cursor-pointer focus:outline-none hover:bg-gray-100 dark:hover:bg-gray-700 rounded-full p-1 transition-colors" 
                title="Mark as Uncleared"
                hx-post="/api/transactions/toggle-status"
                hx-vals='{{"offset": {}, "source_path": "{}"}}'
                hx-swap="outerHTML">
            <svg class="h-5 w-5 text-green-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
            </svg>
        </button>"#,
            payload.offset,
            path_str.replace('"', "&quot;")
        )
    } else {
        format!(
            r#"<button class="cursor-pointer focus:outline-none hover:bg-gray-100 dark:hover:bg-gray-700 rounded-full p-1 transition-colors" 
                title="Mark as Cleared"
                hx-post="/api/transactions/toggle-status"
                hx-vals='{{"offset": {}, "source_path": "{}"}}'
                hx-swap="outerHTML">
            <svg class="h-5 w-5 text-yellow-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
        </button>"#,
            payload.offset,
            path_str.replace('"', "&quot;")
        )
    };

    Html(button_html).into_response()
}

/// Handler to delete a transaction.
pub async fn delete_transaction(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<DeleteTransactionRequest>,
) -> impl IntoResponse {
    // Validate path is within ledger directory
    let path = match validate_path(&payload.source_path, &state.ledger_path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    // Acquire write lock to serialize file modifications
    let _write_guard = state.write_lock.lock().await;

    let mut file = match fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to open file").into_response();
        }
    };

    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response();
    }

    // Check bounds
    if payload.offset + payload.length > buffer.len() {
        return (StatusCode::BAD_REQUEST, "Invalid offset/length").into_response();
    }

    // Remove the bytes
    buffer.drain(payload.offset..payload.offset + payload.length);

    if fs::write(&path, buffer).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write file").into_response();
    }

    // Invalidate cache after successful write
    invalidate_cache(&state).await;

    // Use HX-Redirect for HTMX-friendly redirect
    (
        [("HX-Redirect", "/transactions")],
        Html("<div>Transaction deleted! Redirecting...</div>".to_string()),
    )
        .into_response()
}

/// Handler to get the edit form for a transaction.
pub async fn get_edit_form(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GetEditFormRequest>,
) -> impl IntoResponse {
    // Validate path is within ledger directory
    let path = match validate_path(&params.source_path, &state.ledger_path) {
        Ok(p) => p,
        Err(e) => return Html(format!("Error: {}", e)).into_response(),
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Html("Error reading file".to_string()).into_response(),
    };

    if params.offset + params.length > content.len() {
        return Html("Error: Invalid range".to_string()).into_response();
    }

    let raw_txn = &content[params.offset..params.offset + params.length];

    // Parse the raw transaction to pre-fill the form
    // Date is first 10 chars
    let date = if raw_txn.len() >= 10 {
        &raw_txn[0..10]
    } else {
        ""
    };

    // Cleared status
    let cleared = raw_txn.contains(" * ");

    // Extract payee and narration (format: YYYY-MM-DD * "payee" "narration" or YYYY-MM-DD * "narration")
    let mut payee = String::new();
    let mut narration = String::new();

    // Find all quoted strings in the first line
    let first_line = raw_txn.lines().next().unwrap_or("");
    let mut quoted_strings: Vec<&str> = Vec::new();
    let mut in_quote = false;
    let mut quote_start = 0;

    for (i, c) in first_line.char_indices() {
        if c == '"' {
            if in_quote {
                quoted_strings.push(&first_line[quote_start..i]);
                in_quote = false;
            } else {
                quote_start = i + 1;
                in_quote = true;
            }
        }
    }

    // If two quoted strings, first is payee, second is narration
    // If one quoted string, it's the narration
    if let (Some(p), Some(n)) = (quoted_strings.first(), quoted_strings.get(1)) {
        payee = p.to_string();
        narration = n.to_string();
    } else if let Some(n) = quoted_strings.first() {
        narration = n.to_string();
    }

    // Extract accounts (simple heuristic looking for 2 spaces indentation)
    let mut accounts = Vec::new();
    let mut amounts = Vec::new();

    for line in raw_txn.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if let Some(first) = parts.first() {
            accounts.push(first.to_string());
            if parts.len() > 1 {
                // Join the rest as amount
                amounts.push(parts[1..].join(" "));
            } else {
                amounts.push(String::new());
            }
        }
    }

    // Load accounts for dropdown
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(_) => return Html("Error loading accounts".to_string()).into_response(),
    };
    let all_accounts = extract_accounts(&load_result.directives);
    let all_payees = extract_payees(&load_result.directives);

    let mut context = Context::new();
    context.insert("date", date);
    context.insert("payee", &payee);
    context.insert("narration", &narration);
    context.insert("cleared", &cleared);
    context.insert("account_1", accounts.first().unwrap_or(&String::new()));
    context.insert("amount_1", amounts.first().unwrap_or(&String::new()));
    context.insert("account_2", accounts.get(1).unwrap_or(&String::new()));
    context.insert("amount_2", amounts.get(1).unwrap_or(&String::new()));
    context.insert("original_offset", &params.offset);
    context.insert("original_length", &params.length);
    context.insert("original_source_path", &params.source_path);
    context.insert("accounts", &all_accounts);
    context.insert("payees", &all_payees);

    let rendered = match state
        .tera
        .render("partials/transaction_edit_form.html", &context)
    {
        Ok(t) => t,
        Err(e) => format!("Template Error: {}", e),
    };

    Html(rendered).into_response()
}

/// Handler to process the update (delete old + create new).
pub async fn update_transaction(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<EditTransactionRequest>,
) -> impl IntoResponse {
    // Validate inputs
    if !validate_date(&payload.date) {
        return Html("<div class='text-red-500'>Invalid date format. Use YYYY-MM-DD.</div>")
            .into_response();
    }

    if !validate_account(&payload.account_1) {
        return Html("<div class='text-red-500'>Invalid account name.</div>").into_response();
    }

    if !validate_string_field(&payload.narration) {
        return Html("<div class='text-red-500'>Narration contains invalid characters.</div>")
            .into_response();
    }

    // 1. Delete original
    let del_req = DeleteTransactionRequest {
        offset: payload.original_offset,
        length: payload.original_length,
        source_path: payload.original_source_path.clone(),
    };

    // Validate path is within ledger directory
    let path = match validate_path(&del_req.source_path, &state.ledger_path) {
        Ok(p) => p,
        Err(e) => return Html(format!("Error: {}", e)).into_response(),
    };

    // Acquire write lock to serialize file modifications
    let _write_guard = state.write_lock.lock().await;

    let mut file_content = match fs::read(&path) {
        Ok(c) => c,
        Err(_) => return Html("Error: Read failed".to_string()).into_response(),
    };

    if del_req.offset + del_req.length > file_content.len() {
        return Html("Error: Invalid bounds".to_string()).into_response();
    }

    // remove old
    file_content.drain(del_req.offset..del_req.offset + del_req.length);

    // 2. Construct new transaction text
    let flag = if payload.cleared.is_some() { "*" } else { "!" };

    // Escape quotes in payee and narration
    let payee_str = if let Some(p) = payload.payee {
        if !p.is_empty() {
            format!(" \"{}\"", p.replace('"', "\\\""))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let narration_str = format!("\"{}\"", payload.narration.replace('"', "\\\""));

    let mut new_txn_text = format!(
        "\n{} {} {}{}\n  {} {}\n",
        payload.date, flag, payee_str, narration_str, payload.account_1, payload.amount_1
    );

    if let (Some(acc2), Some(amt2)) = (payload.account_2, payload.amount_2) {
        if !acc2.is_empty() {
            if !validate_account(&acc2) {
                return Html("<div class='text-red-500'>Invalid second account name.</div>")
                    .into_response();
            }
            new_txn_text.push_str(&format!("  {} {}\n", acc2, amt2));
        }
    }

    // Append new to end of file content (which is simpler than inserting in place)
    // Or insert at original position? Inserting at original position keeps date order roughly.
    // Let's insert at original offset to be nice.

    let new_bytes = new_txn_text.as_bytes();
    // We need to splice it in. `file_content` is a Vec<u8>
    // We already drained the old part. The cursor is effectively at `del_req.offset`.
    // So we can insert there.

    // Check if we need to add newlines for spacing if we are inserting in middle
    // But simplistic approach: just insert.

    // Splice:
    // Vec::splice is for replacing a range. We already removed. So we use insert_many?
    // Vec only has insert (single) or splice.
    // We can use splice with empty range to insert.

    file_content.splice(del_req.offset..del_req.offset, new_bytes.iter().cloned());

    if let Err(e) = fs::write(&path, file_content) {
        return Html(format!("Error writing file: {}", e)).into_response();
    }

    // Invalidate cache after successful write
    invalidate_cache(&state).await;

    // Return HX-Redirect header to trigger full page reload
    (
        [("HX-Redirect", "/transactions")],
        Html("<div>Transaction updated! Redirecting...</div>".to_string()),
    )
        .into_response()
}

/// Handler for the transactions list page.
pub async fn transactions_page(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(e) => return Html(format!("<h1>Error loading ledger</h1><p>{}</p>", e)),
    };

    let accounts = extract_accounts(&load_result.directives);
    let account_tree = build_account_tree(&accounts);
    // Get more transactions for the full list
    let transactions =
        extract_recent_transactions(&load_result.directives, &load_result.directive_sources, 100);

    let mut context = Context::new();
    context.insert("current_page", "transactions");
    context.insert("account_tree", &account_tree);
    context.insert("transactions", &transactions);
    context.insert("accounts", &accounts);

    let rendered = match state.tera.render("transactions.html", &context) {
        Ok(t) => t,
        Err(e) => return Html(format!("<h1>Template Error</h1><p>{}</p>", e)),
    };

    Html(rendered)
}

/// Handler for the add transaction page.
pub async fn add_transaction_page(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(e) => return Html(format!("<h1>Error loading ledger</h1><p>{}</p>", e)),
    };

    let accounts = extract_accounts(&load_result.directives);
    let account_tree = build_account_tree(&accounts);
    let payees = extract_payees(&load_result.directives);

    let mut context = Context::new();
    context.insert("current_page", "add");
    context.insert("account_tree", &account_tree);
    context.insert("accounts", &accounts);
    context.insert("payees", &payees);

    let rendered = match state.tera.render("add_transaction.html", &context) {
        Ok(t) => t,
        Err(e) => return Html(format!("<h1>Template Error</h1><p>{}</p>", e)),
    };

    Html(rendered)
}

/// API endpoint to get payees list.
pub async fn get_payees(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(_) => return Json(Vec::<String>::new()).into_response(),
    };

    let payees = extract_payees(&load_result.directives);
    Json(payees).into_response()
}

/// API endpoint for net worth stats.
pub async fn get_net_worth_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let operating_currency = load_result
        .options
        .operating_currency
        .first()
        .cloned()
        .unwrap_or_else(|| detect_operating_currency(&load_result.directives));

    let (assets, liabilities, net_worth) =
        calculate_net_worth(&load_result.directives, &operating_currency);

    Json(NetWorthStats {
        assets: format!("{:.2}", assets),
        liabilities: format!("{:.2}", liabilities),
        net_worth: format!("{:.2}", net_worth),
        currency: operating_currency,
    })
    .into_response()
}

/// API endpoint for income/expenses stats.
pub async fn get_income_expense_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let operating_currency = load_result
        .options
        .operating_currency
        .first()
        .cloned()
        .unwrap_or_else(|| detect_operating_currency(&load_result.directives));

    let (income, expenses) =
        calculate_monthly_income_expenses(&load_result.directives, &operating_currency);

    let now = chrono::Local::now();
    let period = format!("{}-{:02}", now.year(), now.month());

    Json(IncomeExpenseStats {
        income: format!("{:.2}", income),
        expenses: format!("{:.2}", expenses),
        net: format!("{:.2}", income - expenses),
        period,
        currency: operating_currency,
    })
    .into_response()
}

/// API endpoint for cash flow history.
pub async fn get_cash_flow(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let operating_currency = load_result
        .options
        .operating_currency
        .first()
        .cloned()
        .unwrap_or_else(|| detect_operating_currency(&load_result.directives));

    let cash_flow = calculate_cash_flow_history(&load_result.directives, &operating_currency, 12);
    Json(cash_flow).into_response()
}

/// API endpoint for net worth history.
pub async fn get_net_worth_history(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let operating_currency = load_result
        .options
        .operating_currency
        .first()
        .cloned()
        .unwrap_or_else(|| detect_operating_currency(&load_result.directives));

    let history = calculate_net_worth_history(&load_result.directives, &operating_currency, 12);
    Json(history).into_response()
}

use chrono::Datelike;

/// Handler to open a new account.
pub async fn open_account(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<OpenAccountRequest>,
) -> impl IntoResponse {
    // Validate inputs
    if !validate_date(&payload.date) {
        return Html(r#"<div class="text-red-500 p-4">Invalid date format. Use YYYY-MM-DD.</div>"#.to_string())
            .into_response();
    }

    if !validate_account(&payload.account) {
        return Html(r#"<div class="text-red-500 p-4">Invalid account name.</div>"#.to_string())
            .into_response();
    }

    // Build the open directive
    let currencies = payload
        .currencies
        .as_ref()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .map(|c| format!(" {}", c))
        .unwrap_or_default();

    let directive = format!(
        "\n{} open {}{}\n",
        payload.date, payload.account, currencies
    );

    // Acquire write lock to serialize file modifications
    let _write_guard = state.write_lock.lock().await;

    // Append to ledger file
    let mut file = match OpenOptions::new().append(true).open(&state.ledger_path) {
        Ok(f) => f,
        Err(e) => {
            return Html(format!(
                r#"<div class="text-red-500 p-4">Error opening file: {}</div>"#,
                e
            ))
            .into_response();
        }
    };

    if let Err(e) = file.write_all(directive.as_bytes()) {
        return Html(format!(
            r#"<div class="text-red-500 p-4">Error writing to file: {}</div>"#,
            e
        ))
        .into_response();
    }

    // Invalidate cache after successful write
    invalidate_cache(&state).await;

    // Return success message
    Html(format!(
        r#"<div class="text-green-600 dark:text-green-400 p-4 bg-green-50 dark:bg-green-900/20 rounded-lg">
            ✓ Account <strong>{}</strong> opened successfully
        </div>"#,
        payload.account
    ))
    .into_response()
}

/// Handler to close an account.
pub async fn close_account(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<CloseAccountRequest>,
) -> impl IntoResponse {
    // Validate inputs
    if !validate_date(&payload.date) {
        return Html(r#"<div class="text-red-500 p-4">Invalid date format. Use YYYY-MM-DD.</div>"#.to_string())
            .into_response();
    }

    if !validate_account(&payload.account) {
        return Html(r#"<div class="text-red-500 p-4">Invalid account name.</div>"#.to_string())
            .into_response();
    }

    let directive = format!("\n{} close {}\n", payload.date, payload.account);

    // Acquire write lock to serialize file modifications
    let _write_guard = state.write_lock.lock().await;

    // Append to ledger file
    let mut file = match OpenOptions::new().append(true).open(&state.ledger_path) {
        Ok(f) => f,
        Err(e) => {
            return Html(format!(
                r#"<div class="text-red-500 p-4">Error opening file: {}</div>"#,
                e
            ))
            .into_response();
        }
    };

    if let Err(e) = file.write_all(directive.as_bytes()) {
        return Html(format!(
            r#"<div class="text-red-500 p-4">Error writing to file: {}</div>"#,
            e
        ))
        .into_response();
    }

    // Invalidate cache after successful write
    invalidate_cache(&state).await;

    Html(format!(
        r#"<div class="text-green-600 dark:text-green-400 p-4 bg-green-50 dark:bg-green-900/20 rounded-lg">
            ✓ Account <strong>{}</strong> closed successfully
        </div>"#,
        payload.account
    ))
    .into_response()
}

/// Handler for the accounts management page.
pub async fn accounts_page(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(e) => return Html(format!("<h1>Error loading ledger</h1><p>{}</p>", e)),
    };

    let accounts = extract_accounts(&load_result.directives);
    let account_tree = build_account_tree(&accounts);

    let mut context = Context::new();
    context.insert("current_page", "accounts");
    context.insert("account_tree", &account_tree);
    context.insert("accounts", &accounts);

    let rendered = match state.tera.render("accounts.html", &context) {
        Ok(t) => t,
        Err(e) => return Html(format!("<h1>Template Error</h1><p>{}</p>", e)),
    };

    Html(rendered)
}

/// Handler for account detail page.
/// Shows transactions and balance for a specific account or account prefix.
pub async fn account_detail(
    State(state): State<Arc<AppState>>,
    AxumPath(account_path): AxumPath<String>,
) -> Html<String> {
    let load_result = match load_ledger(&state).await {
        Ok(res) => res,
        Err(e) => return Html(format!("<h1>Error loading ledger</h1><p>{}</p>", e)),
    };

    // URL-decode the account path (colons might be encoded)
    let account_name = urlencoding::decode(&account_path)
        .map(|s| s.into_owned())
        .unwrap_or(account_path);

    let accounts = extract_accounts(&load_result.directives);
    let account_tree = build_account_tree(&accounts);

    // Check if this is an exact account or a prefix
    let is_exact_account = accounts.contains(&account_name);
    let sub_accounts = get_sub_accounts(&accounts, &account_name);
    let has_sub_accounts =
        sub_accounts.len() > 1 || (!is_exact_account && !sub_accounts.is_empty());

    // Get transactions for this account/prefix
    let transactions = extract_account_transactions(
        &load_result.directives,
        &load_result.directive_sources,
        &account_name,
        100,
    );

    // Calculate balances
    let balances = calculate_account_balance(&load_result.directives, &account_name);

    // Format balances as a string
    let balance_display: Vec<String> = balances
        .iter()
        .map(|(currency, amount)| format!("{} {}", amount, currency))
        .collect();

    let mut context = Context::new();
    context.insert("current_page", "account_detail");
    context.insert("account_tree", &account_tree);
    context.insert("account_name", &account_name);
    context.insert("is_exact_account", &is_exact_account);
    context.insert("sub_accounts", &sub_accounts);
    context.insert("has_sub_accounts", &has_sub_accounts);
    context.insert("transactions", &transactions);
    context.insert("balances", &balance_display);
    context.insert("transaction_count", &transactions.len());

    let rendered = match state.tera.render("account_detail.html", &context) {
        Ok(t) => t,
        Err(e) => return Html(format!("<h1>Template Error</h1><p>{}</p>", e)),
    };

    Html(rendered)
}
