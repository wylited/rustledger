//! Document links handler for clickable paths.
//!
//! Provides clickable links for:
//! - `include` directive paths
//! - `document` directive paths

use lsp_types::{DocumentLink, DocumentLinkParams, Position, Range, Uri};
use rustledger_core::Directive;
use rustledger_parser::ParseResult;
use std::path::Path;

/// Handle a document links request.
pub fn handle_document_links(
    params: &DocumentLinkParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<Vec<DocumentLink>> {
    let mut links = Vec::new();
    let base_uri = &params.text_document.uri;

    // Get the base directory from the document URI
    let base_dir = get_base_directory(base_uri);

    for spanned in &parse_result.directives {
        if let Directive::Document(doc) = &spanned.value {
            // Create link for document path
            let path_str = doc.path.to_string();
            if let Some(link) =
                create_document_link(source, spanned.span.start, &path_str, &base_dir)
            {
                links.push(link);
            }
        }
    }

    // Also look for include directives in comments/options
    // (includes are typically parsed as options, not directives)
    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("include") {
            if let Some(link) = parse_include_line(line, line_num as u32, &base_dir) {
                links.push(link);
            }
        }
    }

    if links.is_empty() {
        None
    } else {
        Some(links)
    }
}

/// Get the base directory from a file URI.
fn get_base_directory(uri: &Uri) -> Option<String> {
    let uri_str = uri.as_str();
    if let Some(path_str) = uri_str.strip_prefix("file://") {
        let path = Path::new(path_str);
        path.parent().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Create a document link for a path found in source.
fn create_document_link(
    source: &str,
    directive_start: usize,
    path: &str,
    base_dir: &Option<String>,
) -> Option<DocumentLink> {
    let (start_line, _) = byte_offset_to_position(source, directive_start);

    // Find the path in the directive line
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(start_line as usize)?;

    // Find the quoted path
    let quote_start = line.find('"')?;
    let after_quote = &line[quote_start + 1..];
    let quote_end = after_quote.find('"')?;

    let path_in_line = &after_quote[..quote_end];
    if path_in_line != path {
        return None;
    }

    let start_col = (quote_start + 1) as u32;
    let end_col = start_col + path.len() as u32;

    // Resolve the path
    let target_uri = resolve_path_to_uri(path, base_dir)?;

    Some(DocumentLink {
        range: Range {
            start: Position::new(start_line, start_col),
            end: Position::new(start_line, end_col),
        },
        target: Some(target_uri),
        tooltip: Some(format!("Open {}", path)),
        data: None,
    })
}

/// Parse an include line and create a document link.
fn parse_include_line(
    line: &str,
    line_num: u32,
    base_dir: &Option<String>,
) -> Option<DocumentLink> {
    // Match patterns like: include "path/to/file.beancount"
    let trimmed = line.trim();
    if !trimmed.starts_with("include") {
        return None;
    }

    // Find the quoted path
    let quote_start = line.find('"')?;
    let after_quote = &line[quote_start + 1..];
    let quote_end = after_quote.find('"')?;

    let path = &after_quote[..quote_end];
    let start_col = (quote_start + 1) as u32;
    let end_col = start_col + path.len() as u32;

    // Resolve the path
    let target_uri = resolve_path_to_uri(path, base_dir)?;

    Some(DocumentLink {
        range: Range {
            start: Position::new(line_num, start_col),
            end: Position::new(line_num, end_col),
        },
        target: Some(target_uri),
        tooltip: Some(format!("Open {}", path)),
        data: None,
    })
}

/// Resolve a relative path to a file URI.
fn resolve_path_to_uri(path: &str, base_dir: &Option<String>) -> Option<Uri> {
    let resolved = if Path::new(path).is_absolute() {
        path.to_string()
    } else if let Some(ref base) = base_dir {
        let base_path = Path::new(base);
        base_path.join(path).to_string_lossy().to_string()
    } else {
        return None;
    };

    format!("file://{}", resolved).parse().ok()
}

/// Convert a byte offset to a line/column position (0-based for LSP).
fn byte_offset_to_position(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;

    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_include_line() {
        let line = r#"include "accounts.beancount""#;
        let base_dir = Some("/home/user/ledger".to_string());

        let link = parse_include_line(line, 0, &base_dir);
        assert!(link.is_some());

        let link = link.unwrap();
        assert_eq!(link.range.start.character, 9); // After the opening quote
                                                   // "accounts.beancount" is 18 chars, so end is 9 + 18 = 27
        assert_eq!(link.range.end.character, 27);
    }

    #[test]
    fn test_resolve_path_to_uri() {
        let base_dir = Some("/home/user/ledger".to_string());

        let uri = resolve_path_to_uri("accounts.beancount", &base_dir);
        assert!(uri.is_some());
        assert!(uri.unwrap().as_str().contains("accounts.beancount"));
    }
}
