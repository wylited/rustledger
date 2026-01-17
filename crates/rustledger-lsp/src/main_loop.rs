//! Main event loop for the LSP server.
//!
//! Follows rust-analyzer's architecture:
//! - Notifications handled synchronously (critical for correctness)
//! - Requests dispatched to threadpool with immutable snapshots
//! - Revision counter enables cancellation of stale requests

use crate::handlers::code_actions::handle_code_actions;
use crate::handlers::completion::handle_completion;
use crate::handlers::definition::handle_goto_definition;
use crate::handlers::diagnostics::parse_errors_to_diagnostics;
use crate::handlers::document_links::handle_document_links;
use crate::handlers::folding::handle_folding_ranges;
use crate::handlers::formatting::handle_formatting;
use crate::handlers::hover::handle_hover;
use crate::handlers::inlay_hints::handle_inlay_hints;
use crate::handlers::range_formatting::handle_range_formatting;
use crate::handlers::rename::{handle_prepare_rename, handle_rename};
use crate::handlers::selection_range::handle_selection_range;
use crate::handlers::semantic_tokens::handle_semantic_tokens;
use crate::handlers::symbols::handle_document_symbols;
use crate::handlers::workspace_symbols::handle_workspace_symbols;
use crate::snapshot::bump_revision;
use crate::vfs::Vfs;
use crossbeam_channel::{Receiver, Sender};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification,
    PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentLinkRequest, DocumentSymbolRequest, FoldingRangeRequest,
    Formatting, GotoDefinition, HoverRequest, Initialize, InlayHintRequest, PrepareRenameRequest,
    RangeFormatting, Rename, Request, SelectionRangeRequest, SemanticTokensFullRequest, Shutdown,
    WorkspaceSymbolRequest,
};
use lsp_types::{
    CodeActionParams, CompletionParams, DiagnosticOptions, DiagnosticServerCapabilities,
    DocumentFormattingParams, DocumentLinkParams, DocumentRangeFormattingParams,
    DocumentSymbolParams, FoldingRangeParams, GotoDefinitionParams, HoverParams, InitializeParams,
    InitializeResult, InlayHintParams, PublishDiagnosticsParams, RenameParams,
    SelectionRangeParams, SemanticTokensParams, ServerCapabilities, ServerInfo,
    TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
    WorkspaceSymbolParams,
};
use parking_lot::RwLock;
use rustledger_parser::parse;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Convert a URI to a file path.
#[cfg(not(windows))]
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    uri.as_str().strip_prefix("file://").map(PathBuf::from)
}

/// Convert a URI to a file path (Windows version).
#[cfg(windows)]
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    uri.as_str()
        .strip_prefix("file://")
        // Handle Windows paths like file:///C:/...
        .map(|p| p.strip_prefix('/').unwrap_or(p))
        .map(PathBuf::from)
}

/// Events processed by the main loop.
#[derive(Debug)]
pub enum Event {
    /// LSP message from the client.
    Message(Message),
    /// Response from a background task.
    #[allow(dead_code)] // Will be used when we add threadpool
    Task(TaskResult),
}

/// LSP message types.
#[derive(Debug)]
pub enum Message {
    /// Request from client (expects response).
    Request(lsp_server::Request),
    /// Notification from client (no response).
    Notification(lsp_server::Notification),
    /// Response from client (for server-initiated requests).
    Response(lsp_server::Response),
}

/// Result from a background task.
#[derive(Debug)]
#[allow(dead_code)] // Will be used when we add threadpool
pub struct TaskResult {
    /// The request ID this task is responding to.
    pub request_id: lsp_server::RequestId,
    /// The result of the task, or an error message.
    pub result: Result<serde_json::Value, String>,
}

/// State managed by the main loop.
pub struct MainLoopState {
    /// Virtual file system for open documents.
    pub vfs: Arc<RwLock<Vfs>>,
    /// Sender for outgoing LSP messages.
    pub sender: Sender<lsp_server::Message>,
    /// Cached diagnostics per file.
    pub diagnostics: HashMap<Uri, Vec<lsp_types::Diagnostic>>,
    /// Whether shutdown was requested.
    pub shutdown_requested: bool,
}

impl MainLoopState {
    /// Create a new main loop state.
    pub fn new(sender: Sender<lsp_server::Message>) -> Self {
        Self {
            vfs: Arc::new(RwLock::new(Vfs::new())),
            sender,
            diagnostics: HashMap::new(),
            shutdown_requested: false,
        }
    }

