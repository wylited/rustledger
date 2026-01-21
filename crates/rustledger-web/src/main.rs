use axum::{
    extract::{State, Form},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use clap::Parser;
use rustledger_core::Directive;
use rustledger_loader::Loader;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::{Write, Read, Seek, SeekFrom};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tera::{Context, Tera};
use serde::{Serialize, Deserialize};
use tower_http::services::ServeDir;

#[derive(Deserialize, Debug)]
struct CreateTransactionRequest {
    date: String,
    payee: Option<String>,
    narration: String,
    cleared: Option<String>, // Checkbox sends "on" if checked, nothing if not
    account_1: String,
    amount_1: String,
    account_2: Option<String>,
    amount_2: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ToggleStatusRequest {
    offset: usize,
}

#[derive(Serialize, Debug)]
struct AccountNode {
    name: String,
    full_name: String,
    children: BTreeMap<String, AccountNode>,
}

#[derive(Serialize, Debug)]
struct TransactionPosting {
    account: String,
    amount: String,
}

#[derive(Serialize, Debug)]
struct RecentTransaction {
    date: String,
    flag: String,
    payee: String,
    narration: String,
    postings: Vec<TransactionPosting>,
    // Store offsets for editing
    offset: usize,
    length: usize,
    source_path: String,
}

#[derive(Deserialize, Debug)]
struct DeleteTransactionRequest {
    offset: usize,
    length: usize,
    source_path: String,
}

#[derive(Deserialize, Debug)]
struct EditTransactionRequest {
    original_offset: usize,
    original_length: usize,
    original_source_path: String,
    // Fields to update
    date: String,
    payee: Option<String>,
    narration: String,
    cleared: Option<String>,
    account_1: String,
    amount_1: String,
    account_2: Option<String>,
    amount_2: Option<String>,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the ledger file
    #[arg(default_value = "main.beancount")]
    ledger_file: PathBuf,

    /// Port to listen on
    #[arg(short, long, default_value = "3000")]
    port: u16,
}

struct AppState {
    ledger_path: PathBuf,
    tera: Tera,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // Check if file exists
    if !args.ledger_file.exists() {
        tracing::error!("Ledger file not found: {}", args.ledger_file.display());
        std::process::exit(1);
    }

    // Initialize Tera
    let mut tera = Tera::new("crates/rustledger-web/templates/**/*")?;
    // Disable autoescape for now if needed, or keep enabled for security
    tera.autoescape_on(vec![".html", ".sql"]);

    let shared_state = Arc::new(AppState {
        ledger_path: args.ledger_file.clone(),
        tera,
    });

    // Build our application with a route
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/accounts/:name", get(account_handler))
        .route("/api/transactions", post(create_transaction_handler))
        .route("/api/transactions/toggle-status", post(toggle_status_handler))
        .route("/api/transactions/delete", post(delete_transaction_handler))
        .route("/api/transactions/edit-form", get(get_edit_form_handler))
        .route("/api/transactions/update", post(update_transaction_handler))
        .route("/api/stats/directives", get(directives_count_handler))
        .nest_service("/assets", ServeDir::new("crates/rustledger-web/assets"))
        .with_state(shared_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn extract_accounts(directives: &[rustledger_parser::Spanned<Directive>]) -> Vec<String> {
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

fn build_account_tree(accounts: &[String]) -> BTreeMap<String, AccountNode> {
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

fn extract_recent_transactions(
    directives: &[rustledger_parser::Spanned<Directive>],
    sources: &[PathBuf],
    limit: usize
) -> Vec<RecentTransaction> {
    directives.iter().zip(sources.iter())
        .filter_map(|(d, source)| {
            if let Directive::Transaction(txn) = &d.value {
                let postings = txn.postings.iter().map(|p| {
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
                }).collect();

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

async fn root_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = Loader::new().load(&state.ledger_path);
    
    let mut context = Context::new();
    
    match load_result {
        Ok(result) => {
            let accounts = extract_accounts(&result.directives);
            context.insert("accounts", &accounts);
            
            let account_tree = build_account_tree(&accounts);
            context.insert("account_tree", &account_tree);
            
            let recent_txns = extract_recent_transactions(&result.directives, &result.directive_sources, 10);
            context.insert("recent_transactions", &recent_txns);
            
            context.insert("filename", &state.ledger_path.display().to_string());
            context.insert("directive_count", &result.directives.len());
            context.insert("option_count", &result.options.set_options.len());
            context.insert("errors", &Vec::<String>::new()); // No errors for now in template
        },
        Err(e) => {
            context.insert("accounts", &Vec::<String>::new());
            context.insert("filename", &state.ledger_path.display().to_string());
            context.insert("directive_count", &0);
            context.insert("option_count", &0);
            context.insert("errors", &vec![e.to_string()]);
        }
    }

    match state.tera.render("index.html", &context) {
        Ok(html) => Html(html),
        Err(e) => {
            tracing::error!("Template rendering error: {}", e);
            Html(format!("<h1>Internal Server Error</h1><p>{}</p>", e))
        }
    }
}

async fn account_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(account_name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let load_result = Loader::new().load(&state.ledger_path);
    let mut context = Context::new();
    
    context.insert("account_name", &account_name);

    match load_result {
        Ok(result) => {
            let accounts = extract_accounts(&result.directives);
            context.insert("accounts", &accounts);
            
            let account_tree = build_account_tree(&accounts);
            context.insert("account_tree", &account_tree);
            
            // Filter transactions for this account
            let transactions: Vec<RecentTransaction> = result.directives.iter().zip(result.directive_sources.iter())
                .filter_map(|(d, source)| {
                    if let Directive::Transaction(txn) = &d.value {
                        // Check if any posting matches the account
                        let has_account = txn.postings.iter().any(|p| p.account.as_str() == account_name.as_str());
                        
                        if has_account {
                            let postings = txn.postings.iter().map(|p| {
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
                            }).collect();

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
                    } else {
                        None
                    }
                })
                .rev()
                .collect();
            
            context.insert("transactions", &transactions);
        },
        Err(e) => {
            context.insert("accounts", &Vec::<String>::new());
            context.insert("transactions", &Vec::<RecentTransaction>::new());
            context.insert("errors", &vec![e.to_string()]);
        }
    }

    match state.tera.render("account_details.html", &context) {
        Ok(html) => Html(html),
        Err(e) => {
            tracing::error!("Template rendering error: {}", e);
            Html(format!("<h1>Internal Server Error</h1><p>{}</p>", e))
        }
    }
}

async fn delete_transaction_handler(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<DeleteTransactionRequest>,
) -> impl IntoResponse {
    let path = PathBuf::from(&payload.source_path);
    let mut content = match std::fs::read(&path) {
        Ok(c) => c,
        Err(e) => return Html(format!("<div class='bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded'>Error reading file: {}</div>", e)),
    };

    let offset = payload.offset;
    let length = payload.length;
    let end = offset + length;

    // Safety checks
    if offset >= content.len() || end > content.len() {
        return Html("<div class='bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded'>Error: Transaction offset out of bounds. The file may have changed.</div>".to_string());
    }

    // Remove the transaction bytes
    // We also want to remove a preceding or trailing newline if possible to avoid gaps
    // But for now, just removing the exact span is safer.
    
    // Check if we can verify it's the main file? 
    // We can't easily, but if the offset is within bounds, we proceed.
    // Ideally we'd check if the content looks like a transaction.
    
    content.drain(offset..end);
    
    if let Err(e) = std::fs::write(path, content) {
        return Html(format!("<div class='bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded'>Error writing file: {}</div>", e));
    }

    Html("<div class='bg-green-100 border border-green-400 text-green-700 px-4 py-3 rounded'>Transaction deleted! <script>setTimeout(() => window.location.reload(), 1000)</script></div>".to_string())
}

async fn create_transaction_handler(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<CreateTransactionRequest>,
) -> impl IntoResponse {
    let flag = if payload.cleared.is_some() { "*" } else { "!" };
    let payee = if let Some(p) = &payload.payee {
        if !p.is_empty() {
            format!(" \"{}\"", p)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    
    let mut txn_text = format!(
        "\n{} {}{} \"{}\"\n", 
        payload.date, flag, payee, payload.narration
    );
    
    // Posting 1
    if !payload.account_1.is_empty() {
        if !payload.amount_1.is_empty() {
            txn_text.push_str(&format!("  {} {}\n", payload.account_1, payload.amount_1));
        } else {
            txn_text.push_str(&format!("  {}\n", payload.account_1));
        }
    }
    
    // Posting 2
    if let Some(acc2) = &payload.account_2 {
        if !acc2.is_empty() {
            if let Some(amt2) = &payload.amount_2 {
                if !amt2.is_empty() {
                    txn_text.push_str(&format!("  {} {}\n", acc2, amt2));
                } else {
                    txn_text.push_str(&format!("  {}\n", acc2));
                }
            } else {
                txn_text.push_str(&format!("  {}\n", acc2));
            }
        }
    }

    match OpenOptions::new()
        .append(true)
        .open(&state.ledger_path) 
    {
        Ok(mut file) => {
            if let Err(e) = file.write_all(txn_text.as_bytes()) {
                tracing::error!("Failed to write to ledger: {}", e);
                return Html(format!("<div class='bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded'>Error writing to file: {}</div>", e));
            }
        }
        Err(e) => {
            tracing::error!("Failed to open ledger: {}", e);
            return Html(format!("<div class='bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded'>Error opening file: {}</div>", e));
        }
    }
    
    // Return success message
    Html("<div class='bg-green-100 border border-green-400 text-green-700 px-4 py-3 rounded'>Transaction added successfully! <script>setTimeout(() => window.location.reload(), 1000)</script></div>".to_string())
}

async fn toggle_status_handler(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<ToggleStatusRequest>,
) -> impl IntoResponse {
    let mut file = match OpenOptions::new().read(true).write(true).open(&state.ledger_path) {
        Ok(f) => f,
        Err(e) => return Html(format!("<div class='text-red-500'>Error opening file: {}</div>", e)),
    };

    if let Err(e) = file.seek(SeekFrom::Start(payload.offset as u64)) {
         return Html(format!("<div class='text-red-500'>Error seeking: {}</div>", e));
    }
    
    // Read enough bytes to find the flag (date is ~10 chars, plus space)
    // d.span.start points to the start of the directive (the Date).
    let mut buffer = [0u8; 30]; // Read 30 bytes to be safe
    if let Err(e) = file.read_exact(&mut buffer) {
        // If file is too short (EOF), we might just read what's available?
        // But read_exact fails on EOF. 
        // Directives should be longer than 0 bytes.
        // If near EOF, read_exact might fail. Let's try read.
        tracing::warn!("Failed to read exact bytes, trying best effort: {}", e);
        // Retry with just read
    }
    
    // Find * or !
    // Typical format: "2024-01-01 * " -> '*' is at index 11
    let mut flag_pos = None;
    for (i, &b) in buffer.iter().enumerate() {
        if b == b'*' || b == b'!' {
            flag_pos = Some(i);
            break;
        }
        // If we hit a newline, we missed it (or it's a directive without flag?)
        // Transaction always has a flag in standard Beancount (implicit * if omitted? No, usually explicit in file if parsed as such)
        // rustledger parser might normalize. 
        // But we are editing the source file.
        // If the user wrote "2024-01-01 txn", we look for "txn". But let's stick to * / !
        if b == b'\n' {
            break;
        }
    }
    
    if let Some(pos) = flag_pos {
        let actual_offset = payload.offset + pos;
        let new_flag = if buffer[pos] == b'*' { b'!' } else { b'*' };
        
        if let Err(e) = file.seek(SeekFrom::Start(actual_offset as u64)) {
            return Html(format!("<div class='text-red-500'>Error seeking to flag: {}</div>", e));
        }
        if let Err(e) = file.write_all(&[new_flag]) {
             return Html(format!("<div class='text-red-500'>Error writing flag: {}</div>", e));
        }
        
        // Reload to show changes
        Html("<script>window.location.reload()</script>".to_string())
    } else {
        Html("<div class='text-red-500'>Could not find flag (* or !) in the expected position. Only standard transactions are supported for toggling.</div>".to_string())
    }
}

#[derive(Deserialize, Debug)]
struct GetEditFormRequest {
    offset: usize,
    length: usize,
    source_path: String,
}

async fn get_edit_form_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<GetEditFormRequest>,
) -> impl IntoResponse {
    let path = PathBuf::from(&params.source_path);
    let content = match std::fs::read(&path) {
        Ok(c) => c,
        Err(e) => return Html(format!("<div class='p-4 text-red-500'>Error reading file: {}</div>", e)),
    };
    
    // Safety check
    if params.offset + params.length > content.len() {
        return Html("<div class='p-4 text-red-500'>Error: Transaction not found (out of bounds).</div>".to_string());
    }
    
    let txn_bytes = &content[params.offset..(params.offset + params.length)];
    let txn_str = String::from_utf8_lossy(txn_bytes);
    
    let (directives, _) = rustledger_parser::parse_directives(&txn_str);
    if let Some(d) = directives.first() {
        if let Directive::Transaction(txn) = &d.value {
            let mut context = Context::new();
            context.insert("date", &txn.date.to_string());
            context.insert("flag", &txn.flag.to_string());
            context.insert("payee", &txn.payee.clone().unwrap_or_default().to_string());
            context.insert("narration", &txn.narration.to_string());
            
            // Postings
            let postings: Vec<TransactionPosting> = txn.postings.iter().map(|p| {
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
            }).collect();
            context.insert("postings", &postings);
            context.insert("offset", &params.offset);
            context.insert("length", &params.length);
            context.insert("source_path", &params.source_path);
            
            if let Ok(result) = Loader::new().load(&state.ledger_path) {
                let accounts = extract_accounts(&result.directives);
                context.insert("accounts", &accounts);
            }
            
            return match state.tera.render("partials/transaction_edit_form.html", &context) {
                Ok(html) => Html(html),
                Err(e) => Html(format!("<div class='p-4 text-red-500'>Template Error: {}</div>", e)),
            };
        }
    }
    
    Html("<div class='p-4 text-red-500'>Failed to parse transaction for editing.</div>".to_string())
}

async fn update_transaction_handler(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<EditTransactionRequest>,
) -> impl IntoResponse {
    let delete_req = DeleteTransactionRequest {
        offset: payload.original_offset,
        length: payload.original_length,
        source_path: payload.original_source_path.clone(),
    };
    
    let path = PathBuf::from(&delete_req.source_path);
    let mut content = match std::fs::read(&path) {
        Ok(c) => c,
        Err(e) => return Html(format!("<div class='text-red-500'>Error reading file: {}</div>", e)),
    };
    
    let offset = delete_req.offset;
    let length = delete_req.length;
    let end = offset + length;
    
    if offset >= content.len() || end > content.len() {
        return Html("<div class='text-red-500'>Error: Transaction out of bounds.</div>".to_string());
    }
    
    content.drain(offset..end);
    
    let flag = if payload.cleared.is_some() { "*" } else { "!" };
    let payee = if let Some(p) = &payload.payee {
        if !p.is_empty() {
            format!(" \"{}\"", p)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    
    let mut txn_text = format!(
        "\n{} {}{} \"{}\"\n", 
        payload.date, flag, payee, payload.narration
    );
    
    if !payload.account_1.is_empty() {
        let amt = if !payload.amount_1.is_empty() { format!(" {}", payload.amount_1) } else { String::new() };
        txn_text.push_str(&format!("  {}{}\n", payload.account_1, amt));
    }
    
    if let Some(acc2) = &payload.account_2 {
        if !acc2.is_empty() {
             let amt = if let Some(a) = &payload.amount_2 { 
                 if !a.is_empty() { format!(" {}", a) } else { String::new() }
             } else { String::new() };
             txn_text.push_str(&format!("  {}{}\n", acc2, amt));
        }
    }
    
    let new_bytes = txn_text.as_bytes();
    let mut i = offset;
    for b in new_bytes {
        content.insert(i, *b);
        i += 1;
    }
    
    if let Err(e) = std::fs::write(path, content) {
        return Html(format!("<div class='text-red-500'>Error writing file: {}</div>", e));
    }
    
    Html("<div class='bg-green-100 border border-green-400 text-green-700 px-4 py-3 rounded'>Transaction updated! <script>setTimeout(() => window.location.reload(), 1000)</script></div>".to_string())
}

async fn directives_count_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let load_result = Loader::new().load(&state.ledger_path);
    match load_result {
        Ok(result) => result.directives.len().to_string(),
        Err(_) => "0".to_string(),
    }
}
