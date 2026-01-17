//! LSP request and notification handlers.
//!
//! Each handler processes a specific LSP request type against
//! an immutable world snapshot.

pub mod code_actions;
pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod document_links;
pub mod folding;
pub mod formatting;
pub mod hover;
pub mod inlay_hints;
pub mod range_formatting;
pub mod references;
pub mod rename;
pub mod selection_range;
pub mod semantic_tokens;
pub mod symbols;
pub mod workspace_symbols;
