//! Language Server Protocol implementation for Beancount.
//!
//! This crate provides an LSP server for Beancount files, enabling IDE features like:
//! - Real-time syntax error diagnostics
//! - Autocompletion for accounts, currencies, payees
//! - Go-to-definition for accounts
//! - Hover information
//! - Document symbols (outline view)
//!
//! # Architecture
//!
//! The server follows rust-analyzer's architecture:
//! - **Main loop**: Handles LSP messages, applies changes, dispatches requests
//! - **Query database**: Salsa-inspired incremental computation
//! - **Handlers**: Process LSP requests against immutable snapshots
//!
//! # Example
//!
//! ```ignore
//! use rustledger_lsp::Server;
//!
//! #[tokio::main]
//! async fn main() {
//!     Server::new().run().await;
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod db;
pub mod handlers;
pub mod main_loop;

mod server;
mod snapshot;
mod vfs;

pub use main_loop::run_main_loop;
pub use server::{Server, start_stdio};
pub use snapshot::Snapshot;
pub use vfs::Vfs;

/// LSP server version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
