//! Main LSP server implementation.

use crate::main_loop::run_main_loop;
use lsp_server::Connection;
use lsp_types::InitializeParams;

/// The LSP server.
pub struct Server {
    /// Connection to the LSP client.
    connection: Connection,
    /// Initialize parameters from client.
    init_params: InitializeParams,
}

impl Server {
    /// Create a new LSP server from a connection.
    pub fn new(connection: Connection, init_params: InitializeParams) -> Self {
        Self {
            connection,
            init_params,
        }
    }

    /// Run the server's main loop.
    pub fn run(self) {
        tracing::info!("Starting Beancount Language Server v{}", crate::VERSION);

        if let Some(folders) = &self.init_params.workspace_folders {
            if let Some(folder) = folders.first() {
                tracing::info!("Workspace root: {}", folder.uri.as_str());
            }
        }

        // Run the main event loop
        let (sender, receiver) = (self.connection.sender, self.connection.receiver);
        run_main_loop(receiver, sender);

        tracing::info!("Server shutdown complete");
    }
}

/// Start the LSP server using stdio transport.
pub fn start_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("Starting LSP server on stdio");

    // Create connection using stdio
    let (connection, io_threads) = Connection::stdio();

    // Wait for initialize request
    let (id, params) = connection.initialize_start()?;
    let init_params: InitializeParams = serde_json::from_value(params)?;

    // Build server capabilities
    let capabilities = lsp_types::ServerCapabilities {
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
            lsp_types::TextDocumentSyncKind::FULL,
        )),
        ..Default::default()
    };

    let server_info = lsp_types::ServerInfo {
        name: "rledger-lsp".to_string(),
        version: Some(crate::VERSION.to_string()),
    };

    let init_result = lsp_types::InitializeResult {
        capabilities,
        server_info: Some(server_info),
    };

    // Complete initialization handshake
    connection.initialize_finish(id, serde_json::to_value(init_result)?)?;

    tracing::info!("LSP initialized successfully");

    // Create and run server
    let server = Server::new(connection, init_params);
    server.run();

    // Wait for IO threads to finish
    io_threads.join()?;

    Ok(())
}
