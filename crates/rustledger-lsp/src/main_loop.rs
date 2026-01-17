//! Main event loop for the LSP server.
//!
//! Follows rust-analyzer's architecture:
//! - Notifications handled synchronously (critical for correctness)
//! - Requests dispatched to threadpool with immutable snapshots
//! - Revision counter enables cancellation of stale requests

use crate::handlers::diagnostics::parse_errors_to_diagnostics;
use crate::snapshot::bump_revision;
use crate::vfs::Vfs;
use crossbeam_channel::{Receiver, Sender};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification,
    PublishDiagnostics,
};
use lsp_types::request::{Initialize, Request, Shutdown};
use lsp_types::{
    DiagnosticOptions, DiagnosticServerCapabilities, InitializeParams, InitializeResult,
    PublishDiagnosticsParams, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
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
