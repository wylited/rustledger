//! LSP request and notification handlers.
//!
//! Each handler processes a specific LSP request type against
//! an immutable world snapshot.

pub mod utils;

pub mod call_hierarchy;
pub mod code_actions;
pub mod code_lens;
pub mod completion;
pub mod completion_resolve;
pub mod declaration;
pub mod definition;
pub mod diagnostics;
pub mod document_color;
pub mod document_highlight;
pub mod document_links;
pub mod execute_command;
pub mod folding;
pub mod formatting;
pub mod hover;
pub mod inlay_hints;
pub mod linked_editing;
pub mod on_type_formatting;
pub mod range_formatting;
pub mod references;
pub mod rename;
pub mod selection_range;
pub mod semantic_tokens;
pub mod signature_help;
pub mod symbols;
pub mod type_hierarchy;
pub mod workspace_symbols;
