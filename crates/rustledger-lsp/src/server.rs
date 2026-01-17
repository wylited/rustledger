//! Main LSP server implementation.

use crate::vfs::Vfs;
use parking_lot::RwLock;
use std::sync::Arc;

/// The LSP server.
pub struct Server {
    /// Virtual file system for document management.
    #[allow(dead_code)] // WIP: will be used when LSP handlers are implemented
    vfs: Arc<RwLock<Vfs>>,
}

impl Server {
    /// Create a new LSP server.
    pub fn new() -> Self {
        Self {
            vfs: Arc::new(RwLock::new(Vfs::new())),
        }
    }

    /// Run the server, reading from stdin and writing to stdout.
    pub async fn run(self) {
        tracing::info!("Starting Beancount Language Server v{}", crate::VERSION);

        // TODO: Implement LSP main loop
        // 1. Set up async-lsp server
        // 2. Register capabilities
        // 3. Handle requests/notifications
        // 4. Publish diagnostics

        tracing::warn!("LSP server not yet implemented");
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
