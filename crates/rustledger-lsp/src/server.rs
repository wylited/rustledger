//! Main LSP server implementation.

use crate::handlers::semantic_tokens::get_capabilities as get_semantic_tokens_capabilities;
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
        completion_provider: Some(lsp_types::CompletionOptions {
            trigger_characters: Some(vec![
                ":".to_string(),  // Account segments
                " ".to_string(),  // After keywords
                "\"".to_string(), // Strings (payees, narrations)
            ]),
            ..Default::default()
        }),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        document_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        semantic_tokens_provider: Some(get_semantic_tokens_capabilities()),
        code_action_provider: Some(lsp_types::CodeActionProviderCapability::Simple(true)),
        workspace_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        rename_provider: Some(lsp_types::OneOf::Right(lsp_types::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        document_formatting_provider: Some(lsp_types::OneOf::Left(true)),
        document_range_formatting_provider: Some(lsp_types::OneOf::Left(true)),
        document_link_provider: Some(lsp_types::DocumentLinkOptions {
            resolve_provider: Some(false),
            work_done_progress_options: Default::default(),
        }),
        inlay_hint_provider: Some(lsp_types::OneOf::Left(true)),
        selection_range_provider: Some(lsp_types::SelectionRangeProviderCapability::Simple(true)),
        folding_range_provider: Some(lsp_types::FoldingRangeProviderCapability::Simple(true)),
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
