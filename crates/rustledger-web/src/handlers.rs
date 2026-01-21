use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Form,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use tera::Context;

use rustledger_loader::Loader;

use crate::models::{
    CreateTransactionRequest, DeleteTransactionRequest, EditTransactionRequest, GetEditFormRequest,
    ToggleStatusRequest,
};
use crate::utils::{build_account_tree, extract_accounts, extract_recent_transactions};

/// Shared application state
pub struct AppState {
    pub ledger_path: PathBuf,
    pub tera: tera::Tera,
}

/// Helper function to load the ledger, ignoring cache for now
async fn load_ledger(state: &Arc<AppState>) -> anyhow::Result<rustledger_loader::LoadResult> {
    // Reload every time for simplicity and correctness with file edits
    let mut loader = Loader::new();
    let result = loader.load(&state.ledger_path)?;
    Ok(result)
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

    // Convert errors to strings for display
    let error_strings: Vec<String> = load_result.errors.iter().map(|e| e.to_string()).collect();

    let mut context = Context::new();
    context.insert("directive_count", &load_result.directives.len());
    // Fix field name: operating_currency (singular in some versions, check core)
    // Assuming rustledger-loader options struct, let's try operating_currency if that's what the compiler said
    // The compiler said: "help: a field with a similar name exists: operating_currency"
    context.insert(
        "options_count",
        &load_result.options.operating_currency.len(),
    );
    context.insert("source_file", &state.ledger_path.to_string_lossy());
    context.insert("errors", &error_strings);
    context.insert("account_tree", &account_tree);
    context.insert("recent_transactions", &recent_txns);
    context.insert("accounts", &accounts); // For dropdowns

    let rendered = match state.tera.render("index.html", &context) {
        Ok(t) => t,
        Err(e) => return Html(format!("<h1>Template Error</h1><p>{}</p>", e)),
    };

    Html(rendered)
}

