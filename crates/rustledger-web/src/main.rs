mod handlers;
mod models;
mod utils;
mod file_watcher;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use clap::Parser;
use tera::Tera;
use tokio::sync::{Mutex, RwLock, mpsc};
use tower_http::services::ServeDir;

use crate::handlers::AppState;
use crate::file_watcher::spawn_file_watcher;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the ledger file
    #[arg(default_value = "main.beancount")]
    ledger_file: PathBuf,

    /// Port to listen on
    #[arg(short, long, default_value_t = 3000)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // Check if ledger file exists
    if !args.ledger_file.exists() {
        eprintln!(
            "Error: Ledger file '{}' not found.",
            args.ledger_file.display()
        );
        std::process::exit(1);
    }

    // Initialize Tera templates
    // Use CARGO_MANIFEST_DIR to find templates relative to the crate
    let template_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*");
    let mut tera = Tera::new(template_dir)
        .or_else(|_| Tera::new("templates/**/*"))
        .or_else(|_| Tera::new("crates/rustledger-web/templates/**/*"))?;

    // Disable auto-escaping for HTML content if needed
    tera.autoescape_on(vec![".html", ".sql"]);

    // Channel for file change notifications
    let (file_change_tx, mut file_change_rx) = mpsc::channel(10);

    // Shared state with caching and write synchronization
    let state = Arc::new(AppState {
        ledger_path: args.ledger_file.clone(),
        tera,
        cached_ledger: RwLock::new(None),
        write_lock: Mutex::new(()),
        file_change_tx,
    });

    // Spawn file watcher
    let watcher_state = Arc::clone(&state);
    spawn_file_watcher(watcher_state)?;

    // Spawn background task to handle file change notifications
    let notification_state = Arc::clone(&state);
    tokio::spawn(async move {
        while let Some(_change) = file_change_rx.recv().await {
            // Invalidate cache when file changes externally
            let mut cache = notification_state.cached_ledger.write().await;
            *cache = None;
            tracing::info!("Ledger file changed externally, cache invalidated");
        }
    });

    // Build router
    let app = Router::new()
        .route("/", get(handlers::index))
        .route("/transactions", get(handlers::transactions_page))
        .route("/add", get(handlers::add_transaction_page))
        .route("/accounts", get(handlers::accounts_page))
        .route("/accounts/*account", get(handlers::account_detail))
        .route("/api/transactions", post(handlers::create_transaction))
        .route(
            "/api/transactions/toggle-status",
            post(handlers::toggle_status),
        )
        .route(
            "/api/transactions/delete",
            post(handlers::delete_transaction),
        )
        .route("/api/transactions/edit-form", get(handlers::get_edit_form))
        .route(
            "/api/transactions/update",
            post(handlers::update_transaction),
        )
        .route("/api/accounts/open", post(handlers::open_account))
        .route("/api/accounts/close", post(handlers::close_account))
        .route("/api/payees", get(handlers::get_payees))
        .route("/api/stats/net-worth", get(handlers::get_net_worth_stats))
        .route(
            "/api/stats/income-expenses",
            get(handlers::get_income_expense_stats),
        )
        .route("/api/stats/cash-flow", get(handlers::get_cash_flow))
        .route(
            "/api/stats/net-worth-history",
            get(handlers::get_net_worth_history),
        )
        .route("/api/query", post(handlers::execute_query))
        .route("/api/recent-transactions", get(handlers::get_recent_transactions))
        .route("/api/accounts", get(handlers::get_accounts_list))
        .nest_service(
            "/assets",
            ServeDir::new("assets").fallback(ServeDir::new("crates/rustledger-web/assets")),
        )
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    println!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