    /// Handle an incoming event.
    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Message(msg) => self.handle_message(msg),
            Event::Task(_result) => {
                // TODO: Send response back to client
            }
        }
    }

    /// Handle an LSP message.
    fn handle_message(&mut self, msg: Message) {
        match msg {
            Message::Request(req) => self.handle_request(req),
            Message::Notification(notif) => self.handle_notification(notif),
            Message::Response(_resp) => {
                // We don't currently send requests to the client
            }
        }
    }

    /// Handle an LSP request (expects response).
    fn handle_request(&mut self, req: lsp_server::Request) {
        let id = req.id.clone();

        // Dispatch based on method
        let result = match req.method.as_str() {
            Initialize::METHOD => self.handle_initialize(req),
            Shutdown::METHOD => {
                self.shutdown_requested = true;
                Ok(serde_json::Value::Null)
            }
            Completion::METHOD => self.handle_completion_request(req),
            GotoDefinition::METHOD => self.handle_goto_definition_request(req),
            HoverRequest::METHOD => self.handle_hover_request(req),
            DocumentSymbolRequest::METHOD => self.handle_document_symbols_request(req),
            SemanticTokensFullRequest::METHOD => self.handle_semantic_tokens_request(req),
            CodeActionRequest::METHOD => self.handle_code_action_request(req),
            WorkspaceSymbolRequest::METHOD => self.handle_workspace_symbol_request(req),
            PrepareRenameRequest::METHOD => self.handle_prepare_rename_request(req),
            Rename::METHOD => self.handle_rename_request(req),
            Formatting::METHOD => self.handle_formatting_request(req),
            RangeFormatting::METHOD => self.handle_range_formatting_request(req),
            DocumentLinkRequest::METHOD => self.handle_document_link_request(req),
            InlayHintRequest::METHOD => self.handle_inlay_hint_request(req),
            SelectionRangeRequest::METHOD => self.handle_selection_range_request(req),
            FoldingRangeRequest::METHOD => self.handle_folding_range_request(req),
            _ => {
                tracing::warn!("Unhandled request: {}", req.method);
                Err(format!("Unhandled request: {}", req.method))
            }
        };

        // Send response
        let response = match result {
            Ok(value) => lsp_server::Response::new_ok(id, value),
            Err(msg) => {
                lsp_server::Response::new_err(id, lsp_server::ErrorCode::MethodNotFound as i32, msg)
            }
        };

        self.send(lsp_server::Message::Response(response));
    }

    /// Handle the initialize request.
    fn handle_initialize(&mut self, req: lsp_server::Request) -> Result<serde_json::Value, String> {
        let _params: InitializeParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                ..Default::default()
            })),
            ..Default::default()
        };

        let result = InitializeResult {
            capabilities,
            server_info: Some(ServerInfo {
                name: "rledger-lsp".to_string(),
                version: Some(crate::VERSION.to_string()),
            }),
        };

        serde_json::to_value(result).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/completion request.
    fn handle_completion_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: CompletionParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document_position.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle completion
        let response = handle_completion(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/definition request.
    fn handle_goto_definition_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: GotoDefinitionParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document_position_params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle go-to-definition
        let response = handle_goto_definition(&params, &text, &parse_result, uri);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/hover request.
    fn handle_hover_request(&self, req: lsp_server::Request) -> Result<serde_json::Value, String> {
        let params: HoverParams = serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document_position_params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle hover
        let response = handle_hover(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/documentSymbol request.
    fn handle_document_symbols_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: DocumentSymbolParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle document symbols
        let response = handle_document_symbols(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/semanticTokens/full request.
    fn handle_semantic_tokens_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: SemanticTokensParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle semantic tokens
        let response = handle_semantic_tokens(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/codeAction request.
    fn handle_code_action_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: CodeActionParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle code actions
        let response = handle_code_actions(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the workspace/symbol request.
    fn handle_workspace_symbol_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: WorkspaceSymbolParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        // Collect all open documents
        let vfs = self.vfs.read();
        let mut documents = Vec::new();

        for (path, content) in vfs.iter() {
            let uri_str = format!("file://{}", path.display());
            if let Ok(uri) = uri_str.parse() {
                let parse_result = parse(&content);
                documents.push((uri, content, parse_result));
            }
        }

        let response = handle_workspace_symbols(&params, &documents);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/prepareRename request.
    fn handle_prepare_rename_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: TextDocumentPositionParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle prepare rename
        let response = handle_prepare_rename(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/rename request.
    fn handle_rename_request(&self, req: lsp_server::Request) -> Result<serde_json::Value, String> {
        let params: RenameParams = serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document_position.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle rename
        let response = handle_rename(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/formatting request.
    fn handle_formatting_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: DocumentFormattingParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle formatting
        let response = handle_formatting(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/foldingRange request.
    fn handle_folding_range_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: FoldingRangeParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle folding ranges
        let response = handle_folding_ranges(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/rangeFormatting request.
    fn handle_range_formatting_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: DocumentRangeFormattingParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle range formatting
        let response = handle_range_formatting(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/documentLink request.
    fn handle_document_link_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: DocumentLinkParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle document links
        let response = handle_document_links(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/inlayHint request.
    fn handle_inlay_hint_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: InlayHintParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle inlay hints
        let response = handle_inlay_hints(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle the textDocument/selectionRange request.
    fn handle_selection_range_request(
        &self,
        req: lsp_server::Request,
    ) -> Result<serde_json::Value, String> {
        let params: SelectionRangeParams =
            serde_json::from_value(req.params).map_err(|e| e.to_string())?;

        let uri = &params.text_document.uri;

        // Get document content from VFS
        let text = if let Some(path) = uri_to_path(uri) {
            self.vfs.read().get_content(&path).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse the document
        let parse_result = parse(&text);

        // Handle selection range
        let response = handle_selection_range(&params, &text, &parse_result);

        serde_json::to_value(response).map_err(|e| e.to_string())
    }

    /// Handle an LSP notification (no response expected).
    fn handle_notification(&mut self, notif: lsp_server::Notification) {
        // Notifications are handled synchronously - this is critical for correctness
        match notif.method.as_str() {
            DidOpenTextDocument::METHOD => {
                if let Ok(params) =
                    serde_json::from_value::<lsp_types::DidOpenTextDocumentParams>(notif.params)
                {
                    self.on_did_open(params);
                }
            }
            DidChangeTextDocument::METHOD => {
                if let Ok(params) =
                    serde_json::from_value::<lsp_types::DidChangeTextDocumentParams>(notif.params)
                {
                    self.on_did_change(params);
                }
            }
            DidCloseTextDocument::METHOD => {
                if let Ok(params) =
                    serde_json::from_value::<lsp_types::DidCloseTextDocumentParams>(notif.params)
                {
                    self.on_did_close(params);
                }
            }
            "initialized" => {
                tracing::info!("Client initialized");
            }
            "exit" => {
                tracing::info!("Exit notification received");
                std::process::exit(if self.shutdown_requested { 0 } else { 1 });
            }
            _ => {
                tracing::debug!("Unhandled notification: {}", notif.method);
            }
        }
    }

    /// Handle textDocument/didOpen notification.
    fn on_did_open(&mut self, params: lsp_types::DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let version = params.text_document.version;

        tracing::info!("Document opened: {}", uri.as_str());

        // Store in VFS
        if let Some(path) = uri_to_path(&uri) {
            self.vfs.write().open(path, text.clone(), version);
        }

        // Bump revision (invalidates any in-flight requests)
        bump_revision();

        // Compute and publish diagnostics
        self.publish_diagnostics(&uri, &text);
    }

    /// Handle textDocument/didChange notification.
    fn on_did_change(&mut self, params: lsp_types::DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // For full sync, take the last change (which is the full content)
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text;

            tracing::debug!("Document changed: {}", uri.as_str());

            // Update VFS
            if let Some(path) = uri_to_path(&uri) {
                self.vfs.write().update(&path, text.clone(), version);
            }

            // Bump revision
            bump_revision();

            // Recompute diagnostics
            self.publish_diagnostics(&uri, &text);
        }
    }

    /// Handle textDocument/didClose notification.
    fn on_did_close(&mut self, params: lsp_types::DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        tracing::info!("Document closed: {}", uri.as_str());

        // Remove from VFS
        if let Some(path) = uri_to_path(&uri) {
            self.vfs.write().close(&path);
        }

        // Clear diagnostics
        self.diagnostics.remove(&uri);
        self.send_diagnostics(&uri, vec![]);
    }

    /// Parse document and publish diagnostics.
    fn publish_diagnostics(&mut self, uri: &Uri, text: &str) {
        // Parse the document
        let result = parse(text);

        // Convert errors to LSP diagnostics
        let diagnostics = parse_errors_to_diagnostics(&result, text);

        tracing::debug!(
            "Publishing {} diagnostics for {}",
            diagnostics.len(),
            uri.as_str()
        );

        // Cache and send
        self.diagnostics.insert(uri.clone(), diagnostics.clone());
        self.send_diagnostics(uri, diagnostics);
    }

    /// Send diagnostics to the client.
    fn send_diagnostics(&self, uri: &Uri, diagnostics: Vec<lsp_types::Diagnostic>) {
        let params = PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics,
            version: None,
        };

        let notif = lsp_server::Notification::new(PublishDiagnostics::METHOD.to_string(), params);

        self.send(lsp_server::Message::Notification(notif));
    }

    /// Send a message to the client.
    fn send(&self, msg: lsp_server::Message) {
        if let Err(e) = self.sender.send(msg) {
            tracing::error!("Failed to send message: {}", e);
        }
    }
}

/// Run the main event loop.
pub fn run_main_loop(receiver: Receiver<lsp_server::Message>, sender: Sender<lsp_server::Message>) {
    let mut state = MainLoopState::new(sender);

    tracing::info!("Main loop started");

    for msg in receiver {
        let event = match msg {
            lsp_server::Message::Request(req) => Event::Message(Message::Request(req)),
            lsp_server::Message::Notification(notif) => {
                Event::Message(Message::Notification(notif))
            }
            lsp_server::Message::Response(resp) => Event::Message(Message::Response(resp)),
        };

        state.handle_event(event);
    }

    tracing::info!("Main loop ended");
}