/// Handler to create a new transaction.
pub async fn create_transaction(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<CreateTransactionRequest>,
) -> impl IntoResponse {
    let flag = if payload.cleared.is_some() { "*" } else { "!" };

    let payee_str = if let Some(p) = payload.payee {
        if !p.is_empty() {
            format!(" \"{}\"", p)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let narration_str = format!("\"{}\"", payload.narration);

    let mut txn_text = format!(
        "\n{} {} {}{}\n  {} {}\n",
        payload.date, flag, payee_str, narration_str, payload.account_1, payload.amount_1
    );

    if let (Some(acc2), Some(amt2)) = (payload.account_2, payload.amount_2) {
        if !acc2.is_empty() {
            txn_text.push_str(&format!("  {} {}\n", acc2, amt2));
        }
    }

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

    // Trigger a reload
    // In a real app we might update the partial list only via HTMX
    // For now, redirect home
    Redirect::to("/").into_response()
}

/// Handler to toggle the cleared status of a transaction.
pub async fn toggle_status(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<ToggleStatusRequest>,
) -> impl IntoResponse {
    let path_str = payload
        .source_path
        .as_deref()
        .unwrap_or(state.ledger_path.to_str().unwrap());
    let path = Path::new(path_str);

    // Safety check: ensure path is part of our ledger files (basic check)
    if !path.exists() {
        return (StatusCode::BAD_REQUEST, "Invalid source path").into_response();
    }

    let content = match fs::read_to_string(path) {
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
    // But let's do it properly
    let mut bytes = content.into_bytes();
    bytes[flag_idx] = new_flag as u8;

    match String::from_utf8(bytes) {
        Ok(new_content) => {
            if fs::write(path, new_content).is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write file").into_response();
            }
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "UTF8 Error").into_response(),
    }

    // Return the new flag for the UI (HTMX swap)
    Html(new_flag.to_string()).into_response()
}

/// Handler to delete a transaction.
pub async fn delete_transaction(
    State(_state): State<Arc<AppState>>,
    Form(payload): Form<DeleteTransactionRequest>,
) -> impl IntoResponse {
    let path = Path::new(&payload.source_path);

    if !path.exists() {
        return (StatusCode::NOT_FOUND, "Source file not found").into_response();
    }

    let mut file = match fs::File::open(path) {
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

    if fs::write(path, buffer).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write file").into_response();
    }

    // Return empty string to remove element from UI or redirect
    Redirect::to("/").into_response()
}

/// Handler to get the edit form for a transaction.
pub async fn get_edit_form(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GetEditFormRequest>,
) -> impl IntoResponse {
    let path = Path::new(&params.source_path);

    if !path.exists() {
        return Html("Error: File not found".to_string()).into_response();
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Html("Error reading file".to_string()).into_response(),
    };

    if params.offset + params.length > content.len() {
        return Html("Error: Invalid range".to_string()).into_response();
    }

    let raw_txn = &content[params.offset..params.offset + params.length];

    // Parse the raw transaction to pre-fill the form
    // Simple parsing for now (assumes standard format)
    // Date is first 10 chars
    let date = if raw_txn.len() >= 10 {
        &raw_txn[0..10]
    } else {
        ""
    };

    // Extract narration (between quotes)
    let narration_start = raw_txn.find('"').map(|i| i + 1).unwrap_or(0);
    let narration_end = raw_txn[narration_start..]
        .find('"')
        .map(|i| i + narration_start)
        .unwrap_or(raw_txn.len());
    let narration = if narration_start < narration_end {
        &raw_txn[narration_start..narration_end]
    } else {
        ""
    };

    // Cleared status
    let cleared = raw_txn.contains(" * ");

    // Extract accounts (simple heuristic looking for 2 spaces indentation)
    let mut accounts = Vec::new();
    let mut amounts = Vec::new();

    for line in raw_txn.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if !parts.is_empty() {
            accounts.push(parts[0].to_string());
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

    let mut context = Context::new();
    context.insert("date", date);
    context.insert("narration", narration);
    context.insert("cleared", &cleared);
    context.insert("account_1", accounts.first().unwrap_or(&String::new()));
    context.insert("amount_1", amounts.first().unwrap_or(&String::new()));
    context.insert("account_2", accounts.get(1).unwrap_or(&String::new()));
    context.insert("amount_2", amounts.get(1).unwrap_or(&String::new()));
    context.insert("original_offset", &params.offset);
    context.insert("original_length", &params.length);
    context.insert("original_source_path", &params.source_path);
    context.insert("accounts", &all_accounts); // For dropdowns

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
    State(_state): State<Arc<AppState>>,
    Form(payload): Form<EditTransactionRequest>,
) -> impl IntoResponse {
    // 1. Delete original
    let del_req = DeleteTransactionRequest {
        offset: payload.original_offset,
        length: payload.original_length,
        source_path: payload.original_source_path.clone(),
    };

    // We duplicate delete logic here or call a shared function
    // For safety, let's reuse logic carefully.
    // Ideally we wrap file ops in a mutex or use a transactional approach,
    // but for now we do sequential ops.

    let path = Path::new(&del_req.source_path);
    if !path.exists() {
        return Html("Error: Source file not found".to_string()).into_response();
    }

    let mut file_content = match fs::read(path) {
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
    let payee_str = if let Some(p) = payload.payee {
        if !p.is_empty() {
            format!(" \"{}\"", p)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let narration_str = format!("\"{}\"", payload.narration);

    let mut new_txn_text = format!(
        "\n{} {} {}{}\n  {} {}\n",
        payload.date, flag, payee_str, narration_str, payload.account_1, payload.amount_1
    );

    if let (Some(acc2), Some(amt2)) = (payload.account_2, payload.amount_2) {
        if !acc2.is_empty() {
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

    if let Err(e) = fs::write(path, file_content) {
        return Html(format!("Error writing file: {}", e)).into_response();
    }

    Redirect::to("/").into_response()
}
