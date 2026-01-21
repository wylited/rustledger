mod handlers;
mod models;
mod utils;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use clap::Parser;
use tera::Tera;
use tower_http::services::ServeDir;

use crate::handlers::AppState;

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
    // Adjust path to work relative to the workspace root or crate root
    let mut tera = Tera::new("templates/**/*")
        .or_else(|_| Tera::new("crates/rustledger-web/templates/**/*"))?;

    // Disable auto-escaping for HTML content if needed
    tera.autoescape_on(vec![".html", ".sql"]);

    // Shared state
    let state = Arc::new(AppState {
        ledger_path: args.ledger_file.clone(),
        tera,
    });

    // Build router
    let app = Router::new()
        .route("/", get(handlers::index))
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
        .nest_service(
            "/assets",
            ServeDir::new("assets").fallback(ServeDir::new("crates/rustledger-web/assets")),
        )
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    println!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
